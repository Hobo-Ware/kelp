use std::path::Path;

use crate::git_cli;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Fetch,
    Pull,
    Push {
        branch: String,
        remote: String,
        set_upstream: bool,
        force_with_lease: bool,
    },
    Switch(String),
    SwitchTrack(String),
    SwitchDetached(String),
    CreateBranch {
        name: String,
        start: String,
        switch: bool,
    },
    RenameBranch {
        from: String,
        to: String,
    },
    DeleteBranch {
        name: String,
        force: bool,
    },
    DeleteRemoteBranch {
        remote: String,
        branch: String,
    },
    Merge(String),
    CheckoutAndMerge {
        branch: String,
        source: String,
    },
    Rebase(String),
    StashPush,
    StashPop,
    StashApply(String),
    StashDrop(String),
    WorktreeAdd {
        path: String,
        new_branch: Option<String>,
        start: String,
    },
    WorktreeRemove {
        path: String,
        force: bool,
    },
    WorktreePrune,
    Stage(Vec<String>),
    Unstage {
        paths: Vec<String>,
        has_head: bool,
    },
    StageAll,
    UnstageAll {
        has_head: bool,
    },
    DiscardChanges(Vec<String>),
    DeleteUntracked(Vec<String>),
    ApplyToIndex {
        patch: String,
        reverse: bool,
    },
    DiscardPatch(String),
    DiscardFiles {
        tracked: Vec<String>,
        untracked: Vec<String>,
    },
    StashFiles {
        paths: Vec<String>,
        message: String,
    },
    StashStaged {
        message: String,
    },
    Commit {
        message: String,
        amend: bool,
    },
    CherryPick {
        commit: String,
        merge: bool,
    },
    Revert {
        commit: String,
        merge: bool,
    },
    Reset {
        commit: String,
        mode: ResetMode,
    },
    RenameRemoteBranch {
        remote: String,
        from: String,
        to: String,
        tracking: Option<String>,
    },
    StashRename {
        stash: String,
        message: String,
    },
    WorktreeMove {
        from: String,
        to: String,
    },
    CreateTag {
        name: String,
        commit: String,
        message: Option<String>,
    },
    DeleteTag(String),
    SubmoduleUpdate {
        path: Option<String>,
        init: bool,
    },
    PushTag {
        remote: String,
        name: String,
    },
    PushTags(String),
    DeleteRemoteTag {
        remote: String,
        name: String,
    },
    AddRemote {
        name: String,
        url: String,
    },
    RenameRemote {
        from: String,
        to: String,
    },
    RemoveRemote(String),
    SetRemoteUrl {
        name: String,
        url: String,
    },
    FetchRemote(String),
    PruneRemote(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
    Keep,
}

impl ResetMode {
    pub fn flag(self) -> &'static str {
        match self {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
            ResetMode::Keep => "--keep",
        }
    }
}

impl Op {
    pub fn args(&self) -> Vec<String> {
        let v = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        match self {
            Op::Fetch => v(&["fetch", "--all", "--prune"]),
            Op::Pull => v(&["pull"]),
            Op::Push {
                branch,
                remote,
                set_upstream,
                force_with_lease,
            } => {
                let mut args = v(&["push"]);
                if *force_with_lease {
                    args.push("--force-with-lease".into());
                }
                if *set_upstream {
                    args.push("-u".into());
                }
                args.extend([remote.clone(), branch.clone()]);
                args
            }
            Op::Switch(branch) => v(&["switch", branch]),
            Op::SwitchTrack(remote_branch) => v(&["switch", "--track", remote_branch]),
            Op::SwitchDetached(commit) => v(&["switch", "--detach", commit]),
            Op::CreateBranch {
                name,
                start,
                switch: true,
            } => v(&["switch", "-c", name, start]),
            Op::CreateBranch {
                name,
                start,
                switch: false,
            } => v(&["branch", name, start]),
            Op::RenameBranch { from, to } => v(&["branch", "-m", from, to]),
            Op::DeleteBranch { name, force } => {
                v(&["branch", if *force { "-D" } else { "-d" }, name])
            }
            Op::DeleteRemoteBranch { remote, branch } => v(&["push", remote, "--delete", branch]),
            Op::Merge(what) => v(&["merge", what]),
            Op::CheckoutAndMerge { branch, .. } => v(&["switch", branch]),
            Op::Rebase(onto) => v(&["rebase", onto]),
            Op::StashPush => v(&["stash", "push", "--include-untracked"]),
            Op::StashPop => v(&["stash", "pop"]),
            Op::StashApply(stash) => v(&["stash", "apply", stash]),
            Op::StashDrop(stash) => v(&["stash", "drop", stash]),
            Op::WorktreeAdd {
                path,
                new_branch: Some(name),
                start,
            } => v(&["worktree", "add", "-b", name, path, start]),
            Op::WorktreeAdd {
                path,
                new_branch: None,
                start,
            } => v(&["worktree", "add", path, start]),
            Op::WorktreeRemove { path, force: true } => v(&["worktree", "remove", "--force", path]),
            Op::WorktreeRemove { path, force: false } => v(&["worktree", "remove", path]),
            Op::WorktreePrune => v(&["worktree", "prune"]),
            Op::Stage(paths) => with_paths(&["add", "--"], paths),
            Op::Unstage {
                paths,
                has_head: true,
            } => with_paths(&["restore", "--staged", "--"], paths),
            Op::Unstage {
                paths,
                has_head: false,
            } => with_paths(&["rm", "--cached", "-r", "-q", "--"], paths),
            Op::StageAll => v(&["add", "-A"]),
            Op::UnstageAll { has_head: true } => v(&["reset", "-q"]),
            Op::UnstageAll { has_head: false } => v(&["rm", "--cached", "-r", "-q", "."]),
            Op::DiscardChanges(paths) => with_paths(&["restore", "--"], paths),
            Op::DeleteUntracked(paths) => with_paths(&["clean", "-f", "--"], paths),
            Op::ApplyToIndex { reverse: false, .. } => {
                v(&["apply", "--cached", "--whitespace=nowarn", "-"])
            }
            Op::ApplyToIndex { reverse: true, .. } => {
                v(&["apply", "--cached", "--reverse", "--whitespace=nowarn", "-"])
            }
            Op::DiscardPatch(_) => v(&["apply", "--reverse", "--whitespace=nowarn", "-"]),
            Op::DiscardFiles { tracked, .. } if !tracked.is_empty() => {
                with_paths(&["restore", "--"], tracked)
            }
            Op::DiscardFiles { untracked, .. } => with_paths(&["clean", "-f", "--"], untracked),
            Op::StashFiles { paths, message } => {
                let mut args = v(&["stash", "push", "--include-untracked"]);
                if !message.trim().is_empty() {
                    args.extend(["-m".to_string(), message.trim().to_string()]);
                }
                args.push("--".to_string());
                args.extend(paths.iter().cloned());
                args
            }
            Op::StashStaged { message } => {
                let mut args = v(&["stash", "push", "--staged"]);
                if !message.trim().is_empty() {
                    args.extend(["-m".to_string(), message.trim().to_string()]);
                }
                args
            }
            Op::Commit {
                message,
                amend: false,
            } => v(&["commit", "-m", message]),
            Op::Commit {
                message,
                amend: true,
            } => v(&["commit", "--amend", "-m", message]),
            Op::CherryPick { commit, merge } => mainline(&["cherry-pick"], *merge, commit),
            Op::Revert { commit, merge } => mainline(&["revert", "--no-edit"], *merge, commit),
            Op::Reset { commit, mode } => v(&["reset", mode.flag(), commit]),
            Op::RenameRemoteBranch {
                remote, from, to, ..
            } => v(&[
                "push",
                remote,
                &format!("refs/remotes/{remote}/{from}:refs/heads/{to}"),
            ]),
            Op::StashRename { stash, .. } => v(&["rev-parse", stash]),
            Op::WorktreeMove { from, to } => v(&["worktree", "move", from, to]),
            Op::CreateTag {
                name,
                commit,
                message: None,
            } => v(&["tag", name, commit]),
            Op::CreateTag {
                name,
                commit,
                message: Some(message),
            } => v(&["tag", "-a", name, "-m", message, commit]),
            Op::DeleteTag(name) => v(&["tag", "-d", name]),
            Op::SubmoduleUpdate { path, init } => {
                let mut args = vec!["submodule".to_string(), "update".to_string()];
                if *init {
                    args.push("--init".into());
                }
                args.push("--recursive".into());
                if let Some(path) = path {
                    args.extend(["--".into(), path.clone()]);
                }
                args
            }
            Op::PushTag { remote, name } => v(&["push", remote, &format!("refs/tags/{name}")]),
            Op::PushTags(remote) => v(&["push", remote, "--tags"]),
            Op::DeleteRemoteTag { remote, name } => {
                v(&["push", remote, "--delete", &format!("refs/tags/{name}")])
            }
            Op::AddRemote { name, url } => v(&["remote", "add", name, url]),
            Op::RenameRemote { from, to } => v(&["remote", "rename", from, to]),
            Op::RemoveRemote(name) => v(&["remote", "remove", name]),
            Op::SetRemoteUrl { name, url } => v(&["remote", "set-url", name, url]),
            Op::FetchRemote(name) => v(&["fetch", "--prune", name]),
            Op::PruneRemote(name) => v(&["remote", "prune", name]),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Op::Fetch => "Fetching".into(),
            Op::Pull => "Pulling".into(),
            Op::Push {
                branch,
                force_with_lease: true,
                ..
            } => format!("Force pushing {branch}"),
            Op::Push { branch, .. } => format!("Pushing {branch}"),
            Op::Switch(b) | Op::SwitchTrack(b) => format!("Checking out {b}"),
            Op::SwitchDetached(c) => format!("Checking out {}", short(c)),
            Op::CreateBranch { name, .. } => format!("Creating {name}"),
            Op::RenameBranch { to, .. } => format!("Renaming to {to}"),
            Op::DeleteBranch { name, .. } => format!("Deleting {name}"),
            Op::DeleteRemoteBranch { remote, branch } => format!("Deleting {remote}/{branch}"),
            Op::Merge(what) => format!("Merging {what}"),
            Op::CheckoutAndMerge { branch, source } => format!("Merging {source} into {branch}"),
            Op::Rebase(onto) => format!("Rebasing onto {onto}"),
            Op::StashPush => "Stashing".into(),
            Op::StashPop | Op::StashApply(_) => "Applying stash".into(),
            Op::StashDrop(_) => "Dropping stash".into(),
            Op::WorktreeAdd { path, .. } => format!("Adding worktree {path}"),
            Op::WorktreeRemove { path, .. } => format!("Removing worktree {path}"),
            Op::WorktreePrune => "Pruning worktrees".into(),
            Op::Stage(paths) => format!("Staging {}", count(paths)),
            Op::Unstage { paths, .. } => format!("Unstaging {}", count(paths)),
            Op::StageAll => "Staging all changes".into(),
            Op::UnstageAll { .. } => "Unstaging all changes".into(),
            Op::DiscardChanges(paths) | Op::DeleteUntracked(paths) => {
                format!("Discarding {}", count(paths))
            }
            Op::ApplyToIndex { reverse: false, .. } => "Staging hunk".into(),
            Op::ApplyToIndex { reverse: true, .. } => "Unstaging hunk".into(),
            Op::DiscardPatch(_) => "Discarding lines".into(),
            Op::DiscardFiles { tracked, untracked } => {
                format!(
                    "Discarding {}",
                    count(&[tracked.clone(), untracked.clone()].concat())
                )
            }
            Op::StashFiles { paths, .. } => format!("Stashing {}", count(paths)),
            Op::StashStaged { .. } => "Stashing staged changes".into(),
            Op::Commit { amend: false, .. } => "Committing".into(),
            Op::Commit { amend: true, .. } => "Amending".into(),
            Op::CherryPick { commit, .. } => format!("Cherry-picking {}", short(commit)),
            Op::Revert { commit, .. } => format!("Reverting {}", short(commit)),
            Op::Reset { commit, .. } => format!("Resetting to {}", short(commit)),
            Op::RenameRemoteBranch {
                remote, from, to, ..
            } => format!("Renaming {remote}/{from} to {to}"),
            Op::StashRename { stash, .. } => format!("Renaming {stash}"),
            Op::WorktreeMove { to, .. } => format!("Moving worktree to {to}"),
            Op::CreateTag { name, .. } => format!("Tagging {name}"),
            Op::DeleteTag(name) => format!("Deleting tag {name}"),
            Op::SubmoduleUpdate { init: true, .. } => "Initializing submodules".into(),
            Op::SubmoduleUpdate {
                path: Some(path), ..
            } => format!("Updating {path}"),
            Op::SubmoduleUpdate { .. } => "Updating submodules".into(),
            Op::PushTag { remote, name } => format!("Pushing tag {name} to {remote}"),
            Op::PushTags(remote) => format!("Pushing tags to {remote}"),
            Op::DeleteRemoteTag { remote, name } => format!("Deleting tag {name} on {remote}"),
            Op::AddRemote { name, .. } => format!("Adding remote {name}"),
            Op::RenameRemote { to, .. } => format!("Renaming remote to {to}"),
            Op::RemoveRemote(name) => format!("Removing remote {name}"),
            Op::SetRemoteUrl { name, .. } => format!("Changing the URL of {name}"),
            Op::FetchRemote(name) => format!("Fetching {name}"),
            Op::PruneRemote(name) => format!("Pruning {name}"),
        }
    }

    pub fn command_line(&self) -> String {
        let steps = self.steps();
        steps
            .iter()
            .map(|args| git_cli::command_line(&args.iter().map(String::as_str).collect::<Vec<_>>()))
            .collect::<Vec<_>>()
            .join(" && ")
    }

    fn steps(&self) -> Vec<Vec<String>> {
        let v = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        match self {
            Op::CheckoutAndMerge { branch, source } => {
                vec![v(&["switch", branch]), v(&["merge", source])]
            }
            Op::RenameRemoteBranch {
                remote,
                from,
                to,
                tracking,
            } => {
                let mut steps = vec![self.args(), v(&["push", remote, "--delete", from])];
                if let Some(local) = tracking {
                    steps.push(v(&[
                        "branch",
                        &format!("--set-upstream-to={remote}/{to}"),
                        local,
                    ]));
                }
                steps
            }
            Op::StashRename { stash, message } => vec![
                self.args(),
                v(&["stash", "drop", "-q", stash]),
                v(&["stash", "store", "-m", message, "<sha>"]),
            ],
            Op::DiscardFiles { tracked, untracked }
                if !tracked.is_empty() && !untracked.is_empty() =>
            {
                vec![
                    Op::DiscardChanges(tracked.clone()).args(),
                    Op::DeleteUntracked(untracked.clone()).args(),
                ]
            }
            _ => vec![self.args()],
        }
    }

    pub fn run(&self, dir: &Path) -> anyhow::Result<String> {
        let args = self.args();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = match self {
            Op::ApplyToIndex { patch, .. } | Op::DiscardPatch(patch) => {
                git_cli::run_with_stdin(dir, &args, patch)?
            }
            Op::StashRename { stash, message } => {
                let sha = git_cli::run(dir, &args)?.trim().to_string();
                git_cli::run(dir, &["stash", "drop", "-q", stash])?;
                git_cli::run(dir, &["stash", "store", "-m", message, &sha])?
            }
            Op::Commit { .. } => git_cli::run(dir, &args).map_err(|e| {
                match crate::signing::explain_failure(&e.to_string()) {
                    Some(hint) => anyhow::anyhow!("{hint}\n\n{e}"),
                    None => e,
                }
            })?,
            _ => {
                let mut out = String::new();
                for step in self.steps() {
                    let step: Vec<&str> = step.iter().map(String::as_str).collect();
                    out = git_cli::run(dir, &step)?;
                }
                out
            }
        };
        Ok(out.trim().to_string())
    }
}

pub fn push_rejected(error: &str) -> bool {
    error.contains("[rejected]")
        && (error.contains("non-fast-forward") || error.contains("fetch first"))
}

fn mainline(prefix: &[&str], merge: bool, commit: &str) -> Vec<String> {
    let mut args: Vec<String> = prefix.iter().map(|s| s.to_string()).collect();
    if merge {
        args.extend(["-m".to_string(), "1".to_string()]);
    }
    args.push(commit.to_string());
    args
}

fn with_paths(prefix: &[&str], paths: &[String]) -> Vec<String> {
    prefix
        .iter()
        .map(|s| s.to_string())
        .chain(paths.iter().cloned())
        .collect()
}

fn count(paths: &[String]) -> String {
    match paths {
        [one] => one.clone(),
        many => format!("{} files", many.len()),
    }
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(7)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push(remote: &str, branch: &str, set_upstream: bool, force_with_lease: bool) -> Op {
        Op::Push {
            branch: branch.into(),
            remote: remote.into(),
            set_upstream,
            force_with_lease,
        }
    }

    #[test]
    fn push_sets_upstream_and_forces_with_lease_on_request() {
        assert_eq!(
            push("origin", "main", false, false).command_line(),
            "git push origin main"
        );
        assert_eq!(
            push("fork", "feat/x", true, false).command_line(),
            "git push -u fork feat/x"
        );
        assert_eq!(
            push("origin", "main", false, true).command_line(),
            "git push --force-with-lease origin main"
        );
        assert_eq!(
            push("origin", "main", false, true).label(),
            "Force pushing main"
        );
    }

    #[test]
    fn rejected_pushes_are_recognised() {
        let behind = " ! [rejected]        main -> main (non-fast-forward)\nerror: failed to push";
        let fetch_first = " ! [rejected]        main -> main (fetch first)";
        let hook = " ! [remote rejected] main -> main (pre-receive hook declined)";
        assert!(push_rejected(behind));
        assert!(push_rejected(fetch_first));
        assert!(!push_rejected(hook));
    }

    #[test]
    fn history_rewrites_match_git() {
        let pick = |merge| Op::CherryPick {
            commit: "abc1234def".into(),
            merge,
        };
        assert_eq!(pick(false).command_line(), "git cherry-pick abc1234def");
        assert_eq!(pick(true).command_line(), "git cherry-pick -m 1 abc1234def");
        assert_eq!(
            Op::Revert {
                commit: "abc1234def".into(),
                merge: false
            }
            .command_line(),
            "git revert --no-edit abc1234def"
        );
        for (mode, flag) in [
            (ResetMode::Soft, "--soft"),
            (ResetMode::Mixed, "--mixed"),
            (ResetMode::Hard, "--hard"),
        ] {
            let op = Op::Reset {
                commit: "abc1234def".into(),
                mode,
            };
            assert_eq!(op.command_line(), format!("git reset {flag} abc1234def"));
            assert_eq!(op.label(), "Resetting to abc1234");
        }
    }

    #[test]
    fn branch_commands_match_git() {
        let create = Op::CreateBranch {
            name: "feat/y".into(),
            start: "abc1234".into(),
            switch: true,
        };
        assert_eq!(create.command_line(), "git switch -c feat/y abc1234");
        assert_eq!(
            Op::DeleteBranch {
                name: "old".into(),
                force: true
            }
            .command_line(),
            "git branch -D old"
        );
        assert_eq!(
            Op::SwitchTrack("origin/feat".into()).command_line(),
            "git switch --track origin/feat"
        );
    }

    #[test]
    fn worktree_add_matches_the_dialog_preview() {
        let op = Op::WorktreeAdd {
            path: "../kelp-stash-view".into(),
            new_branch: Some("feat/stash-view".into()),
            start: "main".into(),
        };
        assert_eq!(
            op.command_line(),
            "git worktree add -b feat/stash-view ../kelp-stash-view main"
        );
    }
}
