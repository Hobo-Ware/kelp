use std::collections::HashSet;
use std::path::Path;

use gix::ObjectId;

use crate::git_cli;

const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Commit,
    Amend,
    Checkout,
    Rebase,
    Reset,
    Merge,
    Pull,
    CherryPick,
    Revert,
    Branch,
    Clone,
    Other,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::Commit => "commit",
            Action::Amend => "amend",
            Action::Checkout => "checkout",
            Action::Rebase => "rebase",
            Action::Reset => "reset",
            Action::Merge => "merge",
            Action::Pull => "pull",
            Action::CherryPick => "cherry-pick",
            Action::Revert => "revert",
            Action::Branch => "branch",
            Action::Clone => "clone",
            Action::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: ObjectId,
    pub action: Action,
    pub message: String,
    pub time: i64,
    pub title: String,
    pub reachable: bool,
}

pub fn classify(subject: &str) -> (Action, String) {
    let (verb, rest) = match subject.split_once(": ") {
        Some((verb, rest)) => (verb, rest),
        None => (subject, ""),
    };
    let head = verb.split_whitespace().next().unwrap_or_default();
    let action = match head {
        "commit" if verb.contains("(amend)") => Action::Amend,
        "commit" => Action::Commit,
        "checkout" | "switch" => Action::Checkout,
        "rebase" => Action::Rebase,
        "reset" => Action::Reset,
        "merge" => Action::Merge,
        "pull" => Action::Pull,
        "cherry-pick" => Action::CherryPick,
        "revert" => Action::Revert,
        "branch" => Action::Branch,
        "clone" => Action::Clone,
        _ => Action::Other,
    };
    let message = if rest.is_empty() { subject } else { rest };
    (action, message.to_string())
}

fn parse(out: &str) -> Vec<(ObjectId, Action, String, i64, String)> {
    out.split(RECORD)
        .filter_map(|record| {
            let mut fields = record.trim_start_matches('\n').split(FIELD);
            let id = ObjectId::from_hex(fields.next()?.as_bytes()).ok()?;
            let selector = fields.next()?;
            let subject = fields.next()?;
            let title = fields.next().unwrap_or_default().to_string();
            let time = selector
                .rsplit_once('{')
                .and_then(|(_, t)| t.trim_end_matches('}').parse().ok())
                .unwrap_or_default();
            let (action, message) = classify(subject);
            Some((id, action, message, time, title))
        })
        .collect()
}

pub fn load(dir: &Path, reference: &str) -> anyhow::Result<Vec<Entry>> {
    let format = format!("--format=%H{FIELD}%gd{FIELD}%gs{FIELD}%s{RECORD}");
    let out = git_cli::run(
        dir,
        &["reflog", "show", "--date=unix", &format, reference, "--"],
    )?;
    let raw = parse(&out);
    let reachable = reachable_among(dir, raw.iter().map(|(id, ..)| *id))?;
    Ok(raw
        .into_iter()
        .map(|(id, action, message, time, title)| Entry {
            reachable: reachable.contains(&id),
            id,
            action,
            message,
            time,
            title,
        })
        .collect())
}

fn reachable_among(
    dir: &Path,
    ids: impl Iterator<Item = ObjectId>,
) -> anyhow::Result<HashSet<ObjectId>> {
    let wanted: HashSet<ObjectId> = ids.collect();
    if wanted.is_empty() {
        return Ok(wanted);
    }
    let out = git_cli::run(
        dir,
        &["rev-list", "--branches", "--tags", "--remotes", "HEAD"],
    )?;
    Ok(out
        .lines()
        .filter_map(|line| ObjectId::from_hex(line.trim().as_bytes()).ok())
        .filter(|id| wanted.contains(id))
        .collect())
}

pub fn local_branches(dir: &Path) -> Vec<String> {
    git_cli::run(
        dir,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )
    .map(|out| out.lines().map(str::to_string).collect())
    .unwrap_or_default()
}

pub fn stash_untracked(dir: &Path, stash: &str) -> Option<(ObjectId, Vec<String>)> {
    let untracked = format!("{stash}^3");
    let id = git_cli::run(dir, &["rev-parse", "--verify", "--quiet", &untracked]).ok()?;
    let id = ObjectId::from_hex(id.trim().as_bytes()).ok()?;
    let files = git_cli::run(dir, &["ls-tree", "-r", "--name-only", &id.to_string()]).ok()?;
    Some((id, files.lines().map(str::to_string).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_kind_is_recognized() {
        let cases = [
            ("commit: add a thing", Action::Commit, "add a thing"),
            ("commit (initial): first", Action::Commit, "first"),
            (
                "commit (amend): fix: typo: really",
                Action::Amend,
                "fix: typo: really",
            ),
            (
                "commit (merge): Merge branch 'x'",
                Action::Commit,
                "Merge branch 'x'",
            ),
            (
                "checkout: moving from main to feat/x",
                Action::Checkout,
                "moving from main to feat/x",
            ),
            (
                "rebase (finish): returning to refs/heads/main",
                Action::Rebase,
                "returning to refs/heads/main",
            ),
            ("rebase (pick): add thing", Action::Rebase, "add thing"),
            ("reset: moving to HEAD~1", Action::Reset, "moving to HEAD~1"),
            ("merge feat/x: Fast-forward", Action::Merge, "Fast-forward"),
            ("pull: Fast-forward", Action::Pull, "Fast-forward"),
            ("cherry-pick: add thing", Action::CherryPick, "add thing"),
            ("revert: Revert \"x\"", Action::Revert, "Revert \"x\""),
            (
                "branch: Created from HEAD",
                Action::Branch,
                "Created from HEAD",
            ),
            (
                "clone: from https://example.com/x.git",
                Action::Clone,
                "from https://example.com/x.git",
            ),
            ("something new", Action::Other, "something new"),
        ];
        for (subject, action, message) in cases {
            assert_eq!(
                classify(subject),
                (action, message.to_string()),
                "{subject}"
            );
        }
    }

    #[test]
    fn records_parse_with_colons_and_unix_times() {
        let id = "a".repeat(40);
        let out = format!(
            "{id}{FIELD}HEAD@{{1790000000}}{FIELD}commit (amend): fix: a: b{FIELD}fix: a: b{RECORD}\n\
             {id}{FIELD}HEAD@{{1789999000}}{FIELD}checkout: moving from a to b{FIELD}t{RECORD}\n"
        );
        let parsed = parse(&out);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].1, Action::Amend);
        assert_eq!(parsed[0].2, "fix: a: b");
        assert_eq!(parsed[0].3, 1790000000);
        assert_eq!(parsed[1].1, Action::Checkout);
    }
}
