use std::path::Path;

use eframe::egui::{RichText, Ui};
use kelp_core::ops::Op;
use kelp_core::refs::{RefKind, RefLabel};
use kelp_core::workspace::{Stash, Worktree};

use crate::commands::Command;
use crate::dialogs::{Dialog, NewWorktree};
use crate::theme;

pub struct MenuContext {
    pub current_branch: Option<String>,
    pub repo_dir_name: String,
    pub local_branches: Vec<String>,
    pub upstream: Option<String>,
}

fn item(ui: &mut Ui, label: &str, out: &mut Vec<Command>, command: impl FnOnce() -> Command) {
    if ui.button(label).clicked() {
        out.push(command());
        ui.close();
    }
}

fn danger(ui: &mut Ui, label: &str, out: &mut Vec<Command>, command: impl FnOnce() -> Command) {
    if ui
        .button(RichText::new(label).color(theme::DELETED))
        .clicked()
    {
        out.push(command());
        ui.close();
    }
}

fn heading(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .monospace()
            .size(11.0)
            .color(theme::TEXT_FAINT),
    );
}

pub fn branch(ui: &mut Ui, label: &RefLabel, ctx: &MenuContext, out: &mut Vec<Command>) {
    ui.set_min_width(230.0);
    heading(ui, &label.name);
    let name = label.name.clone();
    let current = ctx.current_branch.as_deref();
    match label.kind {
        RefKind::Local => {
            if current != Some(name.as_str()) {
                item(ui, "Check out", out, || {
                    Command::Run(Op::Switch(name.clone()))
                });
            }
            item(ui, "Open in new worktree…", out, || {
                Command::Open(Dialog::NewWorktree(NewWorktree::new(
                    ctx.repo_dir_name.to_string(),
                    name.clone(),
                    Some(name.clone()),
                    ctx.local_branches.clone(),
                )))
            });
            item(ui, "New branch from here…", out, || {
                new_branch(&name, &name)
            });
            item(ui, "Rename…", out, || {
                Command::Open(Dialog::RenameBranch {
                    from: name.clone(),
                    to: name.clone(),
                })
            });
            ui.separator();
            if let Some(current) = current.filter(|c| *c != name) {
                item(ui, &format!("Merge into {current}"), out, || {
                    confirm(
                        format!("Merge {name} into {current}?"),
                        "",
                        Op::Merge(name.clone()),
                        false,
                    )
                });
                item(ui, &format!("Rebase {current} onto this"), out, || {
                    confirm(
                        format!("Rebase {current} onto {name}?"),
                        "Rewrites the commits on your current branch.",
                        Op::Rebase(name.clone()),
                        false,
                    )
                });
            }
            if current == Some(name.as_str()) {
                item(ui, "Pull", out, || Command::Run(Op::Pull));
            }
            let upstream = if current == Some(name.as_str()) {
                ctx.upstream.clone()
            } else {
                None
            };
            item(ui, "Push", out, || {
                Command::Run(Op::Push {
                    branch: name.clone(),
                    remote: upstream,
                })
            });
            item(ui, "Copy branch name", out, || Command::Copy(name.clone()));
            if current != Some(name.as_str()) {
                ui.separator();
                let remote = label.has_remote.then(|| "origin".to_string());
                danger(ui, "Delete branch…", out, || {
                    Command::Open(Dialog::DeleteBranch {
                        name: name.clone(),
                        force: false,
                        remote,
                        delete_remote: false,
                    })
                });
            }
        }
        RefKind::Remote => {
            item(ui, "Check out", out, || {
                Command::Run(Op::SwitchTrack(name.clone()))
            });
            item(ui, "New branch from here…", out, || {
                let local = name
                    .split_once('/')
                    .map_or(name.as_str(), |(_, b)| b)
                    .to_string();
                new_branch(&local, &name)
            });
            if let Some(current) = current {
                item(ui, &format!("Merge into {current}"), out, || {
                    confirm(
                        format!("Merge {name} into {current}?"),
                        "",
                        Op::Merge(name.clone()),
                        false,
                    )
                });
            }
            item(ui, "Copy branch name", out, || Command::Copy(name.clone()));
            ui.separator();
            if let Some((remote, branch)) = name.split_once('/') {
                let (remote, branch) = (remote.to_string(), branch.to_string());
                danger(ui, "Delete remote branch…", out, || {
                    confirm(
                        format!("Delete {name} on {remote}?"),
                        "This removes the branch for everyone using this remote.",
                        Op::DeleteRemoteBranch {
                            remote: remote.clone(),
                            branch: branch.clone(),
                        },
                        true,
                    )
                });
            }
        }
        RefKind::Tag => {
            item(ui, "Check out", out, || {
                Command::Run(Op::SwitchDetached(name.clone()))
            });
            item(ui, "New branch from here…", out, || new_branch("", &name));
            item(ui, "Copy tag name", out, || Command::Copy(name.clone()));
        }
    }
}

pub fn commit(ui: &mut Ui, id: &str, title: &str, ctx: &MenuContext, out: &mut Vec<Command>) {
    ui.set_min_width(230.0);
    let short = &id[..7.min(id.len())];
    heading(ui, short);
    item(ui, "Check out (detached)", out, || {
        Command::Run(Op::SwitchDetached(id.to_string()))
    });
    item(ui, "New branch here…", out, || new_branch("", id));
    item(ui, "New worktree here…", out, || {
        Command::Open(Dialog::NewWorktree(NewWorktree::new(
            ctx.repo_dir_name.to_string(),
            id.to_string(),
            None,
            ctx.local_branches.clone(),
        )))
    });
    ui.separator();
    item(ui, "Copy commit hash", out, || {
        Command::Copy(id.to_string())
    });
    item(ui, "Copy message", out, || Command::Copy(title.to_string()));
}

pub fn wip(ui: &mut Ui, out: &mut Vec<Command>) {
    ui.set_min_width(200.0);
    item(ui, "Stash all changes", out, || Command::Run(Op::StashPush));
}

pub fn stash(ui: &mut Ui, stash: &Stash, out: &mut Vec<Command>) {
    ui.set_min_width(200.0);
    heading(ui, &stash.name);
    item(ui, "Apply", out, || {
        Command::Run(Op::StashApply(stash.name.clone()))
    });
    item(ui, "Pop", out, || Command::Run(Op::StashPop));
    ui.separator();
    danger(ui, "Drop…", out, || {
        confirm(
            format!("Drop {}?", stash.name),
            &stash.message,
            Op::StashDrop(stash.name.clone()),
            true,
        )
    });
}

pub fn worktree(ui: &mut Ui, tree: &Worktree, out: &mut Vec<Command>) {
    ui.set_min_width(220.0);
    heading(ui, &tree.name());
    item(ui, "Open in new tab", out, || {
        Command::OpenRepo(tree.path.clone())
    });
    item(ui, "Open in terminal", out, || {
        Command::OpenTerminal(tree.path.clone())
    });
    item(ui, "Copy path", out, || {
        Command::Copy(tree.path.display().to_string())
    });
    if !tree.is_main {
        ui.separator();
        danger(ui, "Remove…", out, || remove_worktree(&tree.path));
    }
}

pub fn remove_worktree(path: &Path) -> Command {
    confirm(
        format!(
            "Remove worktree {}?",
            path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        ),
        "Deletes the folder. Uncommitted changes in it are lost.",
        Op::WorktreeRemove {
            path: path.display().to_string(),
            force: false,
        },
        true,
    )
}

fn new_branch(name: &str, start: &str) -> Command {
    Command::Open(Dialog::NewBranch {
        name: name.to_string(),
        start: start.to_string(),
        start_label: if start.len() == 40 {
            start[..7].to_string()
        } else {
            start.to_string()
        },
        switch: true,
    })
}

fn confirm(title: String, body: &str, op: Op, danger: bool) -> Command {
    Command::Open(Dialog::Confirm {
        title,
        body: body.to_string(),
        op,
        danger,
    })
}
