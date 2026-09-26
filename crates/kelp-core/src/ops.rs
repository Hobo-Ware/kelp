use std::path::Path;

use crate::git_cli;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Fetch,
    Pull,
    Push {
        branch: String,
        remote: Option<String>,
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
}

impl Op {
    pub fn args(&self) -> Vec<String> {
        let v = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        match self {
            Op::Fetch => v(&["fetch", "--all", "--prune"]),
            Op::Pull => v(&["pull"]),
            Op::Push {
                branch,
                remote: Some(remote),
            } => v(&["push", remote, branch]),
            Op::Push {
                branch,
                remote: None,
            } => v(&["push", "-u", "origin", branch]),
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
        }
    }

    pub fn label(&self) -> String {
        match self {
            Op::Fetch => "Fetching".into(),
            Op::Pull => "Pulling".into(),
            Op::Push { branch, .. } => format!("Pushing {branch}"),
            Op::Switch(b) | Op::SwitchTrack(b) => format!("Checking out {b}"),
            Op::SwitchDetached(c) => format!("Checking out {}", short(c)),
            Op::CreateBranch { name, .. } => format!("Creating {name}"),
            Op::RenameBranch { to, .. } => format!("Renaming to {to}"),
            Op::DeleteBranch { name, .. } => format!("Deleting {name}"),
            Op::DeleteRemoteBranch { remote, branch } => format!("Deleting {remote}/{branch}"),
            Op::Merge(what) => format!("Merging {what}"),
            Op::Rebase(onto) => format!("Rebasing onto {onto}"),
            Op::StashPush => "Stashing".into(),
            Op::StashPop | Op::StashApply(_) => "Applying stash".into(),
            Op::StashDrop(_) => "Dropping stash".into(),
            Op::WorktreeAdd { path, .. } => format!("Adding worktree {path}"),
            Op::WorktreeRemove { path, .. } => format!("Removing worktree {path}"),
            Op::WorktreePrune => "Pruning worktrees".into(),
        }
    }

    pub fn command_line(&self) -> String {
        let args = self.args();
        git_cli::command_line(&args.iter().map(String::as_str).collect::<Vec<_>>())
    }

    pub fn run(&self, dir: &Path) -> anyhow::Result<String> {
        let args = self.args();
        let out = git_cli::run(dir, &args.iter().map(String::as_str).collect::<Vec<_>>())?;
        Ok(out.trim().to_string())
    }
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(7)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_sets_upstream_only_when_missing() {
        let with = Op::Push {
            branch: "main".into(),
            remote: Some("origin".into()),
        };
        let without = Op::Push {
            branch: "feat/x".into(),
            remote: None,
        };
        assert_eq!(with.command_line(), "git push origin main");
        assert_eq!(without.command_line(), "git push -u origin feat/x");
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
