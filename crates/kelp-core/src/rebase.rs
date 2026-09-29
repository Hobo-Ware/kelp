use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};
use gix::ObjectId;

use crate::git_cli;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Pick,
    Reword,
    Edit,
    Squash,
    Fixup,
    Drop,
}

impl Action {
    pub const ALL: [Action; 6] = [
        Action::Pick,
        Action::Reword,
        Action::Edit,
        Action::Squash,
        Action::Fixup,
        Action::Drop,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Action::Pick => "Pick",
            Action::Reword => "Reword",
            Action::Edit => "Edit",
            Action::Squash => "Squash",
            Action::Fixup => "Fixup",
            Action::Drop => "Drop",
        }
    }

    pub fn allowed_with_merges(self) -> bool {
        !self.joins_previous()
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
    pub merge: bool,
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

impl Plan {
    pub fn has_merges(&self) -> bool {
        self.steps.iter().any(|s| s.merge)
    }
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
    Editing(String),
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
        let message = message.trim_end().to_string();
        steps.push(Step {
            id: parse_id(id)?,
            author: author.to_string(),
            time: time.parse().unwrap_or_default(),
            original: message.clone(),
            action: Action::Pick,
            message,
            merge: parents.split_whitespace().count() > 1,
        });
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
    if steps.iter().any(|s| s.merge) {
        if let Some(step) = steps.iter().find(|s| !s.action.allowed_with_merges()) {
            return Some(format!(
                "\"{}\" can't be squashed or fixed up here: the range has merge commits, so only Pick, Reword, Edit and Drop keep the history's shape.",
                step.summary()
            ));
        }
        if let Some(step) = steps.iter().find(|s| s.merge && s.action != Action::Pick) {
            return Some(format!(
                "\"{}\" is a merge commit; it keeps its place and can only be picked.",
                step.summary()
            ));
        }
    }
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
        text.push_str(&amend_line(&path));
        messages.push((path, message));
    };
    for group in groups(steps) {
        let leader = &steps[group.leader];
        let verb = if leader.action == Action::Edit {
            "edit"
        } else {
            "pick"
        };
        text.push_str(&format!("{verb} {}\n", leader.id));
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

pub fn keep_shape(git_todo: &str, steps: &[Step], message_dir: &Path) -> Todo {
    let mut text = String::new();
    let mut messages = Vec::new();
    for line in git_todo.lines() {
        let mut words = line.split_whitespace();
        let step = match (words.next(), words.next()) {
            (Some("pick" | "p"), Some(sha)) => ObjectId::from_hex(sha.as_bytes())
                .ok()
                .and_then(|id| steps.iter().find(|s| s.id == id)),
            _ => None,
        };
        let Some(step) = step else {
            text.push_str(line);
            text.push('\n');
            continue;
        };
        match step.action {
            Action::Drop => text.push_str(&format!("drop {}\n", step.id)),
            Action::Edit => text.push_str(&format!("edit {}\n", step.id)),
            Action::Reword if step.message != step.original => {
                let path = message_dir.join(format!("message-{}.txt", messages.len()));
                text.push_str(&format!("pick {}\n", step.id));
                text.push_str(&amend_line(&path));
                messages.push((path, step.message.clone()));
            }
            _ => {
                text.push_str(line);
                text.push('\n');
            }
        }
    }
    Todo { text, messages }
}

fn amend_line(path: &Path) -> String {
    format!(
        "exec git commit --amend --allow-empty --quiet -F {}\n",
        shell_quote(&path.to_string_lossy())
    )
}

fn git_todo_for(dir: &Path, base: &ObjectId, work: &Path) -> anyhow::Result<String> {
    let captured = work.join("git-todo");
    let base = base.to_string();
    let args = [
        "-c",
        "core.abbrev=40",
        "rebase",
        "-i",
        "--rebase-merges",
        &base,
    ];
    let log = crate::console::start("git", &args, dir);
    let captured_run = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env(
            "GIT_SEQUENCE_EDITOR",
            format!(
                "kelp_capture() {{ cp \"$1\" {}; exit 1; }}; kelp_capture",
                shell_quote(&captured.to_string_lossy())
            ),
        )
        .env("GIT_EDITOR", "true")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .context("could not start git rebase")?;
    log.finish_output(&captured_run);
    std::fs::read_to_string(&captured).context("git did not produce a rebase plan")
}

pub fn run(
    dir: &Path,
    plan: &Plan,
    combined: &dyn Fn(&Group) -> String,
) -> anyhow::Result<Outcome> {
    crate::console::as_action(|| run_plan(dir, plan, combined))
}

fn run_plan(
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
    let merges = plan.has_merges();
    let todo = if merges {
        keep_shape(&git_todo_for(dir, &plan.base, &work)?, &plan.steps, &work)
    } else {
        todo(&plan.steps, combined, &work)
    };
    for (path, message) in &todo.messages {
        std::fs::write(path, message)?;
    }
    let todo_path = work.join("todo");
    std::fs::write(&todo_path, &todo.text)?;
    let base = plan.base.to_string();
    let mut args = vec![
        "-c",
        "core.abbrev=40",
        "rebase",
        "-i",
        "--no-autosquash",
        "--autostash",
    ];
    if merges {
        args.push("--rebase-merges");
    }
    args.push(&base);
    let log = crate::console::start("git", &args, dir);
    let output = Command::new("git")
        .current_dir(dir)
        .args(&args)
        .env(
            "GIT_SEQUENCE_EDITOR",
            format!("cp {}", shell_quote(&todo_path.to_string_lossy())),
        )
        .env("GIT_EDITOR", "true")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .context("could not start git rebase")?;
    log.finish_output(&output);
    let stopped = git_dir.join("rebase-merge");
    if output.status.success() && !stopped.exists() {
        let _ = std::fs::remove_dir_all(&work);
        return Ok(Outcome::Done);
    }
    if !stopped.exists() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("git rebase failed: {stderr}");
    }
    if let Some(sha) = editing_at(&git_dir) {
        return Ok(Outcome::Editing(sha));
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

pub fn editing_at(git_dir: &Path) -> Option<String> {
    let amend = std::fs::read_to_string(git_dir.join("rebase-merge").join("amend")).ok()?;
    let sha = amend.trim();
    (!sha.is_empty()).then(|| sha.chars().take(7).collect())
}

pub fn edit_message(dir: &Path, commit: &str, message: &str) -> anyhow::Result<Outcome> {
    let message = message.trim_end();
    if message.trim().is_empty() {
        bail!("The commit message can't be empty.");
    }
    let resolve = |rev: &str| -> anyhow::Result<String> {
        Ok(git_cli::run(
            dir,
            &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
        )?
        .trim()
        .to_string())
    };
    let target = resolve(commit)?;
    if target == resolve("HEAD")? {
        let git_dir =
            PathBuf::from(git_cli::run(dir, &["rev-parse", "--absolute-git-dir"])?.trim());
        let work = git_dir.join("kelp");
        std::fs::create_dir_all(&work)?;
        let file = work.join("edit-message.txt");
        std::fs::write(&file, message)?;
        let result = git_cli::run(
            dir,
            &[
                "commit",
                "--amend",
                "--only",
                "--allow-empty",
                "--quiet",
                "-F",
                &file.to_string_lossy(),
            ],
        );
        let _ = std::fs::remove_file(&file);
        result?;
        return Ok(Outcome::Done);
    }
    if git_cli::run(dir, &["merge-base", "--is-ancestor", &target, "HEAD"]).is_err() {
        bail!("This commit is not on the current branch, so its message can't be edited here.");
    }
    let Ok(parent) = resolve(&format!("{target}^")) else {
        bail!("The first commit of a branch can only be edited while it is the latest commit.");
    };
    let mut plan = load(dir, &parent)?;
    let target_id = parse_id(&target)?;
    let Some(step) = plan.steps.iter_mut().find(|s| s.id == target_id) else {
        bail!("Kelp could not find this commit in the branch history.");
    };
    if step.merge {
        bail!("Merge commit messages can only be edited while the merge is the latest commit.");
    }
    step.action = Action::Reword;
    step.message = message.to_string();
    let steps = plan.steps.clone();
    run(dir, &plan, &|group| default_combined(&steps, group))
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
            merge: false,
        }
    }

    #[test]
    fn edit_stops_instead_of_picking() {
        let steps = [step(1, Action::Edit), step(2, Action::Pick)];
        let combined = |g: &Group| default_combined(&steps, g);
        let todo = todo(&steps, &combined, Path::new("/m"));
        assert!(todo.text.starts_with(&format!("edit {}\n", steps[0].id)));
    }

    #[test]
    fn merge_plans_keep_the_shape_and_only_touch_picks() {
        let mut steps = vec![
            step(1, Action::Drop),
            step(2, Action::Reword),
            step(3, Action::Pick),
            step(4, Action::Edit),
        ];
        steps[1].message = "new two".into();
        steps[2].merge = true;
        let captured = format!(
            "label onto\nreset onto\npick {} # one\nlabel side\nreset onto\npick {} # two\nmerge -C {} side # Merge side\npick {} # four\n",
            steps[0].id, steps[1].id, steps[2].id, steps[3].id
        );
        let todo = keep_shape(&captured, &steps, Path::new("/m"));
        let lines: Vec<&str> = todo.text.lines().collect();
        assert_eq!(lines[0], "label onto");
        assert_eq!(lines[2], format!("drop {}", steps[0].id));
        assert_eq!(lines[5], format!("pick {}", steps[1].id));
        assert!(lines[6].ends_with("-F '/m/message-0.txt'"));
        assert_eq!(
            lines[7],
            format!("merge -C {} side # Merge side", steps[2].id)
        );
        assert_eq!(lines[8], format!("edit {}", steps[3].id));
        assert_eq!(todo.messages[0].1, "new two");
    }

    #[test]
    fn merge_plans_refuse_squash_and_touching_merges() {
        let mut steps = vec![step(1, Action::Pick), step(2, Action::Squash)];
        steps[0].merge = true;
        assert!(problem(&steps).unwrap().contains("merge commits"));
        steps[1].action = Action::Pick;
        steps[0].action = Action::Drop;
        assert!(problem(&steps).unwrap().contains("is a merge commit"));
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
