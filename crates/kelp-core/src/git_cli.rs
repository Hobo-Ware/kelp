use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};

use crate::console;

pub fn run(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    let log = console::start("git", args, dir);
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .with_context(|| format!("could not start git {}", args.join(" ")))?;
    log.finish_output(&output);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("git {} failed: {stderr}", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn run_with_stdin(dir: &Path, args: &[&str], input: &str) -> anyhow::Result<String> {
    use std::io::Write;
    use std::process::Stdio;
    let log = console::start("git", args, dir);
    let mut child = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("could not start git {}", args.join(" ")))?;
    child
        .stdin
        .take()
        .context("git stdin unavailable")?
        .write_all(input.as_bytes())?;
    let output = child.wait_with_output()?;
    log.finish_output(&output);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("git {} failed: {stderr}", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
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
    use super::{command_line, version_at_least};

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
    fn command_line_quotes_arguments_with_spaces() {
        assert_eq!(
            command_line(&["commit", "-m", "hello world"]),
            "git commit -m 'hello world'"
        );
    }
}
