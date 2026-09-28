use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, bail};
use gix::ObjectId;

const POLL: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub id: ObjectId,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub summary: String,
    pub boundary: bool,
    pub previous: Option<(ObjectId, String)>,
}

impl Origin {
    pub fn is_uncommitted(&self) -> bool {
        self.id.is_null()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub origin: usize,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Blame {
    pub origins: Vec<Origin>,
    pub lines: Vec<Line>,
}

impl Blame {
    pub fn origin(&self, line: usize) -> Option<&Origin> {
        self.lines.get(line).map(|l| &self.origins[l.origin])
    }

    pub fn starts_block(&self, line: usize) -> bool {
        line == 0
            || self.lines.get(line).map(|l| l.origin) != self.lines.get(line - 1).map(|l| l.origin)
    }
}

pub fn parse(text: &str) -> anyhow::Result<Blame> {
    let mut blame = Blame::default();
    let mut index: HashMap<ObjectId, usize> = HashMap::new();
    let mut current: Option<usize> = None;
    for line in text.lines() {
        if let Some(content) = line.strip_prefix('\t') {
            let origin = current.context("blame line before its header")?;
            blame.lines.push(Line {
                origin,
                text: content.to_string(),
            });
            continue;
        }
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        if key.len() == 40
            && let Ok(id) = ObjectId::from_hex(key.as_bytes())
        {
            let at = *index.entry(id).or_insert_with(|| {
                blame.origins.push(Origin {
                    id,
                    author: String::new(),
                    email: String::new(),
                    time: 0,
                    summary: String::new(),
                    boundary: false,
                    previous: None,
                });
                blame.origins.len() - 1
            });
            current = Some(at);
            continue;
        }
        let Some(at) = current else { continue };
        let origin = &mut blame.origins[at];
        match key {
            "author" => origin.author = value.to_string(),
            "author-mail" => {
                origin.email = value
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_string()
            }
            "author-time" => origin.time = value.parse().unwrap_or(0),
            "summary" => origin.summary = value.to_string(),
            "boundary" => origin.boundary = true,
            "previous" => {
                origin.previous = value.split_once(' ').and_then(|(sha, path)| {
                    ObjectId::from_hex(sha.as_bytes())
                        .ok()
                        .map(|id| (id, path.to_string()))
                })
            }
            _ => {}
        }
    }
    Ok(blame)
}

pub fn run(
    dir: &Path,
    rev: Option<ObjectId>,
    path: &str,
    cancel: &AtomicBool,
) -> anyhow::Result<Option<Blame>> {
    let mut command = Command::new("git");
    command.current_dir(dir).args(["blame", "--porcelain"]);
    if let Some(rev) = rev {
        command.arg(rev.to_string());
    }
    let mut child = command
        .arg("--")
        .arg(path)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("could not start git blame")?;
    let mut stdout = child.stdout.take().context("git blame has no output")?;
    let mut stderr = child
        .stderr
        .take()
        .context("git blame has no errors pipe")?;
    let output = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(POLL);
    };
    let bytes = output.join().unwrap_or_default();
    if !status.success() {
        bail!(
            "git blame failed: {}",
            errors.join().unwrap_or_default().trim()
        );
    }
    parse(&String::from_utf8_lossy(&bytes)).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const ZERO: &str = "0000000000000000000000000000000000000000";

    fn porcelain() -> String {
        format!(
            "{A} 1 1 2\nauthor Maya Lindqvist\nauthor-mail <maya@example.com>\nauthor-time 1700000000\nauthor-tz +0100\nsummary Add the list\nboundary\nfilename list.rs\n\tfn list() {{\n\
             {A} 2 2\n\t    let a = 1;\n\
             {B} 3 3 1\nauthor Sam Whitaker\nauthor-mail <sam@example.com>\nauthor-time 1700100000\nsummary Count items\nprevious {A} list.rs\nfilename list.rs\n\t    let count = 2;\n\
             {ZERO} 4 4 1\nauthor Not Committed Yet\nauthor-mail <not.committed.yet>\nauthor-time 1700200000\nsummary Version of list.rs from list.rs\nfilename list.rs\n\t    todo();\n\
             {A} 5 5 1\n\t}}\n"
        )
    }

    #[test]
    fn porcelain_parses_blocks_boundaries_and_uncommitted_lines() {
        let blame = parse(&porcelain()).unwrap();
        assert_eq!(blame.lines.len(), 5);
        assert_eq!(blame.origins.len(), 3);
        let first = blame.origin(0).unwrap();
        assert_eq!(first.author, "Maya Lindqvist");
        assert_eq!(first.email, "maya@example.com");
        assert!(first.boundary);
        assert_eq!(blame.origin(1).unwrap().id, first.id);
        let second = blame.origin(2).unwrap();
        assert_eq!(second.summary, "Count items");
        assert_eq!(
            second.previous.as_ref().map(|(_, p)| p.as_str()),
            Some("list.rs")
        );
        assert!(blame.origin(3).unwrap().is_uncommitted());
        assert_eq!(blame.origin(4).unwrap().id, first.id);
        assert_eq!(blame.lines[4].text, "}");
    }

    #[test]
    fn blocks_start_where_the_commit_changes() {
        let blame = parse(&porcelain()).unwrap();
        let starts: Vec<bool> = (0..5).map(|i| blame.starts_block(i)).collect();
        assert_eq!(starts, [true, false, true, true, true]);
    }
}
