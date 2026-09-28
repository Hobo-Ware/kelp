use std::collections::HashMap;
use std::path::Path;

use gix::ObjectId;

use crate::git_cli;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    NotInitialized,
    Clean,
    Modified,
    NewCommits,
    Conflict,
}

impl State {
    pub fn label(self) -> &'static str {
        match self {
            State::NotInitialized => "not initialized",
            State::Clean => "clean",
            State::Modified => "modified",
            State::NewCommits => "new commits",
            State::Conflict => "conflict",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submodule {
    pub name: String,
    pub path: String,
    pub recorded: Option<ObjectId>,
    pub checked_out: Option<ObjectId>,
    pub state: State,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusLine {
    pub flag: char,
    pub id: Option<ObjectId>,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub path: String,
    pub old: Option<ObjectId>,
    pub new: Option<ObjectId>,
    pub old_title: Option<String>,
    pub new_title: Option<String>,
}

pub fn parse_status(out: &str) -> Vec<StatusLine> {
    out.lines()
        .filter_map(|line| {
            let mut chars = line.chars();
            let flag = chars.next()?;
            let rest = chars.as_str();
            let (sha, rest) = rest.split_once(' ')?;
            let path = rest.split(" (").next()?.trim().to_string();
            Some(StatusLine {
                flag,
                id: ObjectId::from_hex(sha.as_bytes()).ok(),
                path,
            })
        })
        .collect()
}

pub fn list(dir: &Path) -> anyhow::Result<Vec<Submodule>> {
    if !dir.join(".gitmodules").exists() {
        return Ok(Vec::new());
    }
    let names = names(dir);
    let current = parse_status(&git_cli::run(dir, &["submodule", "status"])?);
    let recorded: HashMap<String, Option<ObjectId>> =
        parse_status(&git_cli::run(dir, &["submodule", "status", "--cached"])?)
            .into_iter()
            .map(|line| (line.path, line.id))
            .collect();
    Ok(current
        .into_iter()
        .map(|line| {
            let state = match line.flag {
                '-' => State::NotInitialized,
                '+' => State::NewCommits,
                'U' => State::Conflict,
                _ if dirty(&dir.join(&line.path)) => State::Modified,
                _ => State::Clean,
            };
            let checked_out = (line.flag != '-').then_some(line.id).flatten();
            Submodule {
                name: names
                    .get(&line.path)
                    .cloned()
                    .unwrap_or_else(|| line.path.clone()),
                recorded: recorded.get(&line.path).copied().flatten().or(line.id),
                checked_out,
                path: line.path,
                state,
            }
        })
        .collect())
}

fn names(dir: &Path) -> HashMap<String, String> {
    git_cli::run(
        dir,
        &[
            "config",
            "-f",
            ".gitmodules",
            "--get-regexp",
            r"^submodule\..*\.path$",
        ],
    )
    .unwrap_or_default()
    .lines()
    .filter_map(|line| {
        let (key, path) = line.split_once(' ')?;
        let name = key.strip_prefix("submodule.")?.strip_suffix(".path")?;
        Some((path.to_string(), name.to_string()))
    })
    .collect()
}

fn dirty(path: &Path) -> bool {
    git_cli::run(path, &["--no-optional-locks", "status", "--porcelain"])
        .is_ok_and(|out| !out.trim().is_empty())
}

pub fn change(
    workdir: Option<&Path>,
    path: &str,
    old: Option<ObjectId>,
    new: Option<ObjectId>,
) -> Change {
    let repo = workdir
        .map(|w| w.join(path))
        .filter(|p| p.join(".git").exists())
        .and_then(|p| gix::open(p).ok());
    let title = |id: Option<ObjectId>| {
        let repo = repo.as_ref()?;
        let commit = repo.find_commit(id?).ok()?;
        let message = commit.message().ok()?;
        Some(message.summary().to_string())
    };
    Change {
        path: path.to_string(),
        old_title: title(old),
        new_title: title(new),
        old,
        new,
    }
}

pub fn checked_out(workdir: &Path, path: &str) -> Option<ObjectId> {
    let sub = workdir.join(path);
    if !sub.join(".git").exists() {
        return None;
    }
    let repo = gix::open(sub).ok()?;
    Some(repo.head_id().ok()?.detach())
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "1111111111111111111111111111111111111111";
    const B: &str = "2222222222222222222222222222222222222222";

    #[test]
    fn status_lines_parse_every_flag() {
        let out = format!(
            " {A} libs/core (v1.0-3-g1111111)\n-{B} vendor/ui\n+{A} tools (heads/main)\nU{B} docs\n"
        );
        let lines = parse_status(&out);
        let flags: Vec<char> = lines.iter().map(|l| l.flag).collect();
        assert_eq!(flags, [' ', '-', '+', 'U']);
        let paths: Vec<&str> = lines.iter().map(|l| l.path.as_str()).collect();
        assert_eq!(paths, ["libs/core", "vendor/ui", "tools", "docs"]);
        assert_eq!(lines[0].id, ObjectId::from_hex(A.as_bytes()).ok());
    }

    #[test]
    fn repos_without_gitmodules_have_none() {
        let dir = std::env::temp_dir().join(format!("kelp-no-modules-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(list(&dir).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
