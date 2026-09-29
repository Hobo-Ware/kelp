use std::path::Path;
use std::process::{Command, Output};

use anyhow::{Context, bail};

use crate::console;

pub fn run(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    run_retrying(dir, args, None)
}

pub fn run_with_stdin(dir: &Path, args: &[&str], input: &str) -> anyhow::Result<String> {
    run_retrying(dir, args, Some(input))
}

const LOCK_RETRY_MS: [u64; 5] = [50, 100, 200, 400, 800];

fn run_retrying(dir: &Path, args: &[&str], input: Option<&str>) -> anyhow::Result<String> {
    let mut waits = LOCK_RETRY_MS.iter();
    loop {
        let output = run_once(dir, args, input)?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let busy = lock_is_busy(&stderr);
        match waits.next() {
            Some(ms) if busy => std::thread::sleep(std::time::Duration::from_millis(*ms)),
            _ if busy => bail!(
                "Another git process is using this repository right now, so git could not \
                 take its lock. Try again in a moment.\n\ngit {} failed: {stderr}",
                args.join(" ")
            ),
            _ => bail!("git {} failed: {stderr}", args.join(" ")),
        }
    }
}

fn lock_is_busy(stderr: &str) -> bool {
    stderr.contains("index.lock")
        || stderr.contains("could not write index")
        || stderr.contains(".lock': File exists")
}

fn run_once(dir: &Path, args: &[&str], input: Option<&str>) -> anyhow::Result<Output> {
    use std::io::Write;
    use std::process::Stdio;
    let log = console::start("git", args, dir);
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = command
        .spawn()
        .with_context(|| format!("could not start git {}", args.join(" ")))?;
    if let Some(input) = input {
        child
            .stdin
            .take()
            .context("git stdin unavailable")?
            .write_all(input.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    log.finish_output(&output);
    Ok(output)
}

pub fn command_line(args: &[&str]) -> String {
    std::iter::once("git")
        .chain(args.iter().copied())
        .map(|a| {
            if a.contains(' ') {
                format!("'{a}'")
            } else {
                a.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn has_commit_graph(repo: &gix::Repository) -> bool {
    let info = repo.common_dir().join("objects").join("info");
    info.join("commit-graph").exists()
        || info
            .join("commit-graphs")
            .join("commit-graph-chain")
            .exists()
}

pub fn write_commit_graph(dir: &Path) -> anyhow::Result<()> {
    run(
        dir,
        &["commit-graph", "write", "--reachable", "--changed-paths"],
    )
    .map(|_| ())
}

pub fn supports_stash_staged() -> bool {
    static SUPPORTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SUPPORTED.get_or_init(|| {
        Command::new("git")
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| version_at_least(&String::from_utf8_lossy(&o.stdout), (2, 35)))
            .unwrap_or(false)
    })
}

fn version_at_least(version_line: &str, wanted: (u32, u32)) -> Option<bool> {
    let version = version_line.split_whitespace().nth(2)?;
    let mut parts = version.split('.').map(|p| p.parse::<u32>().ok());
    let found = (parts.next()??, parts.next()??);
    Some(found >= wanted)
}

#[cfg(test)]
mod tests {
    use super::{command_line, run, version_at_least};

    #[test]
    fn git_versions_compare_by_major_and_minor() {
        assert_eq!(version_at_least("git version 2.55.0", (2, 35)), Some(true));
        assert_eq!(
            version_at_least("git version 2.34.1 (Apple Git-137)", (2, 35)),
            Some(false)
        );
        assert_eq!(version_at_least("git version 3.0", (2, 35)), Some(true));
        assert_eq!(version_at_least("nope", (2, 35)), None);
    }

    #[test]
    fn a_busy_index_lock_is_waited_out() {
        let dir = std::env::temp_dir().join(format!("kelp-lock-wait-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        run(&dir, &["init", "-q", "-b", "main"]).unwrap();
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();
        let lock = dir.join(".git").join("index.lock");
        std::fs::write(&lock, "").unwrap();
        let release = {
            let lock = lock.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(250));
                std::fs::remove_file(lock).unwrap();
            })
        };
        run(&dir, &["add", "a.txt"]).unwrap();
        release.join().unwrap();
        let staged = run(&dir, &["diff", "--cached", "--name-only"]).unwrap();
        assert_eq!(staged.trim(), "a.txt");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_lock_that_never_frees_explains_itself() {
        let dir = std::env::temp_dir().join(format!("kelp-lock-stuck-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        run(&dir, &["init", "-q", "-b", "main"]).unwrap();
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();
        std::fs::write(dir.join(".git").join("index.lock"), "").unwrap();
        let error = run(&dir, &["add", "a.txt"]).unwrap_err().to_string();
        assert!(
            error.starts_with("Another git process is using this repository"),
            "{error}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_line_quotes_arguments_with_spaces() {
        assert_eq!(
            command_line(&["commit", "-m", "hello world"]),
            "git commit -m 'hello world'"
        );
    }
}
