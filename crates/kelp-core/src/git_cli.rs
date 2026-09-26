use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};

pub fn run(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .with_context(|| format!("could not start git {}", args.join(" ")))?;
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

#[cfg(test)]
mod tests {
    use super::command_line;

    #[test]
    fn command_line_quotes_arguments_with_spaces() {
        assert_eq!(
            command_line(&["commit", "-m", "hello world"]),
            "git commit -m 'hello world'"
        );
    }
}
