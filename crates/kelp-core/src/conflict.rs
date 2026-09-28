use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};

use crate::git_cli;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

impl Operation {
    fn command(self) -> &'static str {
        match self {
            Operation::Merge => "merge",
            Operation::Rebase => "rebase",
            Operation::CherryPick => "cherry-pick",
            Operation::Revert => "revert",
        }
    }

    fn noun(self) -> &'static str {
        match self {
            Operation::Merge => "Merge",
            Operation::Rebase => "Rebase",
            Operation::CherryPick => "Cherry-pick",
            Operation::Revert => "Revert",
        }
    }

    pub fn can_skip(self) -> bool {
        matches!(
            self,
            Operation::Rebase | Operation::CherryPick | Operation::Revert
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InProgress {
    pub operation: Operation,
    pub subject: String,
    pub editing: Option<String>,
}

impl InProgress {
    pub fn title(&self) -> String {
        if let Some(sha) = &self.editing {
            return format!("Rebase paused at {sha} for editing");
        }
        if self.subject.is_empty() {
            format!("{} in progress", self.operation.noun())
        } else {
            format!("{} of {} in progress", self.operation.noun(), self.subject)
        }
    }
}

/// Reads the markers git leaves in the (per-worktree) git dir while a
/// merge, rebase, cherry-pick or revert is stopped.
pub fn in_progress(git_dir: &Path) -> Option<InProgress> {
    let read = |name: &str| {
        std::fs::read_to_string(git_dir.join(name))
            .ok()
            .map(|s| s.trim().to_string())
    };
    let short = |sha: String| sha.chars().take(7).collect::<String>();
    for dir in ["rebase-merge", "rebase-apply"] {
        if git_dir.join(dir).is_dir() {
            let subject = read(&format!("{dir}/head-name"))
                .map(|head| head.trim_start_matches("refs/heads/").to_string())
                .filter(|head| head != "detached HEAD")
                .unwrap_or_default();
            return Some(InProgress {
                operation: Operation::Rebase,
                subject,
                editing: crate::rebase::editing_at(git_dir),
            });
        }
    }
    if let Some(head) = read("MERGE_HEAD") {
        let subject = read("MERGE_MSG")
            .and_then(|msg| merged_branch(&msg))
            .unwrap_or_else(|| short(head.lines().next().unwrap_or_default().to_string()));
        return Some(InProgress {
            operation: Operation::Merge,
            subject,
            editing: None,
        });
    }
    for (file, operation) in [
        ("CHERRY_PICK_HEAD", Operation::CherryPick),
        ("REVERT_HEAD", Operation::Revert),
    ] {
        if let Some(head) = read(file) {
            return Some(InProgress {
                operation,
                subject: short(head),
                editing: None,
            });
        }
    }
    None
}

fn merged_branch(message: &str) -> Option<String> {
    let first = message.lines().next()?;
    let rest = first
        .strip_prefix("Merge branch '")
        .or_else(|| first.strip_prefix("Merge remote-tracking branch '"))?;
    rest.split('\'').next().map(str::to_string)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Continue,
    Skip,
    Abort,
}

impl Step {
    pub fn label(self, operation: Operation) -> String {
        let verb = match self {
            Step::Continue => "Continuing",
            Step::Skip => "Skipping",
            Step::Abort => "Aborting",
        };
        format!("{verb} {}", operation.command())
    }

    fn flag(self) -> &'static str {
        match self {
            Step::Continue => "--continue",
            Step::Skip => "--skip",
            Step::Abort => "--abort",
        }
    }
}

pub fn command_line(operation: Operation, step: Step) -> String {
    git_cli::command_line(&[operation.command(), step.flag()])
}

pub fn run_step(dir: &Path, operation: Operation, step: Step) -> anyhow::Result<String> {
    if step == Step::Skip && operation == Operation::Merge {
        bail!("a merge cannot skip a commit");
    }
    let output = Command::new("git")
        .current_dir(dir)
        .args([operation.command(), step.flag()])
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .with_context(|| format!("could not start {}", command_line(operation, step)))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let reason = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        bail!("{} failed: {reason}", command_line(operation, step));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Ours,
    Theirs,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stages {
    pub base: bool,
    pub ours: bool,
    pub theirs: bool,
}

pub fn stages(dir: &Path, path: &str) -> anyhow::Result<Stages> {
    let out = git_cli::run(dir, &["ls-files", "-u", "-z", "--", path])?;
    let mut stages = Stages::default();
    for entry in out.split('\0').filter(|e| !e.is_empty()) {
        let stage = entry.split_whitespace().nth(2);
        match stage {
            Some("1") => stages.base = true,
            Some("2") => stages.ours = true,
            Some("3") => stages.theirs = true,
            _ => {}
        }
    }
    Ok(stages)
}

pub fn take_side(dir: &Path, path: &str, side: Side) -> anyhow::Result<()> {
    let present = stages(dir, path)?;
    let exists = match side {
        Side::Ours => present.ours,
        Side::Theirs => present.theirs,
    };
    if exists {
        let flag = match side {
            Side::Ours => "--ours",
            Side::Theirs => "--theirs",
        };
        git_cli::run(dir, &["checkout", flag, "--", path])?;
        git_cli::run(dir, &["add", "--", path])?;
    } else {
        git_cli::run(dir, &["rm", "--quiet", "--", path])?;
    }
    Ok(())
}

pub fn mark_resolved(dir: &Path, path: &str, content: Option<&str>) -> anyhow::Result<()> {
    if let Some(content) = content {
        std::fs::write(dir.join(path), content)
            .with_context(|| format!("could not write {path}"))?;
    }
    git_cli::run(dir, &["add", "--", path])?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub ours_label: String,
    pub ours: String,
    pub base: Option<String>,
    pub theirs: String,
    pub theirs_label: String,
    raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Region {
    Clean(String),
    Conflict(Conflict),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Ours,
    Theirs,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub regions: Vec<Region>,
}

impl Parsed {
    pub fn conflicts(&self) -> impl Iterator<Item = &Conflict> {
        self.regions.iter().filter_map(|r| match r {
            Region::Conflict(c) => Some(c),
            Region::Clean(_) => None,
        })
    }

    pub fn conflict_count(&self) -> usize {
        self.conflicts().count()
    }

    /// Joins the file back together. Conflicts without a choice keep their
    /// original marker lines, so the result is only final when every
    /// conflict has one.
    pub fn render(&self, choices: &[Option<Choice>]) -> String {
        let mut out = String::new();
        let mut index = 0;
        for region in &self.regions {
            match region {
                Region::Clean(text) => out.push_str(text),
                Region::Conflict(conflict) => {
                    match choices.get(index).copied().flatten() {
                        Some(Choice::Ours) => out.push_str(&conflict.ours),
                        Some(Choice::Theirs) => out.push_str(&conflict.theirs),
                        Some(Choice::Both) => {
                            out.push_str(&conflict.ours);
                            out.push_str(&conflict.theirs);
                        }
                        None => out.push_str(&conflict.raw),
                    }
                    index += 1;
                }
            }
        }
        out
    }
}

enum Section {
    Ours,
    Base,
    Theirs,
}

fn marker(line: &str, symbol: char) -> Option<&str> {
    let body = line.trim_end_matches(['\n', '\r']);
    let rest = body.strip_prefix(&symbol.to_string().repeat(7))?;
    if rest.is_empty() {
        return Some("");
    }
    rest.strip_prefix(' ').map(str::trim)
}

pub fn parse(text: &str) -> Parsed {
    let mut regions = Vec::new();
    let mut clean = String::new();
    let mut open: Option<(Conflict, Section)> = None;
    for line in text.split_inclusive('\n') {
        let mut closed = false;
        match open.as_mut() {
            None => {
                if let Some(label) = marker(line, '<') {
                    if !clean.is_empty() {
                        regions.push(Region::Clean(std::mem::take(&mut clean)));
                    }
                    open = Some((
                        Conflict {
                            ours_label: label.to_string(),
                            ours: String::new(),
                            base: None,
                            theirs: String::new(),
                            theirs_label: String::new(),
                            raw: line.to_string(),
                        },
                        Section::Ours,
                    ));
                } else {
                    clean.push_str(line);
                }
            }
            Some((conflict, section)) => {
                conflict.raw.push_str(line);
                match section {
                    Section::Ours | Section::Base
                        if marker(line, '=').is_some_and(str::is_empty) =>
                    {
                        *section = Section::Theirs;
                    }
                    Section::Ours if marker(line, '|').is_some() => {
                        conflict.base = Some(String::new());
                        *section = Section::Base;
                    }
                    Section::Theirs if marker(line, '>').is_some() => {
                        conflict.theirs_label = marker(line, '>').unwrap_or_default().to_string();
                        closed = true;
                    }
                    Section::Ours => conflict.ours.push_str(line),
                    Section::Base => {
                        if let Some(base) = conflict.base.as_mut() {
                            base.push_str(line);
                        }
                    }
                    Section::Theirs => conflict.theirs.push_str(line),
                }
            }
        }
        if closed && let Some((done, _)) = open.take() {
            regions.push(Region::Conflict(done));
        }
    }
    if let Some((unfinished, _)) = open {
        clean.push_str(&unfinished.raw);
    }
    if !clean.is_empty() {
        regions.push(Region::Clean(clean));
    }
    merge_clean(&mut regions);
    Parsed { regions }
}

fn merge_clean(regions: &mut Vec<Region>) {
    let mut merged: Vec<Region> = Vec::with_capacity(regions.len());
    for region in regions.drain(..) {
        match (merged.last_mut(), region) {
            (Some(Region::Clean(prev)), Region::Clean(next)) => prev.push_str(&next),
            (_, region) => merged.push(region),
        }
    }
    *regions = merged;
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_WAY: &str =
        "top\n<<<<<<< HEAD\nours 1\nours 2\n=======\ntheirs\n>>>>>>> feat/x\nbottom\n";

    #[test]
    fn two_way_conflict() {
        let parsed = parse(TWO_WAY);
        assert_eq!(parsed.regions.len(), 3);
        let conflict = parsed.conflicts().next().unwrap();
        assert_eq!(conflict.ours_label, "HEAD");
        assert_eq!(conflict.theirs_label, "feat/x");
        assert_eq!(conflict.ours, "ours 1\nours 2\n");
        assert_eq!(conflict.theirs, "theirs\n");
        assert_eq!(conflict.base, None);
    }

    #[test]
    fn choices_render() {
        let parsed = parse(TWO_WAY);
        assert_eq!(
            parsed.render(&[Some(Choice::Ours)]),
            "top\nours 1\nours 2\nbottom\n"
        );
        assert_eq!(
            parsed.render(&[Some(Choice::Theirs)]),
            "top\ntheirs\nbottom\n"
        );
        assert_eq!(
            parsed.render(&[Some(Choice::Both)]),
            "top\nours 1\nours 2\ntheirs\nbottom\n"
        );
        assert_eq!(parsed.render(&[None]), TWO_WAY);
    }

    #[test]
    fn diff3_base_section() {
        let text = "<<<<<<< ours\na\n||||||| base\nb\n=======\nc\n>>>>>>> theirs\n";
        let conflict = parse(text).conflicts().next().cloned().unwrap();
        assert_eq!(conflict.ours, "a\n");
        assert_eq!(conflict.base.as_deref(), Some("b\n"));
        assert_eq!(conflict.theirs, "c\n");
    }

    #[test]
    fn crlf_lines_are_kept() {
        let text = "x\r\n<<<<<<< HEAD\r\na\r\n=======\r\nb\r\n>>>>>>> other\r\ny\r\n";
        let parsed = parse(text);
        let conflict = parsed.conflicts().next().unwrap();
        assert_eq!(conflict.ours, "a\r\n");
        assert_eq!(conflict.theirs_label, "other");
        assert_eq!(parsed.render(&[Some(Choice::Theirs)]), "x\r\nb\r\ny\r\n");
        assert_eq!(parsed.render(&[None]), text);
    }

    #[test]
    fn no_trailing_newline_after_markers() {
        let text = "<<<<<<< HEAD\na\n=======\nb\n>>>>>>> other";
        let parsed = parse(text);
        assert_eq!(parsed.conflict_count(), 1);
        assert_eq!(parsed.render(&[Some(Choice::Ours)]), "a\n");
        assert_eq!(parsed.render(&[None]), text);
    }

    #[test]
    fn marker_look_alikes_stay_text() {
        let text =
            "let s = \"<<<<<<< not a marker\";\n<<<<<<<< eight\n  <<<<<<< indented\n=======\n";
        let parsed = parse(text);
        assert_eq!(parsed.conflict_count(), 0);
        assert_eq!(parsed.render(&[]), text);
    }

    #[test]
    fn unfinished_conflict_is_plain_text() {
        let text = "a\n<<<<<<< HEAD\nb\n=======\nc\n";
        let parsed = parse(text);
        assert_eq!(parsed.conflict_count(), 0);
        assert_eq!(parsed.render(&[]), text);
    }

    #[test]
    fn merge_message_names_the_branch() {
        assert_eq!(
            merged_branch("Merge branch 'feat/x' into main\n\n# Conflicts:").as_deref(),
            Some("feat/x")
        );
        assert_eq!(merged_branch("Merge commit 'abc'"), None);
    }
}
