use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::bail;

use crate::git_cli;
use crate::ops::Op;

const MAX_ENTRIES: usize = 50;
const MAX_UNTRACKED_BYTES: usize = 16 * 1024 * 1024;
const SAVED_REFS: &str = "refs/kelp/undo";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Stash {
    sha: String,
    subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    head_ref: Option<String>,
    head: Option<String>,
    index_tree: Option<String>,
    worktree_tree: Option<String>,
    snapshot: Option<String>,
    branches: BTreeMap<String, String>,
    stashes: Vec<Stash>,
}

#[derive(Debug, Clone)]
pub enum Replay {
    Op(Op),
    Rewrite,
}

#[derive(Debug, Clone)]
pub struct Record {
    pub replay: Replay,
    pub label: String,
    before: State,
    after: State,
    untracked: Vec<(PathBuf, Vec<u8>)>,
    branch_config: Vec<(String, String)>,
    saved_refs: Vec<String>,
}

#[derive(Debug)]
pub enum Outcome {
    Recorded(Box<Record>),
    NotUndoable { label: String, reason: &'static str },
    Unchanged,
}

impl Record {
    pub fn op(&self) -> Option<&Op> {
        match &self.replay {
            Replay::Op(op) => Some(op),
            Replay::Rewrite => None,
        }
    }
}

pub fn not_undoable_reason(op: &Op) -> Option<&'static str> {
    match op {
        Op::Push { .. } | Op::DeleteRemoteBranch { .. } => Some("it changed the remote"),
        Op::Fetch => Some("fetching only updates remote branches"),
        Op::WorktreeAdd { .. } | Op::WorktreeRemove { .. } | Op::WorktreePrune => {
            Some("worktree folders are not tracked by undo")
        }
        _ => None,
    }
}

/// Runs `op` like `Op::run`, recording the repository state around it so
/// the change can be reversed with [`undo`].
pub fn run_recorded(op: &Op, dir: &Path) -> (anyhow::Result<String>, Outcome) {
    if let Some(reason) = not_undoable_reason(op) {
        let result = op.run(dir);
        let outcome = match result {
            Ok(_) => Outcome::NotUndoable {
                label: op.label(),
                reason,
            },
            Err(_) => Outcome::Unchanged,
        };
        return (result, outcome);
    }
    let before = capture(dir).ok();
    let untracked = match op {
        Op::DeleteUntracked(paths) => read_untracked(dir, paths),
        _ => Some(Vec::new()),
    };
    let branch_config = match op {
        Op::DeleteBranch { name, .. } => branch_config(dir, name),
        _ => Vec::new(),
    };
    let result = op.run(dir);
    if result.is_err() {
        return (result, Outcome::Unchanged);
    }
    let (Some(before), Some(untracked), Ok(after)) = (before, untracked, capture(dir)) else {
        return (
            result,
            Outcome::NotUndoable {
                label: op.label(),
                reason: "Kelp could not read the repository state around it",
            },
        );
    };
    if before == after && untracked.is_empty() {
        return (result, Outcome::Unchanged);
    }
    let saved_refs = save_snapshots(dir, &[&before, &after]);
    let record = Record {
        replay: Replay::Op(op.clone()),
        label: op.label(),
        before,
        after,
        untracked,
        branch_config,
        saved_refs,
    };
    (result, Outcome::Recorded(Box::new(record)))
}

/// Records a history rewrite (interactive rebase, message edit) that already
/// finished, given the state captured before it ran.
pub fn record_rewrite(dir: &Path, label: String, before: State) -> Outcome {
    let Ok(after) = capture(dir) else {
        return Outcome::NotUndoable {
            label,
            reason: "Kelp could not read the repository state around it",
        };
    };
    if before == after {
        return Outcome::Unchanged;
    }
    let mut saved_refs = save_snapshots(dir, &[&before, &after]);
    let tips = changed_tips(&before, &after);
    saved_refs.extend(save_commits(dir, tips));
    Outcome::Recorded(Box::new(Record {
        replay: Replay::Rewrite,
        label,
        before,
        after,
        untracked: Vec::new(),
        branch_config: Vec::new(),
        saved_refs,
    }))
}

/// Re-applies an undone history rewrite by moving the refs back to the
/// rewritten commits. Refuses when the repository changed since the undo.
pub fn redo_rewrite(dir: &Path, record: &Record) -> anyhow::Result<Vec<String>> {
    let current = capture(dir)?;
    if !matches_state(&current, &record.before, &record.after) {
        bail!(
            "The repository changed since you undid {}, so redo would lose work. Nothing was changed.",
            lowercase_first(&record.label)
        );
    }
    let mut run = Runner {
        dir,
        list: Vec::new(),
    };
    let head_moves = record.before.head_ref != record.after.head_ref
        || (record.after.head_ref.is_none() && record.before.head != record.after.head);
    move_state(&mut run, &record.before, &record.after, head_moves, None)?;
    let _ = git_cli::run(dir, &["update-index", "-q", "--refresh"]);
    Ok(run.list)
}

fn changed_tips<'a>(before: &'a State, after: &'a State) -> Vec<&'a str> {
    let mut tips: Vec<&str> = before
        .branches
        .iter()
        .chain(after.branches.iter())
        .filter(|(name, sha)| {
            before.branches.get(*name) != after.branches.get(*name) && !sha.is_empty()
        })
        .map(|(_, sha)| sha.as_str())
        .collect();
    tips.extend(before.head.as_deref());
    tips.extend(after.head.as_deref());
    tips.sort_unstable();
    tips.dedup();
    tips
}

/// Reverses a recorded action and returns the git commands it ran. Refuses
/// without touching anything when the repository moved on since the action.
pub fn undo(dir: &Path, record: &Record) -> anyhow::Result<Vec<String>> {
    let current = capture(dir)?;
    if !still_after(dir, record, &current) {
        bail!(
            "The repository changed since {}, so undo would lose work. Nothing was changed.",
            lowercase_first(&record.label)
        );
    }
    let (before, after) = (&record.before, &record.after);
    let mut run = Runner {
        dir,
        list: Vec::new(),
    };

    let renamed = match record.op() {
        Some(Op::RenameBranch { from, to }) => {
            run.git(&["branch", "-m", to, from])?;
            Some((from, to))
        }
        _ => None,
    };
    let head_moves = renamed.is_none()
        && (before.head_ref != after.head_ref
            || (before.head_ref.is_none() && before.head != after.head));
    move_state(&mut run, after, before, head_moves, renamed)?;
    for (key, value) in &record.branch_config {
        run.git(&["config", key, value])?;
    }
    for (path, bytes) in &record.untracked {
        let full = dir.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full, bytes)?;
        run.list.push(format!("restore {}", path.display()));
    }
    for stash in &before.stashes {
        if !after.stashes.contains(stash) {
            run.git(&["stash", "store", "-m", &stash.subject, &stash.sha])?;
        }
    }
    for stash in &after.stashes {
        if !before.stashes.contains(stash)
            && let Some(position) = list_stashes(dir)?.iter().position(|s| s == stash)
        {
            run.git(&["stash", "drop", "-q", &format!("stash@{{{position}}}")])?;
        }
    }
    let _ = git_cli::run(dir, &["update-index", "-q", "--refresh"]);
    Ok(run.list)
}

fn move_state(
    run: &mut Runner,
    from: &State,
    to: &State,
    head_moves: bool,
    renamed: Option<(&String, &String)>,
) -> anyhow::Result<()> {
    if head_moves {
        match (&to.head_ref, &to.head) {
            (Some(name), _) => run.git(&["symbolic-ref", "HEAD", name])?,
            (None, Some(sha)) => run.git(&["update-ref", "--no-deref", "HEAD", sha])?,
            (None, None) => {}
        }
    }
    let names: BTreeSet<&String> = to
        .branches
        .keys()
        .chain(from.branches.keys())
        .filter(|n| renamed.is_none_or(|(a, b)| *n != a && *n != b))
        .collect();
    for name in names {
        let full = format!("refs/heads/{name}");
        match (to.branches.get(name), from.branches.get(name)) {
            (Some(target), Some(current)) if target != current => {
                run.git(&["update-ref", &full, target, current])?
            }
            (Some(target), None) => run.git(&["update-ref", &full, target])?,
            (None, Some(current)) => run.git(&["update-ref", "-d", &full, current])?,
            _ => {}
        }
    }
    if to.worktree_tree != from.worktree_tree {
        let (Some(current), Some(target)) = (&from.worktree_tree, &to.worktree_tree) else {
            bail!("Kelp could not read the files as they were");
        };
        run.git(&["read-tree", current])?;
        run.git(&["read-tree", "-u", "--reset", target])?;
    }
    if to.index_tree != current_index(run.dir) {
        let Some(index) = &to.index_tree else {
            bail!("Kelp could not read the staged changes as they were");
        };
        run.git(&["read-tree", index])?;
    }
    Ok(())
}

struct Runner<'a> {
    dir: &'a Path,
    list: Vec<String>,
}

impl Runner<'_> {
    fn git(&mut self, args: &[&str]) -> anyhow::Result<()> {
        git_cli::run(self.dir, args)?;
        self.list.push(git_cli::command_line(args));
        Ok(())
    }
}

pub fn forget(dir: &Path, records: &[Record]) {
    for name in records.iter().flat_map(|r| &r.saved_refs) {
        let _ = git_cli::run(dir, &["update-ref", "-d", name]);
    }
}

pub fn clear_saved(dir: &Path) {
    let Ok(out) = git_cli::run(dir, &["for-each-ref", "--format=%(refname)", SAVED_REFS]) else {
        return;
    };
    for name in out.lines().filter(|l| !l.is_empty()) {
        let _ = git_cli::run(dir, &["update-ref", "-d", name]);
    }
}

#[derive(Default)]
pub struct Stack {
    undo: Vec<Record>,
    redo: Vec<Record>,
    pub last_skipped: Option<(String, &'static str)>,
}

impl Stack {
    pub fn record(&mut self, record: Record) -> Vec<Record> {
        self.last_skipped = None;
        self.undo.push(record);
        let mut evicted: Vec<Record> = std::mem::take(&mut self.redo);
        if self.undo.len() > MAX_ENTRIES {
            evicted.extend(self.undo.drain(..self.undo.len() - MAX_ENTRIES));
        }
        evicted
    }

    pub fn record_redone(&mut self, record: Record) {
        self.last_skipped = None;
        self.undo.push(record);
    }

    pub fn skip(&mut self, label: String, reason: &'static str) {
        self.last_skipped = Some((label, reason));
    }

    pub fn next_undo(&self) -> Option<&Record> {
        self.undo.last()
    }

    pub fn next_redo(&self) -> Option<&Record> {
        self.redo.last()
    }

    pub fn take_undo(&mut self) -> Option<Record> {
        self.undo.pop()
    }

    pub fn take_redo(&mut self) -> Option<Record> {
        self.redo.pop()
    }

    pub fn undone(&mut self, record: Record) {
        self.redo.push(record);
    }

    pub fn restore_undo(&mut self, record: Record) {
        self.undo.push(record);
    }

    pub fn restore_redo(&mut self, record: Record) {
        self.redo.push(record);
    }

    pub fn clear(&mut self) -> Vec<Record> {
        self.undo.drain(..).chain(self.redo.drain(..)).collect()
    }
}

fn matches_state(current: &State, expected: &State, other: &State) -> bool {
    let same_trees = current.head_ref == expected.head_ref
        && current.head == expected.head
        && current.index_tree == expected.index_tree
        && current.worktree_tree == expected.worktree_tree;
    let names: BTreeSet<&String> = expected
        .branches
        .keys()
        .chain(other.branches.keys())
        .collect();
    same_trees
        && names
            .into_iter()
            .filter(|n| expected.branches.get(*n) != other.branches.get(*n))
            .all(|n| current.branches.get(n) == expected.branches.get(n))
}

fn still_after(dir: &Path, record: &Record, current: &State) -> bool {
    let (before, after) = (&record.before, &record.after);
    let stashes_kept = before.stashes == after.stashes || current.stashes == after.stashes;
    let untracked_free = record
        .untracked
        .iter()
        .all(|(path, _)| !dir.join(path).exists());
    matches_state(current, after, before) && stashes_kept && untracked_free
}

fn branch_config(dir: &Path, name: &str) -> Vec<(String, String)> {
    let key = format!("branch.{name}.");
    git_cli::run(dir, &["config", "--local", "--list"])
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('='))
        .filter(|(k, _)| k.starts_with(&key))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

pub fn capture(dir: &Path) -> anyhow::Result<State> {
    let quiet = |args: &[&str]| -> Option<String> {
        let mut full = vec!["--no-optional-locks"];
        full.extend_from_slice(args);
        git_cli::run(dir, &full)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let head = quiet(&["rev-parse", "-q", "--verify", "HEAD^{commit}"]);
    let head_ref = quiet(&["symbolic-ref", "-q", "HEAD"]);
    let index_tree = quiet(&["write-tree"]);
    let snapshot = head.as_ref().and_then(|_| quiet(&["stash", "create"]));
    let worktree_tree = match (&snapshot, &head) {
        (Some(snapshot), _) => quiet(&["rev-parse", &format!("{snapshot}^{{tree}}")]),
        (None, Some(head)) => quiet(&["rev-parse", &format!("{head}^{{tree}}")]),
        (None, None) => index_tree.clone(),
    };
    let branches = quiet(&[
        "for-each-ref",
        "--format=%(refname:short) %(objectname)",
        "refs/heads",
    ])
    .unwrap_or_default()
    .lines()
    .filter_map(|l| l.rsplit_once(' '))
    .map(|(name, sha)| (name.to_string(), sha.to_string()))
    .collect();
    Ok(State {
        head_ref,
        head,
        index_tree,
        worktree_tree,
        snapshot,
        branches,
        stashes: list_stashes(dir)?,
    })
}

fn list_stashes(dir: &Path) -> anyhow::Result<Vec<Stash>> {
    let out = git_cli::run(
        dir,
        &["--no-optional-locks", "stash", "list", "--format=%H %gs"],
    )?;
    Ok(out
        .lines()
        .filter_map(|l| l.split_once(' '))
        .map(|(sha, subject)| Stash {
            sha: sha.to_string(),
            subject: subject.to_string(),
        })
        .collect())
}

fn current_index(dir: &Path) -> Option<String> {
    git_cli::run(dir, &["write-tree"])
        .ok()
        .map(|s| s.trim().to_string())
}

fn read_untracked(dir: &Path, paths: &[String]) -> Option<Vec<(PathBuf, Vec<u8>)>> {
    let mut files = Vec::new();
    let mut total = 0;
    for path in paths {
        let full = dir.join(path);
        if full.is_file() {
            let bytes = std::fs::read(&full).ok()?;
            total += bytes.len();
            if total > MAX_UNTRACKED_BYTES {
                return None;
            }
            files.push((PathBuf::from(path), bytes));
        }
    }
    Some(files)
}

fn save_snapshots(dir: &Path, states: &[&State]) -> Vec<String> {
    save_commits(dir, states.iter().filter_map(|s| s.snapshot.as_deref()))
}

fn save_commits<'a>(dir: &Path, shas: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    shas.into_iter()
        .filter_map(|sha| {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let name = format!("{SAVED_REFS}/{stamp}-{n}");
            git_cli::run(dir, &["update-ref", &name, sha]).ok()?;
            Some(name)
        })
        .collect()
}

pub fn lowercase_first(label: &str) -> String {
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}
