use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, bail};
use gix::ObjectId;

const RECORD: char = '\u{1e}';
const FIELD: char = '\u{1f}';
const FORMAT: &str = "--format=%x1e%H%x1f%at%x1f%an%x1f%ae%x1f%s";
const FIRST_BATCH: usize = 40;
const BATCH: usize = 400;
const POLL: Duration = Duration::from_millis(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: ObjectId,
    pub time: i64,
    pub author: String,
    pub email: String,
    pub title: String,
    pub path: String,
    pub renamed_from: Option<String>,
    pub added: Option<u32>,
    pub removed: Option<u32>,
}

pub fn parse(text: &str) -> Vec<Entry> {
    text.split(RECORD).filter_map(parse_record).collect()
}

fn parse_record(record: &str) -> Option<Entry> {
    let mut lines = record.lines();
    let header = lines.next()?;
    let mut fields = header.split(FIELD);
    let id = ObjectId::from_hex(fields.next()?.as_bytes()).ok()?;
    let time = fields.next()?.parse().ok()?;
    let author = fields.next()?.to_string();
    let email = fields.next()?.to_string();
    let title = fields.next().unwrap_or_default().to_string();
    let stat = lines.find(|l| l.contains('\t'))?;
    let mut columns = stat.splitn(3, '\t');
    let added = columns.next()?.parse().ok();
    let removed = columns.next()?.parse().ok();
    let (renamed_from, path) = split_rename(columns.next()?);
    Some(Entry {
        id,
        time,
        author,
        email,
        title,
        path,
        renamed_from,
        added,
        removed,
    })
}

fn split_rename(spec: &str) -> (Option<String>, String) {
    if let (Some(open), Some(close)) = (spec.find('{'), spec.rfind('}'))
        && open < close
        && let Some((old, new)) = spec[open + 1..close].split_once(" => ")
    {
        let (prefix, suffix) = (&spec[..open], &spec[close + 1..]);
        let join = |middle: &str| {
            format!("{prefix}{middle}{suffix}")
                .replace("//", "/")
                .trim_start_matches('/')
                .to_string()
        };
        return (Some(join(old)), join(new));
    }
    match spec.split_once(" => ") {
        Some((old, new)) => (Some(old.to_string()), new.to_string()),
        None => (None, spec.to_string()),
    }
}

pub fn stream(
    dir: &Path,
    path: &str,
    cancel: &AtomicBool,
    mut on_batch: impl FnMut(Vec<Entry>),
) -> anyhow::Result<()> {
    let mut child = Command::new("git")
        .current_dir(dir)
        .args(["log", "--follow", "-M", "--numstat", FORMAT, "--"])
        .arg(path)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("could not start git log")?;
    let stdout = child.stdout.take().context("git log has no output")?;
    let stderr = child.stderr.take();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for chunk in BufReader::new(stdout).split(RECORD as u8) {
            let Ok(chunk) = chunk else { break };
            if tx
                .send(String::from_utf8_lossy(&chunk).into_owned())
                .is_err()
            {
                break;
            }
        }
    });
    let mut batch = Vec::new();
    let mut sent_first = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(());
        }
        match rx.recv_timeout(POLL) {
            Ok(record) => {
                batch.extend(parse_record(&record));
                let size = if sent_first { BATCH } else { FIRST_BATCH };
                if batch.len() >= size {
                    on_batch(std::mem::take(&mut batch));
                    sent_first = true;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !batch.is_empty() {
                    on_batch(std::mem::take(&mut batch));
                    sent_first = true;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    if !batch.is_empty() {
        on_batch(batch);
    }
    let status = child.wait()?;
    if !status.success() {
        let mut message = String::new();
        if let Some(mut stderr) = stderr {
            let _ = std::io::Read::read_to_string(&mut stderr, &mut message);
        }
        bail!("git log failed: {}", message.trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "1111111111111111111111111111111111111111";
    const B: &str = "2222222222222222222222222222222222222222";
    const C: &str = "3333333333333333333333333333333333333333";

    fn sample() -> String {
        format!(
            "\u{1e}{A}\u{1f}300\u{1f}Maya\u{1f}maya@example.com\u{1f}Tweak the parser\n\n4\t1\tsrc/parse.rs\n\
             \u{1e}{B}\u{1f}200\u{1f}Sam\u{1f}sam@example.com\u{1f}Move the parser\n\n0\t0\tsrc/{{lexer.rs => parse.rs}}\n\
             \u{1e}{C}\u{1f}100\u{1f}Maya\u{1f}maya@example.com\u{1f}Add a lexer\n\n12\t0\tlexer.rs\n"
        )
    }

    #[test]
    fn records_parse_with_counts_and_paths() {
        let entries = parse(&sample());
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].title, "Tweak the parser");
        assert_eq!(entries[0].path, "src/parse.rs");
        assert_eq!((entries[0].added, entries[0].removed), (Some(4), Some(1)));
        assert_eq!(entries[1].renamed_from.as_deref(), Some("src/lexer.rs"));
        assert_eq!(entries[1].path, "src/parse.rs");
        assert_eq!(entries[2].path, "lexer.rs");
        assert_eq!(entries[2].time, 100);
    }

    #[test]
    fn rename_forms() {
        assert_eq!(
            split_rename("a.rs => b.rs"),
            (Some("a.rs".into()), "b.rs".into())
        );
        assert_eq!(
            split_rename("src/{old => new}/mod.rs"),
            (Some("src/old/mod.rs".into()), "src/new/mod.rs".into())
        );
        assert_eq!(
            split_rename("{ => nested}/file.rs"),
            (Some("file.rs".into()), "nested/file.rs".into())
        );
        assert_eq!(split_rename("plain.rs"), (None, "plain.rs".into()));
    }

    #[test]
    fn binary_files_have_no_counts() {
        let text = format!("\u{1e}{A}\u{1f}1\u{1f}M\u{1f}m@x\u{1f}Logo\n\n-\t-\tlogo.png\n");
        let entries = parse(&text);
        assert_eq!(entries[0].added, None);
        assert_eq!(entries[0].path, "logo.png");
    }
}
