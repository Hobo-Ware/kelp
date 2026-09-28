use std::cell::Cell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

pub const CAPACITY: usize = 500;
pub const OUTPUT_CAP: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Action,
    Background,
}

#[derive(Debug, Clone)]
pub struct Output {
    pub text: String,
    pub truncated: bool,
}

impl Output {
    pub fn capture(bytes: &[u8]) -> Self {
        let truncated = bytes.len() > OUTPUT_CAP;
        let mut end = bytes.len().min(OUTPUT_CAP);
        while end > 0 && end < bytes.len() && (bytes[end] & 0xC0) == 0x80 {
            end -= 1;
        }
        Self {
            text: redact(&String::from_utf8_lossy(&bytes[..end])),
            truncated,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: u64,
    pub program: String,
    pub args: Vec<String>,
    pub dir: PathBuf,
    pub started: SystemTime,
    pub duration: Duration,
    pub exit: Option<i32>,
    pub stdout: Output,
    pub stderr: Output,
    pub kind: Kind,
}

impl Entry {
    pub fn command_line(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .map(|a| {
                if a.is_empty() || a.contains([' ', '\'', '"', '\n']) {
                    format!("'{}'", a.replace('\'', "'\\''"))
                } else {
                    a.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn failed(&self) -> bool {
        self.exit != Some(0)
    }
}

pub struct Ring {
    entries: VecDeque<Entry>,
    capacity: usize,
}

impl Ring {
    pub const fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity,
        }
    }

    pub fn push(&mut self, entry: Entry) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn newest_first(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().rev()
    }
}

static LOG: Mutex<Ring> = Mutex::new(Ring::new(CAPACITY));
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static ACTION_DEPTH: Cell<u32> = const { Cell::new(0) };
}

pub fn as_action<T>(run: impl FnOnce() -> T) -> T {
    ACTION_DEPTH.with(|d| d.set(d.get() + 1));
    let out = run();
    ACTION_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    out
}

fn current_kind() -> Kind {
    if ACTION_DEPTH.with(Cell::get) > 0 {
        Kind::Action
    } else {
        Kind::Background
    }
}

pub struct Started {
    program: String,
    args: Vec<String>,
    dir: PathBuf,
    wall: SystemTime,
    clock: Instant,
    kind: Kind,
}

pub fn start<S: AsRef<str>>(program: &str, args: &[S], dir: &Path) -> Started {
    Started {
        program: program.to_string(),
        args: args.iter().map(|a| redact(a.as_ref())).collect(),
        dir: dir.to_path_buf(),
        wall: SystemTime::now(),
        clock: Instant::now(),
        kind: current_kind(),
    }
}

impl Started {
    pub fn finish(self, exit: Option<i32>, stdout: &[u8], stderr: &[u8]) -> u64 {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        push(Entry {
            id,
            program: self.program,
            args: self.args,
            dir: self.dir,
            started: self.wall,
            duration: self.clock.elapsed(),
            exit,
            stdout: Output::capture(stdout),
            stderr: Output::capture(stderr),
            kind: self.kind,
        });
        id
    }

    pub fn finish_output(self, output: &std::process::Output) -> u64 {
        self.finish(output.status.code(), &output.stdout, &output.stderr)
    }
}

fn push(entry: Entry) {
    LOG.lock().unwrap_or_else(|e| e.into_inner()).push(entry);
}

pub fn entries() -> Vec<Entry> {
    LOG.lock()
        .unwrap_or_else(|e| e.into_inner())
        .newest_first()
        .cloned()
        .collect()
}

pub fn latest_id() -> u64 {
    NEXT_ID.load(Ordering::Relaxed).saturating_sub(1)
}

pub fn latest_failure_within(window: Duration) -> Option<u64> {
    let now = SystemTime::now();
    LOG.lock()
        .unwrap_or_else(|e| e.into_inner())
        .newest_first()
        .take_while(|e| {
            now.duration_since(e.started)
                .is_ok_and(|age| age <= window + e.duration)
        })
        .find(|e| e.failed())
        .map(|e| e.id)
}

pub fn redact(text: &str) -> String {
    let mut out = redact_url_credentials(text);
    for prefix in [
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "github_pat_",
        "glpat-",
    ] {
        out = redact_token(&out, prefix);
    }
    redact_auth_header(&out)
}

fn redact_url_credentials(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(scheme) = rest.find("://") {
        let (head, tail) = rest.split_at(scheme + 3);
        out.push_str(head);
        let host_end = tail
            .find(|c: char| c == '/' || c.is_whitespace())
            .unwrap_or(tail.len());
        match tail[..host_end].rfind('@') {
            Some(at) => {
                out.push_str("***");
                out.push_str(&tail[at..host_end]);
            }
            None => out.push_str(&tail[..host_end]),
        }
        rest = &tail[host_end..];
    }
    out.push_str(rest);
    out
}

fn redact_token(text: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(prefix) {
        out.push_str(&rest[..at]);
        let token_len = rest[at..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
            .unwrap_or(rest.len() - at);
        if token_len > prefix.len() + 8 {
            out.push_str(prefix);
            out.push_str("***");
        } else {
            out.push_str(&rest[at..at + token_len]);
        }
        rest = &rest[at + token_len..];
    }
    out.push_str(rest);
    out
}

fn redact_auth_header(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    for marker in [
        "authorization: bearer ",
        "authorization: basic ",
        "authorization: token ",
    ] {
        if let Some(at) = lower.find(marker) {
            let start = at + marker.len();
            let end = text[start..]
                .find(|c: char| c.is_whitespace())
                .map_or(text.len(), |e| start + e);
            return format!("{}***{}", &text[..start], redact_auth_header(&text[end..]));
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_in_urls_are_hidden() {
        assert_eq!(
            redact("fetch https://vlad:s3cret@github.com/o/r.git main"),
            "fetch https://***@github.com/o/r.git main"
        );
        assert_eq!(redact("https://github.com/o/r"), "https://github.com/o/r");
        assert_eq!(redact("ssh://git@host/x"), "ssh://***@host/x");
    }

    #[test]
    fn tokens_and_auth_headers_are_hidden() {
        assert_eq!(
            redact("token ghp_abcdefghijklmnopqrstuvwxyz0123 ok"),
            "token ghp_*** ok"
        );
        assert_eq!(
            redact("-c http.extraHeader=Authorization: Bearer abc.def.ghi"),
            "-c http.extraHeader=Authorization: Bearer ***"
        );
        assert_eq!(redact("ghp_short"), "ghp_short");
    }

    #[test]
    fn output_is_capped_on_a_char_boundary() {
        let big = "é".repeat(OUTPUT_CAP);
        let out = Output::capture(big.as_bytes());
        assert!(out.truncated);
        assert!(out.text.len() <= OUTPUT_CAP);
        assert!(out.text.chars().all(|c| c == 'é'));
        assert!(!Output::capture(b"small").truncated);
    }

    fn entry(id: u64, args: &[&str]) -> Entry {
        Entry {
            id,
            program: "git".into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            dir: PathBuf::new(),
            started: SystemTime::now(),
            duration: Duration::ZERO,
            exit: Some(0),
            stdout: Output::capture(b""),
            stderr: Output::capture(b""),
            kind: Kind::Action,
        }
    }

    #[test]
    fn the_ring_keeps_the_newest_entries() {
        let mut ring = Ring::new(3);
        for id in 1..=5 {
            ring.push(entry(id, &["status"]));
        }
        let ids: Vec<u64> = ring.newest_first().map(|e| e.id).collect();
        assert_eq!(ids, [5, 4, 3]);
    }

    #[test]
    fn actions_are_told_apart_from_background_work() {
        assert_eq!(
            start("git", &["status"], Path::new(".")).kind,
            Kind::Background
        );
        let kind = as_action(|| start("git", &["push"], Path::new(".")).kind);
        assert_eq!(kind, Kind::Action);
        assert_eq!(
            start("git", &["status"], Path::new(".")).kind,
            Kind::Background
        );
    }

    #[test]
    fn command_lines_quote_what_needs_quoting() {
        let entry = entry(1, &["commit", "-m", "it's done"]);
        assert_eq!(entry.command_line(), "git commit -m 'it'\\''s done'");
    }
}
