use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};
use gix::ObjectId;

use crate::git_cli;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Pick,
    Reword,
    Squash,
    Fixup,
    Drop,
}

impl Action {
    pub const ALL: [Action; 5] = [
        Action::Pick,
        Action::Reword,
        Action::Squash,
        Action::Fixup,
        Action::Drop,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Action::Pick => "Pick",
            Action::Reword => "Reword",
            Action::Squash => "Squash",
            Action::Fixup => "Fixup",
            Action::Drop => "Drop",
        }
    }

    pub fn joins_previous(self) -> bool {
        matches!(self, Action::Squash | Action::Fixup)
    }
}

#[derive(Debug, Clone)]
pub struct Step {
    pub id: ObjectId,
    pub author: String,
    pub time: i64,
    pub original: String,
    pub action: Action,
    pub message: String,
}

impl Step {
    pub fn summary(&self) -> &str {
        self.original.lines().next().unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub base: ObjectId,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub leader: usize,
    pub members: Vec<usize>,
}

impl Group {
    pub fn squashes(&self, steps: &[Step]) -> bool {
        self.members
            .iter()
            .any(|&m| steps[m].action == Action::Squash)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Conflicts,
    Paused(String),
}

pub fn load(dir: &Path, base: &str) -> anyhow::Result<Plan> {
    let base_id = git_cli::run(
        dir,
        &["rev-parse", "--verify", &format!("{base}^{{commit}}")],
    )?
    .trim()
    .to_string();
    if git_cli::run(dir, &["merge-base", "--is-ancestor", &base_id, "HEAD"]).is_err() {
        bail!(
            "This commit is not part of the current branch, so there is nothing to rebase onto it."
        );
    }
    let log = git_cli::run(
        dir,
        &[
            "log",
            "--reverse",
            "--topo-order",
            "--format=%H%x1f%P%x1f%an%x1f%at%x1f%B%x1e",
            &format!("{base_id}..HEAD"),
        ],
    )?;
    let mut steps = Vec::new();
    let mut merges = 0;
    for record in log.split('\x1e').map(|r| r.trim_start_matches('\n')) {
        let mut fields = record.splitn(5, '\x1f');
        let (Some(id), Some(parents), Some(author), Some(time), Some(message)) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            continue;
        };
        if parents.split_whitespace().count() > 1 {
            merges += 1;
        }
        let message = message.trim_end().to_string();
        steps.push(Step {
            id: parse_id(id)?,
            author: author.to_string(),
            time: time.parse().unwrap_or_default(),
            original: message.clone(),
            action: Action::Pick,
            message,
        });
    }
    if merges > 0 {
        bail!(
            "There {} {merges} merge commit{} after this commit. Interactive rebase here only handles a straight line of commits.",
            if merges == 1 { "is" } else { "are" },
            if merges == 1 { "" } else { "s" },
        );
    }
    if steps.is_empty() {
        bail!("This is the latest commit, so there is nothing after it to rebase.");
    }
    Ok(Plan {
        base: parse_id(&base_id)?,
        steps,
    })
}

pub fn groups(steps: &[Step]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        match step.action {
            Action::Drop => {}
            action if action.joins_previous() && !groups.is_empty() => {
                groups.last_mut().expect("checked above").members.push(i);
            }
            _ => groups.push(Group {
                leader: i,
                members: Vec::new(),
            }),
        }
    }
    groups
}

pub fn problem(steps: &[Step]) -> Option<String> {
    let first = steps.iter().find(|s| s.action != Action::Drop)?;
    first.action.joins_previous().then(|| {
        format!(
            "\"{}\" can't be squashed or fixed up: there is no kept commit above it to fold into.",
            first.summary()
        )
    })
}

pub fn default_combined(steps: &[Step], group: &Group) -> String {
    let mut parts = vec![steps[group.leader].message.trim_end().to_string()];
    parts.extend(
        group
            .members
            .iter()
            .filter(|&&m| steps[m].action == Action::Squash)
            .map(|&m| steps[m].original.trim_end().to_string()),
    );
    parts.join("\n\n")
}

pub struct Todo {
    pub text: String,
    pub messages: Vec<(PathBuf, String)>,
}

pub fn todo(steps: &[Step], combined: &dyn Fn(&Group) -> String, message_dir: &Path) -> Todo {
    let mut text = String::new();
    let mut messages = Vec::new();
    let mut amend = |text: &mut String, message: String| {
        let path = message_dir.join(format!("message-{}.txt", messages.len()));
        text.push_str(&format!(
            "exec git commit --amend --allow-empty --quiet -F {}\n",
            shell_quote(&path.to_string_lossy())
        ));
        messages.push((path, message));
    };
    for group in groups(steps) {
        let leader = &steps[group.leader];
        text.push_str(&format!("pick {}\n", leader.id));
        for &m in &group.members {
            text.push_str(&format!("fixup {}\n", steps[m].id));
        }
        if group.squashes(steps) {
            amend(&mut text, combined(&group));
        } else if leader.action == Action::Reword && leader.message != leader.original {
            amend(&mut text, leader.message.clone());
        }
    }
    for step in steps.iter().filter(|s| s.action == Action::Drop) {
        text.push_str(&format!("drop {}\n", step.id));
    }
    Todo { text, messages }
}

pub fn run(
    dir: &Path,
    plan: &Plan,
    combined: &dyn Fn(&Group) -> String,
) -> anyhow::Result<Outcome> {
    if let Some(problem) = problem(&plan.steps) {
        bail!(problem);
    }
    let git_dir = PathBuf::from(git_cli::run(dir, &["rev-parse", "--absolute-git-dir"])?.trim());
    if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        bail!("A rebase is already in progress. Finish or abort it first.");
    }
    let dirty = git_cli::run(dir, &["status", "--porcelain", "--untracked-files=no"])?;
    if !dirty.trim().is_empty() {
        bail!("You have uncommitted changes. Commit or stash them before rebasing.");
    }
    let work = git_dir.join("kelp").join("rebase");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;
    let todo = todo(&plan.steps, combined, &work);
    for (path, message) in &todo.messages {
        std::fs::write(path, message)?;
    }
    let todo_path = work.join("todo");
    std::fs::write(&todo_path, &todo.text)?;
    let output = Command::new("git")
        .current_dir(dir)
        .args(["rebase", "-i", "--no-autosquash", &plan.base.to_string()])
        .env(
            "GIT_SEQUENCE_EDITOR",
            format!("cp {}", shell_quote(&todo_path.to_string_lossy())),
        )
        .env("GIT_EDITOR", "true")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .context("could not start git rebase")?;
    if output.status.success() {
        let _ = std::fs::remove_dir_all(&work);
        return Ok(Outcome::Done);
    }
    if !git_dir.join("rebase-merge").exists() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("git rebase failed: {stderr}");
    }
    let conflicts = git_cli::run(dir, &["diff", "--name-only", "--diff-filter=U"])?;
    if !conflicts.trim().is_empty() {
        return Ok(Outcome::Conflicts);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let reason = stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty() && !l.starts_with("hint:"))
        .unwrap_or("git stopped the rebase")
        .trim()
        .to_string();
    Ok(Outcome::Paused(reason))
}

fn parse_id(hex: &str) -> anyhow::Result<ObjectId> {
    ObjectId::from_hex(hex.as_bytes())
        .map_err(|_| anyhow::anyhow!("git printed a bad commit id: {hex}"))
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(n: u8, action: Action) -> Step {
        let hex = format!("{n:02x}").repeat(20);
        Step {
            id: ObjectId::from_hex(hex.as_bytes()).unwrap(),
            author: "a".into(),
            time: 0,
            original: format!("commit {n}"),
            action,
            message: format!("commit {n}"),
        }
    }

    #[test]
    fn squashes_and_fixups_join_the_kept_commit_above() {
        let steps = [
            step(1, Action::Pick),
            step(2, Action::Drop),
            step(3, Action::Squash),
            step(4, Action::Pick),
            step(5, Action::Fixup),
        ];
        assert_eq!(
            groups(&steps),
            vec![
                Group {
                    leader: 0,
                    members: vec![2]
                },
                Group {
                    leader: 3,
                    members: vec![4]
                }
            ]
        );
        assert_eq!(problem(&steps), None);
    }

    #[test]
    fn first_kept_commit_cannot_fold() {
        let steps = [step(1, Action::Drop), step(2, Action::Fixup)];
        assert!(problem(&steps).unwrap().contains("commit 2"));
    }

    #[test]
    fn todo_amends_only_where_the_message_changes() {
        let mut steps = vec![
            step(1, Action::Reword),
            step(2, Action::Pick),
            step(3, Action::Squash),
            step(4, Action::Drop),
            step(5, Action::Reword),
        ];
        steps[0].message = "new".into();
        let combined = |g: &Group| default_combined(&steps, g);
        let todo = todo(&steps, &combined, Path::new("/m"));
        let lines: Vec<&str> = todo.text.lines().collect();
        assert_eq!(lines[0], format!("pick {}", steps[0].id));
        assert!(lines[1].ends_with("-F '/m/message-0.txt'"));
        assert_eq!(lines[2], format!("pick {}", steps[1].id));
        assert_eq!(lines[3], format!("fixup {}", steps[2].id));
        assert!(lines[4].ends_with("-F '/m/message-1.txt'"));
        assert_eq!(lines[5], format!("pick {}", steps[4].id));
        assert_eq!(lines[6], format!("drop {}", steps[3].id));
        assert_eq!(todo.messages[0].1, "new");
        assert_eq!(todo.messages[1].1, "commit 2\n\ncommit 3");
    }

    #[test]
    fn quotes_survive_the_shell() {
        assert_eq!(shell_quote("/a b/it's"), r"'/a b/it'\''s'");
    }
}
