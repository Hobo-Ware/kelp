use std::path::Path;

use eframe::egui;
use gix::ObjectId;
use kelp_core::ops::{Op, ResetMode};
use kelp_core::refs::RefKind;

use super::{Center, Repo, Selection, reveal_in_finder};
use crate::actions::{RepoAction, State};
use crate::commands::Command;
use crate::dialogs::{Dialog, NewWorktree};
use crate::menus::{self, Entry};
use crate::palette::BranchItem;

impl Repo {
    pub fn action_state(&self) -> State {
        State {
            repo: true,
            branch: self.current_branch().is_some(),
            workdir: self.workdir.is_some(),
            changes: !self.wip.is_empty(),
            unstaged: !self.status.unstaged.is_empty(),
            staged: !self.status.staged.is_empty(),
            stashes: !self.workspace.stashes.is_empty(),
            can_undo: self.can_undo(),
            can_redo: self.undo.next_redo().is_some(),
            commit_selected: self.selected_commit().is_some(),
            comparing: self.compare.is_some() || self.is_picking_compare(),
            pull: self.current_pull().is_some(),
            github: self.github.is_some(),
            filtering: self.view.is_filtering(),
            file_open: self.open_file().is_some(),
            tabs: 0,
        }
    }

    pub fn palette_branches(&self) -> Vec<BranchItem> {
        let current = self.current_branch();
        let mut branches: Vec<BranchItem> = [RefKind::Local, RefKind::Remote]
            .into_iter()
            .flat_map(|kind| self.history.refs.of_kind(kind))
            .map(|label| BranchItem {
                name: label.name.clone(),
                remote: label.kind == RefKind::Remote,
                current: label.kind == RefKind::Local && Some(label.name.as_str()) == current,
            })
            .collect();
        branches.dedup_by(|a, b| a.name == b.name);
        branches
    }

    pub fn palette_files(&self) -> Vec<String> {
        match self.selected {
            Some(Selection::Wip) => self.wip.iter().map(|c| c.path.clone()).collect(),
            _ => self
                .details
                .as_ref()
                .map(|d| d.changes.iter().map(|c| c.path.clone()).collect())
                .unwrap_or_default(),
        }
    }

    pub fn palette_history(&self) -> (&Path, &[ObjectId]) {
        (&self.dir, self.history.ids())
    }

    pub fn branch_entries(&self, name: &str) -> Option<Vec<Entry>> {
        let label = [RefKind::Local, RefKind::Remote]
            .into_iter()
            .flat_map(|kind| self.history.refs.of_kind(kind))
            .find(|l| l.name == name)?;
        Some(menus::branch_entries(label, &self.menu_context()))
    }

    pub fn check_out(&mut self, branch: &BranchItem) {
        if branch.current {
            return;
        }
        let op = if branch.remote {
            Op::SwitchTrack(branch.name.clone())
        } else {
            Op::Switch(branch.name.clone())
        };
        self.run_op(op);
    }

    pub fn reveal_commit(&mut self, id: ObjectId) {
        match self.history.row(&id) {
            Some(row) => self.reveal(Selection::Commit(row)),
            None => self.notify("That commit is no longer in the graph", true),
        }
    }

    fn open_file(&self) -> Option<String> {
        match &self.center {
            Center::Diff(view) => Some(view.path().to_string()),
            Center::FileHistory(view) => Some(view.path.clone()),
            _ => None,
        }
    }

    fn selected_commit(&self) -> Option<(String, bool)> {
        match self.selected {
            Some(Selection::Commit(row)) => Some((
                self.history.id(row).to_string(),
                self.history.parents(row).len() > 1,
            )),
            _ => None,
        }
    }

    pub fn run_action(&mut self, ctx: &egui::Context, action: RepoAction) {
        let command = match action {
            RepoAction::Fetch => Some(Command::Run(Op::Fetch)),
            RepoAction::Pull => Some(Command::Run(Op::Pull)),
            RepoAction::Push => self.current_branch().map(|b| Command::Push(b.to_string())),
            RepoAction::NewBranch => {
                let (start, start_label) = self.selected_ref();
                Some(Command::Open(Dialog::NewBranch {
                    name: String::new(),
                    start_label,
                    start,
                    switch: true,
                }))
            }
            RepoAction::NewWorktree => {
                let (start, _) = self.selected_ref();
                let menu = self.menu_context();
                Some(Command::Open(Dialog::NewWorktree(NewWorktree::new(
                    menu.repo_dir_name,
                    start,
                    None,
                    menu.local_branches,
                ))))
            }
            RepoAction::Stash => Some(Command::Run(Op::StashPush)),
            RepoAction::Pop => Some(Command::Run(Op::StashPop)),
            RepoAction::Undo => Some(Command::Undo),
            RepoAction::Redo => {
                self.redo_last();
                None
            }
            RepoAction::Refresh => {
                self.refresh_status();
                self.reload();
                self.refresh_workspace();
                None
            }
            RepoAction::Search => {
                self.search.open = true;
                self.center = Center::Graph;
                None
            }
            RepoAction::ShowGraph => {
                self.center = Center::Graph;
                None
            }
            RepoAction::ShowWorktrees => Some(Command::ShowWorktrees),
            RepoAction::ShowReflog => Some(Command::ShowReflog("HEAD".into())),
            RepoAction::ShowConsole => Some(Command::ShowConsole(None)),
            RepoAction::ShowChanges => Some(Command::Reveal(Selection::Wip)),
            RepoAction::StageAll => Some(Command::Run(Op::StageAll)),
            RepoAction::UnstageAll => Some(Command::Run(Op::UnstageAll {
                has_head: self.has_head(),
            })),
            RepoAction::ShowAllBranches => Some(Command::ShowAllRefs),
            RepoAction::CheckoutCommit => self
                .selected_commit()
                .map(|(id, _)| Command::Run(Op::SwitchDetached(id))),
            RepoAction::CherryPick => self
                .selected_commit()
                .map(|(commit, merge)| Command::Run(Op::CherryPick { commit, merge })),
            RepoAction::Revert => self
                .selected_commit()
                .map(|(commit, merge)| Command::Run(Op::Revert { commit, merge })),
            RepoAction::InteractiveRebase => self
                .selected_commit()
                .map(|(id, _)| Command::OpenRebase(id)),
            RepoAction::FileHistory => self.open_file().map(Command::FileHistory),
            RepoAction::Blame => self.open_file().map(Command::Blame),
            RepoAction::EditMessage => self
                .selected_commit()
                .map(|(id, _)| Command::EditMessage(id)),
            RepoAction::CopyCommitHash => self.selected_commit().map(|(id, _)| Command::Copy(id)),
            RepoAction::CopyBranchName => {
                self.current_branch().map(|b| Command::Copy(b.to_string()))
            }
            RepoAction::RenameBranch => self
                .current_branch()
                .map(|b| Command::StartRename(b.to_string())),
            RepoAction::CreateTag => self.selected_commit().map(|(id, _)| {
                let menu = self.menu_context();
                Command::Open(crate::dialogs::Dialog::NewTag {
                    commit_label: id[..7.min(id.len())].to_string(),
                    commit: id,
                    name: String::new(),
                    annotated: false,
                    message: String::new(),
                    push: false,
                    remote: menu.tag_remote(),
                    remotes: menu.remotes.clone(),
                })
            }),
            RepoAction::PushTags => crate::menus::push_all_tags(&self.menu_context()),
            RepoAction::AddRemote => Some(crate::menus::add_remote()),
            RepoAction::FilterCommits => {
                self.filter.toggle();
                self.center = Center::Graph;
                None
            }
            RepoAction::CompareWithHead => self.selected_commit().map(|(id, _)| Command::Compare {
                base: id,
                target: Some("HEAD".into()),
            }),
            RepoAction::CompareWithWorkTree => {
                self.selected_commit().map(|(id, _)| Command::Compare {
                    base: id,
                    target: None,
                })
            }
            RepoAction::StopCompare => {
                self.stop_compare();
                None
            }
            RepoAction::ResetSoft | RepoAction::ResetMixed => {
                let mode = if action == RepoAction::ResetSoft {
                    ResetMode::Soft
                } else {
                    ResetMode::Mixed
                };
                self.selected_commit()
                    .map(|(commit, _)| Command::Run(Op::Reset { commit, mode }))
            }
            RepoAction::ResetHard => self.selected_commit().map(|(id, _)| Command::ResetHard(id)),
            RepoAction::NextChange | RepoAction::PreviousChange => {
                if let Center::Diff(view) = &mut self.center {
                    view.jump_to_change(action == RepoAction::NextChange);
                }
                None
            }
            RepoAction::OpenFileInEditor => self.open_file().map(Command::OpenInEditor),
            RepoAction::RevealFile => self.open_file().map(Command::RevealFile),
            RepoAction::CopyFilePath => self.open_file().map(Command::Copy),
            RepoAction::ShowLatestStash => self
                .workspace
                .stashes
                .first()
                .map(|s| Command::ShowStash(s.name.clone())),
            RepoAction::OpenTerminal => self.workdir.clone().map(Command::OpenTerminal),
            RepoAction::ShowPulls => Some(Command::ShowPulls),
            RepoAction::OpenPullRequest => {
                self.current_pull().map(|p| Command::OpenUrl(p.url.clone()))
            }
            RepoAction::CreatePullRequest => self
                .current_branch()
                .map(|b| Command::CreatePullRequest(b.to_string())),
            RepoAction::RevealRepo => {
                if let Err(e) = reveal_in_finder(&self.dir) {
                    self.notify(format!("Could not reveal the repository: {e}"), true);
                }
                None
            }
        };
        if let Some(command) = command {
            self.execute(ctx, vec![command]);
        }
    }
}
