use std::path::Path;

use eframe::egui::{Align2, Color32, FontId, Rect, Sense, Stroke, Ui, pos2, vec2};
use kelp_core::ops::{Op, ResetMode};
use kelp_core::refs::{RefKind, RefLabel};
use kelp_core::workspace::{Stash, Worktree};

use crate::commands::Command;
use crate::dialogs::{Dialog, NewWorktree, TextAction, TextDialog};
use crate::icons::{self, Icon};
use crate::ref_labels::{self, DropChoice, DropPlan};
use crate::theme;

pub struct MenuContext {
    pub current_branch: Option<String>,
    pub repo_dir_name: String,
    pub local_branches: Vec<String>,
    pub head: Option<String>,
    pub on_github: bool,
    pub remotes: Vec<String>,
    pub upstreams: Vec<(String, String)>,
}

impl MenuContext {
    pub fn tracking(&self, remote_branch: &str) -> Option<String> {
        self.upstreams
            .iter()
            .find(|(_, upstream)| upstream == remote_branch)
            .map(|(local, _)| local.clone())
    }

    pub fn push_remote(&self) -> Option<String> {
        kelp_core::workspace::default_push_remote(&self.remotes)
    }
}

const ROW_H: f32 = 30.0;

pub fn row(ui: &mut Ui, icon: Option<Icon>, label: &str, hint: Option<&str>, danger: bool) -> bool {
    let width = ui.available_width().max(ui.min_rect().width());
    let (rect, response) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
    let painter = ui.painter_at(rect.expand(1.0));
    let hovered = response.hovered();
    if hovered {
        let fill = if danger {
            theme::with_alpha(theme::DELETED, 0x22)
        } else {
            theme::MENU_HOVER
        };
        painter.rect_filled(rect, 6.0, fill);
    }
    let text = match (danger, hovered) {
        (true, _) => theme::DELETED,
        (false, true) => theme::TEXT_STRONG,
        (false, false) => Color32::from_rgb(0xd5, 0xd7, 0xdc),
    };
    let icon_color = if danger {
        theme::DELETED
    } else if hovered {
        theme::TEXT_STRONG
    } else {
        theme::TEXT_MUTED
    };
    if let Some(icon) = icon {
        let icon_rect =
            Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(15.0, 15.0));
        icons::paint(&painter, icon_rect, icon, icon_color);
    }
    painter.text(
        pos2(rect.left() + 36.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        text,
    );
    if let Some(hint) = hint {
        painter.text(
            pos2(rect.right() - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            hint,
            FontId::proportional(11.5),
            theme::TEXT_FAINT,
        );
    }
    if response.clicked() {
        ui.close();
        return true;
    }
    false
}

fn item(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    out: &mut Vec<Command>,
    command: impl FnOnce() -> Command,
) {
    if row(ui, Some(icon), label, None, false) {
        out.push(command());
    }
}

fn danger(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    out: &mut Vec<Command>,
    command: impl FnOnce() -> Command,
) {
    if row(ui, Some(icon), label, None, true) {
        out.push(command());
    }
}

pub fn heading(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
    let g = crate::graph_view::truncated(
        ui.painter(),
        text.to_string(),
        FontId::monospace(11.0),
        theme::TEXT_FAINT,
        rect.width() - 20.0,
    );
    ui.painter().galley(
        pos2(rect.left() + 10.0, rect.center().y - g.size().y / 2.0),
        g,
        theme::TEXT_FAINT,
    );
}

pub fn separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(
        rect.left() + 8.0..=rect.right() - 8.0,
        rect.center().y,
        Stroke::new(1.0, Color32::from_rgb(0x2c, 0x32, 0x3d)),
    );
}

pub fn menu_width(ui: &mut Ui, width: f32) {
    ui.set_min_width(width);
    ui.set_max_width(width);
    ui.spacing_mut().item_spacing.y = 0.0;
}

pub fn branch(ui: &mut Ui, label: &RefLabel, ctx: &MenuContext, out: &mut Vec<Command>) {
    menu_width(ui, 230.0);
    heading(ui, &label.name);
    branch_items(&mut UiSink { ui, out }, label, ctx);
}

pub fn branch_entries(label: &RefLabel, ctx: &MenuContext) -> Vec<Entry> {
    let mut entries = Vec::new();
    branch_items(&mut entries, label, ctx);
    entries
}

pub struct Entry {
    pub icon: Icon,
    pub label: String,
    pub danger: bool,
    pub command: Command,
}

fn pull_items(sink: &mut dyn Sink, label: &RefLabel, ctx: &MenuContext) {
    if let Some(pull) = &label.pull {
        let url = pull.url.clone();
        sink.item(
            Icon::Merge,
            &format!("Open pull request #{}", pull.number),
            &|| Command::OpenUrl(url.clone()),
        );
        return;
    }
    let branch = match label.kind {
        RefKind::Local if label.has_remote => label.name.clone(),
        RefKind::Remote => match label.name.split_once('/') {
            Some((_, branch)) => branch.to_string(),
            None => return,
        },
        _ => return,
    };
    if ctx.on_github {
        sink.item(Icon::Merge, "Create pull request…", &|| {
            Command::CreatePullRequest(branch.clone())
        });
    }
}

trait Sink {
    fn add(&mut self, icon: Icon, label: &str, danger: bool, command: &dyn Fn() -> Command);
    fn separator(&mut self);

    fn item(&mut self, icon: Icon, label: &str, command: &dyn Fn() -> Command) {
        self.add(icon, label, false, command);
    }

    fn danger(&mut self, icon: Icon, label: &str, command: &dyn Fn() -> Command) {
        self.add(icon, label, true, command);
    }
}

struct UiSink<'a> {
    ui: &'a mut Ui,
    out: &'a mut Vec<Command>,
}

impl Sink for UiSink<'_> {
    fn add(&mut self, icon: Icon, label: &str, danger: bool, command: &dyn Fn() -> Command) {
        if row(self.ui, Some(icon), label, None, danger) {
            self.out.push(command());
        }
    }

    fn separator(&mut self) {
        separator(self.ui);
    }
}

impl Sink for Vec<Entry> {
    fn add(&mut self, icon: Icon, label: &str, danger: bool, command: &dyn Fn() -> Command) {
        self.push(Entry {
            icon,
            label: label.to_string(),
            danger,
            command: command(),
        });
    }

    fn separator(&mut self) {}
}

fn branch_items(sink: &mut impl Sink, label: &RefLabel, ctx: &MenuContext) {
    let name = label.name.clone();
    let current = ctx.current_branch.as_deref();
    match label.kind {
        RefKind::Local => {
            if current != Some(name.as_str()) {
                sink.item(Icon::Check, "Check out", &|| {
                    Command::Run(Op::Switch(name.clone()))
                });
            }
            sink.item(Icon::Worktree, "Open in new worktree…", &|| {
                Command::Open(Dialog::NewWorktree(NewWorktree::new(
                    ctx.repo_dir_name.to_string(),
                    name.clone(),
                    Some(name.clone()),
                    ctx.local_branches.clone(),
                )))
            });
            sink.item(Icon::Branch, "New branch from here…", &|| {
                new_branch(&name, &name)
            });
            sink.item(Icon::Pencil, "Rename…", &|| {
                Command::StartRename(name.clone())
            });
            sink.separator();
            if let Some(current) = current.filter(|c| *c != name) {
                sink.item(Icon::Merge, &format!("Merge into {current}"), &|| {
                    confirm(
                        format!("Merge {name} into {current}?"),
                        "",
                        Op::Merge(name.clone()),
                        false,
                    )
                });
                sink.item(
                    Icon::Rebase,
                    &format!("Rebase {current} onto this"),
                    &|| {
                        confirm(
                            format!("Rebase {current} onto {name}?"),
                            "Rewrites the commits on your current branch.",
                            Op::Rebase(name.clone()),
                            false,
                        )
                    },
                );
            }
            if current == Some(name.as_str()) {
                sink.item(Icon::Pull, "Pull", &|| Command::Run(Op::Pull));
            }
            sink.item(Icon::Push, "Push", &|| Command::Push(name.clone()));
            sink.item(Icon::Copy, "Copy branch name", &|| {
                Command::Copy(name.clone())
            });
            pull_items(sink, label, ctx);
            if current != Some(name.as_str()) {
                sink.separator();
                let remote = label.has_remote.then(|| "origin".to_string());
                sink.danger(Icon::Trash, "Delete branch…", &|| {
                    Command::Open(Dialog::DeleteBranch {
                        name: name.clone(),
                        force: false,
                        remote: remote.clone(),
                        delete_remote: false,
                    })
                });
            }
        }
        RefKind::Remote => {
            sink.item(Icon::Check, "Check out", &|| {
                Command::Run(Op::SwitchTrack(name.clone()))
            });
            sink.item(Icon::Branch, "New branch from here…", &|| {
                let local = name
                    .split_once('/')
                    .map_or(name.as_str(), |(_, b)| b)
                    .to_string();
                new_branch(&local, &name)
            });
            if let Some(current) = current {
                sink.item(Icon::Merge, &format!("Merge into {current}"), &|| {
                    confirm(
                        format!("Merge {name} into {current}?"),
                        "",
                        Op::Merge(name.clone()),
                        false,
                    )
                });
            }
            sink.item(Icon::Copy, "Copy branch name", &|| {
                Command::Copy(name.clone())
            });
            pull_items(sink, label, ctx);
            sink.separator();
            if let Some((remote, branch)) = name.split_once('/') {
                let (remote, branch) = (remote.to_string(), branch.to_string());
                sink.item(Icon::Pencil, "Rename on remote…", &|| {
                    Command::Open(TextDialog::open(
                        TextAction::RenameRemoteBranch {
                            remote: remote.clone(),
                            from: branch.clone(),
                            tracking: ctx.tracking(&name),
                        },
                        branch.clone(),
                    ))
                });
                sink.danger(Icon::Trash, "Delete remote branch…", &|| {
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
            sink.item(Icon::Check, "Check out", &|| {
                Command::Run(Op::SwitchDetached(name.clone()))
            });
            sink.item(Icon::Branch, "New branch from here…", &|| {
                new_branch("", &name)
            });
            sink.item(Icon::Copy, "Copy tag name", &|| Command::Copy(name.clone()));
            sink.separator();
            for remote in &ctx.remotes {
                let text = if ctx.remotes.len() == 1 {
                    "Push tag".to_string()
                } else {
                    format!("Push tag to {remote}")
                };
                sink.item(Icon::Push, &text, &|| {
                    Command::Run(Op::PushTag {
                        remote: remote.clone(),
                        name: name.clone(),
                    })
                });
            }
            sink.separator();
            sink.danger(Icon::Trash, "Delete tag…", &|| {
                confirm(
                    format!("Delete tag {name}?"),
                    "Only the local tag is removed. Undo brings it back.",
                    Op::DeleteTag(name.clone()),
                    true,
                )
            });
            if let Some(remote) = ctx.push_remote() {
                sink.danger(Icon::Trash, &format!("Delete tag on {remote}…"), &|| {
                    confirm(
                        format!("Delete tag {name} on {remote}?"),
                        "This removes the tag for everyone using this remote.",
                        Op::DeleteRemoteTag {
                            remote: remote.clone(),
                            name: name.clone(),
                        },
                        true,
                    )
                });
            }
        }
    }
}

pub fn commit(
    ui: &mut Ui,
    id: &str,
    parents: usize,
    title: &str,
    ctx: &MenuContext,
    can_rebase: bool,
    out: &mut Vec<Command>,
) {
    menu_width(ui, 250.0);
    let short = &id[..7.min(id.len())];
    heading(ui, short);
    item(ui, Icon::Check, "Check out (detached)", out, || {
        Command::Run(Op::SwitchDetached(id.to_string()))
    });
    item(ui, Icon::Branch, "New branch here…", out, || {
        new_branch("", id)
    });
    item(ui, Icon::Worktree, "New worktree here…", out, || {
        Command::Open(Dialog::NewWorktree(NewWorktree::new(
            ctx.repo_dir_name.to_string(),
            id.to_string(),
            None,
            ctx.local_branches.clone(),
        )))
    });
    item(ui, Icon::Tag, "Create tag here…", out, || {
        Command::Open(Dialog::NewTag {
            commit: id.to_string(),
            commit_label: short.to_string(),
            name: String::new(),
            annotated: false,
            message: String::new(),
        })
    });
    separator(ui);
    let merge = parents > 1;
    if ctx.head.as_deref() != Some(id) {
        item(ui, Icon::CherryPick, "Cherry-pick", out, || {
            Command::Run(Op::CherryPick {
                commit: id.to_string(),
                merge,
            })
        });
    }
    item(ui, Icon::Revert, "Revert commit", out, || {
        Command::Run(Op::Revert {
            commit: id.to_string(),
            merge,
        })
    });
    ui.add_enabled_ui(can_rebase, |ui| {
        item(
            ui,
            Icon::Rebase,
            "Interactive rebase from here…",
            out,
            || Command::OpenRebase(id.to_string()),
        );
    });
    item(ui, Icon::Pencil, "Edit message…", out, || {
        Command::EditMessage(id.to_string())
    });
    separator(ui);
    heading(
        ui,
        &format!(
            "Reset {} to here",
            ctx.current_branch.as_deref().unwrap_or("HEAD")
        ),
    );
    let reset = |mode| {
        Command::Run(Op::Reset {
            commit: id.to_string(),
            mode,
        })
    };
    item(ui, Icon::Reset, "Soft: keep changes staged", out, || {
        reset(ResetMode::Soft)
    });
    item(ui, Icon::Reset, "Mixed: keep changes unstaged", out, || {
        reset(ResetMode::Mixed)
    });
    danger(ui, Icon::Reset, "Hard: discard changes…", out, || {
        Command::ResetHard(id.to_string())
    });
    separator(ui);
    item(ui, Icon::Copy, "Copy commit hash", out, || {
        Command::Copy(id.to_string())
    });
    item(ui, Icon::Copy, "Copy message", out, || {
        Command::Copy(title.to_string())
    });
}

pub fn drop(ui: &mut Ui, plan: &DropPlan, ctx: &MenuContext, out: &mut Vec<Command>) {
    menu_width(ui, 270.0);
    heading(
        ui,
        &format!(
            "Drop {} on {}",
            plan.source.name,
            ref_labels::target_name(&plan.target)
        ),
    );
    let choices =
        ref_labels::drop_choices(&plan.source, &plan.target, ctx.current_branch.as_deref());
    if choices.is_empty() {
        ui.add_enabled_ui(false, |ui| row(ui, None, "Nothing to do here", None, false));
    }
    for choice in choices {
        match choice {
            DropChoice::Merge { source, into } => {
                item(
                    ui,
                    Icon::Merge,
                    &format!("Merge {source} into {into}"),
                    out,
                    || {
                        confirm(
                            format!("Merge {source} into {into}?"),
                            "",
                            Op::Merge(source.clone()),
                            false,
                        )
                    },
                );
            }
            DropChoice::CheckoutAndMerge { branch, source } => {
                item(
                    ui,
                    Icon::Merge,
                    &format!("Check out {branch} and merge {source}"),
                    out,
                    || {
                        confirm(
                            format!("Check out {branch} and merge {source} into it?"),
                            "",
                            Op::CheckoutAndMerge {
                                branch: branch.clone(),
                                source: source.clone(),
                            },
                            false,
                        )
                    },
                );
            }
            DropChoice::Rebase { current, onto } => {
                item(
                    ui,
                    Icon::Rebase,
                    &format!("Rebase {current} onto {onto}"),
                    out,
                    || {
                        confirm(
                            format!("Rebase {current} onto {onto}?"),
                            "Rewrites the commits on your current branch.",
                            Op::Rebase(onto.clone()),
                            false,
                        )
                    },
                );
            }
            DropChoice::Reset { current, commit } => {
                separator(ui);
                heading(ui, &format!("Reset {current} to here"));
                let reset = |mode| {
                    Command::Run(Op::Reset {
                        commit: commit.clone(),
                        mode,
                    })
                };
                item(ui, Icon::Reset, "Soft: keep changes staged", out, || {
                    reset(ResetMode::Soft)
                });
                item(ui, Icon::Reset, "Mixed: keep changes unstaged", out, || {
                    reset(ResetMode::Mixed)
                });
                danger(ui, Icon::Reset, "Hard: discard changes…", out, || {
                    Command::ResetHard(commit.clone())
                });
            }
        }
    }
}

pub fn file(ui: &mut Ui, path: &str, out: &mut Vec<Command>) {
    menu_width(ui, 210.0);
    file_items(ui, path, out);
}

pub fn file_items(ui: &mut Ui, path: &str, out: &mut Vec<Command>) {
    item(ui, Icon::Pencil, "Open in editor", out, || {
        Command::OpenInEditor(path.to_string())
    });
    item(ui, Icon::Folder, "Reveal in Finder", out, || {
        Command::RevealFile(path.to_string())
    });
    item(ui, Icon::Copy, "Copy path", out, || {
        Command::Copy(path.to_string())
    });
}

pub fn wip(ui: &mut Ui, out: &mut Vec<Command>) {
    menu_width(ui, 200.0);
    item(ui, Icon::Stash, "Stash all changes", out, || {
        Command::Run(Op::StashPush)
    });
}

pub fn stash(ui: &mut Ui, stash: &Stash, out: &mut Vec<Command>) {
    menu_width(ui, 200.0);
    heading(ui, &stash.name);
    item(ui, Icon::Eye, "Show changes", out, || {
        Command::ShowStash(stash.name.clone())
    });
    item(ui, Icon::Pencil, "Rename…", out, || {
        Command::Open(TextDialog::open(
            TextAction::RenameStash {
                stash: stash.name.clone(),
            },
            stash.message.clone(),
        ))
    });
    separator(ui);
    item(ui, Icon::Pop, "Apply", out, || {
        Command::Run(Op::StashApply(stash.name.clone()))
    });
    item(ui, Icon::Pop, "Pop", out, || Command::Run(Op::StashPop));
    separator(ui);
    danger(ui, Icon::Trash, "Drop…", out, || {
        confirm(
            format!("Drop {}?", stash.name),
            &stash.message,
            Op::StashDrop(stash.name.clone()),
            true,
        )
    });
}

pub fn worktree(ui: &mut Ui, tree: &Worktree, out: &mut Vec<Command>) {
    menu_width(ui, 220.0);
    heading(ui, &tree.name());
    item(ui, Icon::Folder, "Open in new tab", out, || {
        Command::OpenRepo(tree.path.clone())
    });
    item(ui, Icon::Terminal, "Open in terminal", out, || {
        Command::OpenTerminal(tree.path.clone())
    });
    item(ui, Icon::Copy, "Copy path", out, || {
        Command::Copy(tree.path.display().to_string())
    });
    if !tree.is_main {
        item(ui, Icon::Folder, "Move…", out, || {
            move_worktree(&tree.path)
        });
        separator(ui);
        danger(ui, Icon::Trash, "Remove…", out, || {
            remove_worktree(&tree.path)
        });
    }
}

pub fn move_worktree(path: &Path) -> Command {
    let from = path.display().to_string();
    Command::Open(TextDialog::open(
        TextAction::MoveWorktree { from: from.clone() },
        from,
    ))
}

pub fn remote_items(ui: &mut Ui, remote: &str, out: &mut Vec<Command>) {
    let name = remote.to_string();
    item(ui, Icon::Fetch, &format!("Fetch {remote}"), out, || {
        Command::Run(Op::FetchRemote(name.clone()))
    });
    item(ui, Icon::Fetch, "Prune stale branches", out, || {
        Command::Run(Op::PruneRemote(name.clone()))
    });
    item(ui, Icon::Pencil, "Rename remote…", out, || {
        Command::Open(TextDialog::open(
            TextAction::RenameRemote { from: name.clone() },
            name.clone(),
        ))
    });
    item(ui, Icon::Pencil, "Change URL…", out, || {
        Command::Open(TextDialog::open(
            TextAction::SetRemoteUrl { name: name.clone() },
            String::new(),
        ))
    });
    separator(ui);
    danger(ui, Icon::Trash, "Remove remote…", out, || {
        confirm(
            format!("Remove remote {name}?"),
            "Its remote branches disappear from Kelp. Branches on the server are not touched.",
            Op::RemoveRemote(name.clone()),
            true,
        )
    });
}

pub fn add_remote() -> Command {
    Command::Open(Dialog::AddRemote {
        name: String::new(),
        url: String::new(),
    })
}

pub fn push_all_tags(ctx: &MenuContext) -> Option<Command> {
    ctx.push_remote()
        .map(|remote| Command::Run(Op::PushTags(remote)))
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TabAction {
    Reveal,
    CopyPath,
    Close,
    CloseOthers,
}

pub fn tab(ui: &mut Ui, name: &str, has_others: bool) -> Option<TabAction> {
    menu_width(ui, 220.0);
    heading(ui, name);
    let mut action = None;
    if row(ui, Some(Icon::Folder), "Reveal in Finder", None, false) {
        action = Some(TabAction::Reveal);
    }
    if row(ui, Some(Icon::Copy), "Copy path", None, false) {
        action = Some(TabAction::CopyPath);
    }
    separator(ui);
    if row(ui, None, "Close", Some("⌘W"), false) {
        action = Some(TabAction::Close);
    }
    if has_others && row(ui, None, "Close other tabs", None, false) {
        action = Some(TabAction::CloseOthers);
    }
    action
}

pub fn view_items(ui: &mut Ui, label: &RefLabel, filtering: bool, out: &mut Vec<Command>) {
    separator(ui);
    let full = label.full_name();
    if !label.is_head {
        let hide = if label.hidden {
            "Show in graph"
        } else {
            "Hide from graph"
        };
        let icon = if label.hidden {
            Icon::Eye
        } else {
            Icon::EyeOff
        };
        item(ui, icon, hide, out, || Command::ToggleRef(full.clone()));
    }
    item(ui, Icon::Eye, "Show only this branch", out, || {
        Command::SoloRef(full.clone())
    });
    if filtering {
        item(ui, Icon::Eye, "Show all branches", out, || {
            Command::ShowAllRefs
        });
    }
}

#[cfg(test)]
mod tests {
    use kelp_core::pulls::{Pull, State};
    use kelp_core::refs::{RefKind, RefLabel};

    use super::{MenuContext, branch_entries};
    use crate::commands::Command;

    fn ctx(on_github: bool) -> MenuContext {
        MenuContext {
            current_branch: Some("main".into()),
            repo_dir_name: "repo".into(),
            local_branches: vec!["main".into(), "feat/a".into()],
            head: None,
            on_github,
            remotes: vec!["origin".into()],
            upstreams: vec![("feat/a".into(), "origin/feat/a".into())],
        }
    }

    fn branch(name: &str, kind: RefKind, has_remote: bool) -> RefLabel {
        RefLabel {
            name: name.into(),
            kind,
            target: gix::ObjectId::null(gix::hash::Kind::Sha1),
            row: Some(0),
            is_head: false,
            has_remote,
            hidden: false,
            pull: None,
        }
    }

    fn labels(label: &RefLabel, on_github: bool) -> Vec<String> {
        branch_entries(label, &ctx(on_github))
            .into_iter()
            .map(|e| e.label)
            .collect()
    }

    #[test]
    fn a_branch_with_a_pull_request_offers_to_open_it() {
        let mut label = branch("feat/a", RefKind::Local, true);
        label.pull = Some(Pull {
            number: 42,
            title: "A".into(),
            head: "feat/a".into(),
            head_owner: None,
            state: State::Open,
            url: "https://github.com/o/r/pull/42".into(),
            review: None,
            checks: None,
        });
        let entries = branch_entries(&label, &ctx(true));
        let open = entries
            .iter()
            .find(|e| e.label == "Open pull request #42")
            .expect("open entry");
        assert!(matches!(&open.command, Command::OpenUrl(url) if url.ends_with("/42")));
        assert!(!labels(&label, true).iter().any(|l| l.starts_with("Create")));
    }

    #[test]
    fn pushed_branches_on_github_offer_to_create_one() {
        let pushed = branch("feat/a", RefKind::Local, true);
        assert!(labels(&pushed, true).contains(&"Create pull request\u{2026}".to_string()));
        let remote = branch("origin/feat/a", RefKind::Remote, false);
        let entries = branch_entries(&remote, &ctx(true));
        let create = entries
            .iter()
            .find(|e| e.label.starts_with("Create pull request"))
            .expect("create entry");
        assert!(matches!(&create.command, Command::CreatePullRequest(b) if b == "feat/a"));
        let local_only = branch("feat/b", RefKind::Local, false);
        assert!(
            !labels(&local_only, true)
                .iter()
                .any(|l| l.contains("pull request"))
        );
        assert!(
            !labels(&pushed, false)
                .iter()
                .any(|l| l.contains("pull request"))
        );
    }
}
