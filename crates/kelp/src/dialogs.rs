use eframe::egui::{self, Key, Margin, RichText, Stroke, Ui};
use kelp_core::ops::Op;

use crate::commands::Command;
use crate::theme;

pub enum Dialog {
    NewBranch {
        name: String,
        start: String,
        start_label: String,
        switch: bool,
    },
    RenameBranch {
        from: String,
        to: String,
    },
    DeleteBranch {
        name: String,
        force: bool,
        remote: Option<String>,
        delete_remote: bool,
    },
    NewWorktree(NewWorktree),
    Confirm {
        title: String,
        body: String,
        op: Op,
        danger: bool,
    },
    PushTo {
        branch: String,
        remote: String,
        remotes: Vec<String>,
    },
    Text(TextDialog),
    NewTag {
        commit: String,
        commit_label: String,
        name: String,
        annotated: bool,
        message: String,
        push: bool,
        remote: String,
        remotes: Vec<String>,
    },
    AddRemote {
        name: String,
        url: String,
    },
}

pub enum TextAction {
    RenameRemoteBranch {
        remote: String,
        from: String,
        tracking: Option<String>,
    },
    RenameStash {
        stash: String,
    },
    MoveWorktree {
        from: String,
    },
    RenameRemote {
        from: String,
    },
    SetRemoteUrl {
        name: String,
    },
}

pub struct TextDialog {
    pub action: TextAction,
    pub value: String,
}

impl TextDialog {
    pub fn open(action: TextAction, value: impl Into<String>) -> Dialog {
        Dialog::Text(Self {
            action,
            value: value.into(),
        })
    }

    fn op(&self) -> Op {
        let value = self.value.trim().to_string();
        match &self.action {
            TextAction::RenameRemoteBranch {
                remote,
                from,
                tracking,
            } => Op::RenameRemoteBranch {
                remote: remote.clone(),
                from: from.clone(),
                to: value,
                tracking: tracking.clone(),
            },
            TextAction::RenameStash { stash } => Op::StashRename {
                stash: stash.clone(),
                message: value,
            },
            TextAction::MoveWorktree { from } => Op::WorktreeMove {
                from: from.clone(),
                to: value,
            },
            TextAction::RenameRemote { from } => Op::RenameRemote {
                from: from.clone(),
                to: value,
            },
            TextAction::SetRemoteUrl { name } => Op::SetRemoteUrl {
                name: name.clone(),
                url: value,
            },
        }
    }

    fn valid(&self) -> bool {
        let value = self.value.trim();
        match &self.action {
            TextAction::RenameRemoteBranch { from, .. } => {
                valid_branch_name(value) && value != from
            }
            TextAction::RenameStash { .. } | TextAction::SetRemoteUrl { .. } => !value.is_empty(),
            TextAction::MoveWorktree { from } => !value.is_empty() && value != from,
            TextAction::RenameRemote { from } => valid_remote_name(value) && value != from,
        }
    }

    fn texts(&self) -> (String, &'static str, &'static str, Option<String>) {
        match &self.action {
            TextAction::RenameRemoteBranch { remote, from, .. } => (
                format!("Rename {remote}/{from}"),
                "New name on the remote",
                "Rename",
                Some(format!(
                    "Pushes the new name, then deletes {from} on {remote}. Anyone else using it \
                     needs to switch to the new name."
                )),
            ),
            TextAction::RenameStash { stash } => {
                (format!("Rename {stash}"), "Message", "Rename", None)
            }
            TextAction::MoveWorktree { .. } => (
                "Move worktree".into(),
                "New folder",
                "Move",
                Some("The worktree must have no untracked changes Kelp would lose.".into()),
            ),
            TextAction::RenameRemote { from } => (
                format!("Rename remote {from}"),
                "New name",
                "Rename",
                Some("Remote branches and upstreams follow the new name.".into()),
            ),
            TextAction::SetRemoteUrl { name } => {
                (format!("Change the URL of {name}"), "URL", "Save", None)
            }
        }
    }
}

pub fn reset_hard_dialog(branch: &str, commit: &str, dropped: usize, dirty: usize) -> Dialog {
    let short = &commit[..commit.len().min(7)];
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut lost = Vec::new();
    if dropped > 0 {
        lost.push(format!(
            "{} will no longer be on {branch}",
            plural(dropped, "commit", "commits")
        ));
    }
    if dirty > 0 {
        lost.push(format!(
            "uncommitted changes in {} are discarded",
            plural(dirty, "file", "files")
        ));
    }
    let consequence = match lost.as_slice() {
        [] => "Nothing is lost".to_string(),
        [one] => capitalize(one),
        [first, second] => format!("{} and {second}", capitalize(first)),
        _ => unreachable!(),
    };
    Dialog::Confirm {
        title: "Reset hard?".into(),
        body: format!("{branch} moves to {short}. {consequence}. Untracked files stay."),
        op: Op::Reset {
            commit: commit.to_string(),
            mode: kelp_core::ops::ResetMode::Hard,
        },
        danger: true,
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

pub fn force_push_dialog(op: Op) -> Dialog {
    Dialog::Confirm {
        title: "Force push (with lease)?".into(),
        body: "The remote has commits your branch does not. Forcing replaces them with yours, \
               and stops if someone pushed after your last fetch."
            .into(),
        op,
        danger: true,
    }
}

pub fn recovery(op: &Op, error: &str) -> Option<Dialog> {
    match op {
        Op::Push {
            branch,
            remote,
            set_upstream,
            force_with_lease: false,
        } if kelp_core::ops::push_rejected(error) => Some(force_push_dialog(Op::Push {
            branch: branch.clone(),
            remote: remote.clone(),
            set_upstream: *set_upstream,
            force_with_lease: true,
        })),
        Op::WorktreeRemove { path, force: false }
            if error.contains("contains modified or untracked files") =>
        {
            Some(Dialog::Confirm {
                title: "Delete the worktree and its changes?".into(),
                body: format!(
                    "{path} has uncommitted or untracked files. Removing the worktree \
                     deletes them for good."
                ),
                op: Op::WorktreeRemove {
                    path: path.clone(),
                    force: true,
                },
                danger: true,
            })
        }
        Op::DeleteBranch { name, force: false } if error.contains("not fully merged") => {
            Some(Dialog::Confirm {
                title: "Delete unmerged branch?".into(),
                body: format!(
                    "{name} has commits that are not merged into your current branch. \
                     Deleting it drops them, and Undo (Cmd+Z) brings the branch back."
                ),
                op: Op::DeleteBranch {
                    name: name.clone(),
                    force: true,
                },
                danger: true,
            })
        }
        _ => None,
    }
}

pub struct NewWorktree {
    pub create_branch: bool,
    pub branch: String,
    pub existing: String,
    pub branches: Vec<String>,
    pub start: String,
    pub folder: String,
    pub repo_dir_name: String,
    pub folder_edited: bool,
    pub open_after: bool,
}

impl NewWorktree {
    pub fn new(
        repo_dir_name: String,
        start: String,
        existing: Option<String>,
        branches: Vec<String>,
    ) -> Self {
        let create_branch = existing.is_none();
        let mut dialog = Self {
            create_branch,
            branch: String::new(),
            existing: existing
                .or_else(|| branches.first().cloned())
                .unwrap_or_default(),
            branches,
            start,
            folder: String::new(),
            repo_dir_name,
            folder_edited: false,
            open_after: true,
        };
        dialog.suggest_folder();
        dialog
    }

    fn branch_name(&self) -> &str {
        if self.create_branch {
            &self.branch
        } else {
            &self.existing
        }
    }

    fn suggest_folder(&mut self) {
        if self.folder_edited {
            return;
        }
        let slug: String = self
            .branch_name()
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        self.folder = if slug.is_empty() {
            format!("../{}-worktree", self.repo_dir_name)
        } else {
            format!("../{}-{slug}", self.repo_dir_name)
        };
    }

    pub fn op(&self) -> Op {
        if self.create_branch {
            Op::WorktreeAdd {
                path: self.folder.clone(),
                new_branch: Some(self.branch.clone()),
                start: self.start.clone(),
            }
        } else {
            Op::WorktreeAdd {
                path: self.folder.clone(),
                new_branch: None,
                start: self.existing.clone(),
            }
        }
    }
}

pub enum Outcome {
    Keep,
    Close,
    Confirm(Vec<Command>),
}

pub fn show(ctx: &egui::Context, dialog: &mut Dialog) -> Outcome {
    let mut outcome = Outcome::Keep;
    let modal = egui::Modal::new(egui::Id::new("kelp-dialog"))
        .backdrop_color(theme::backdrop())
        .frame(
            egui::Frame::new()
                .fill(theme::modal())
                .stroke(Stroke::new(1.0, theme::modal_border()))
                .corner_radius(10)
                .inner_margin(Margin::same(22)),
        )
        .show(ctx, |ui| {
            ui.set_width(460.0);
            ui.spacing_mut().item_spacing.y = 12.0;
            outcome = match dialog {
                Dialog::NewBranch {
                    name,
                    start,
                    start_label,
                    switch,
                } => new_branch(ui, name, start, start_label, switch),
                Dialog::RenameBranch { from, to } => rename(ui, from, to),
                Dialog::DeleteBranch {
                    name,
                    force,
                    remote,
                    delete_remote,
                } => delete(ui, name, force, remote, delete_remote),
                Dialog::NewWorktree(state) => new_worktree(ui, state),
                Dialog::Confirm {
                    title,
                    body,
                    op,
                    danger,
                } => confirm(ui, title, body, op, *danger),
                Dialog::PushTo {
                    branch,
                    remote,
                    remotes,
                } => push_to(ui, branch, remote, remotes),
                Dialog::Text(state) => text_dialog(ui, state),
                Dialog::NewTag {
                    commit,
                    commit_label,
                    name,
                    annotated,
                    message,
                    push,
                    remote,
                    remotes,
                } => new_tag(
                    ui,
                    commit,
                    commit_label,
                    name,
                    annotated,
                    message,
                    (push, remote, remotes),
                ),
                Dialog::AddRemote { name, url } => add_remote(ui, name, url),
            };
        });
    if modal.should_close() {
        return Outcome::Close;
    }
    outcome
}

fn title(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(17.0)
            .family(theme::semibold())
            .color(theme::text_strong()),
    );
}

fn field(ui: &mut Ui, label: &str, value: &mut String, focus: bool) -> egui::Response {
    ui.label(RichText::new(label).size(12.0).color(theme::text_body()));
    let response = ui.add(
        egui::TextEdit::singleline(value)
            .font(egui::FontId::monospace(13.0))
            .desired_width(f32::INFINITY)
            .margin(Margin::symmetric(10, 8)),
    );
    if focus && !response.has_focus() && ui.memory(|m| m.focused().is_none()) {
        response.request_focus();
    }
    response
}

fn preview(ui: &mut Ui, op: &Op) {
    ui.label(RichText::new("Runs").size(11.0).color(theme::text_faint()));
    egui::Frame::new()
        .fill(theme::field_deep())
        .stroke(Stroke::new(1.0, theme::border()))
        .corner_radius(6)
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(op.command_line())
                    .monospace()
                    .size(12.0)
                    .color(theme::text_body()),
            );
        });
}

fn enter_confirms(ctx: &egui::Context, danger: bool) -> bool {
    let in_text_or_nowhere =
        crate::list_keys::nothing_focused(ctx) || crate::list_keys::is_typing(ctx);
    !danger && in_text_or_nowhere && ctx.input(|i| i.key_pressed(Key::Enter))
}

fn buttons(ui: &mut Ui, confirm_label: &str, enabled: bool, danger: bool) -> Outcome {
    let enter = enter_confirms(ui.ctx(), danger);
    let mut outcome = Outcome::Keep;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let fill = if danger {
                theme::danger()
            } else {
                theme::accent()
            };
            let ink = if danger {
                theme::text_strong()
            } else {
                theme::on_accent()
            };
            let confirm = egui::Button::new(
                RichText::new(confirm_label)
                    .family(theme::semibold())
                    .color(ink),
            )
            .fill(fill)
            .corner_radius(6)
            .min_size(egui::vec2(0.0, 34.0));
            if ui.add_enabled(enabled, confirm).clicked() || (enabled && enter) {
                outcome = Outcome::Confirm(Vec::new());
            }
            let cancel = egui::Button::new("Cancel")
                .corner_radius(6)
                .min_size(egui::vec2(0.0, 34.0));
            if ui.add(cancel).clicked() {
                outcome = Outcome::Close;
            }
        });
    });
    outcome
}

fn with(outcome: Outcome, commands: impl FnOnce() -> Vec<Command>) -> Outcome {
    match outcome {
        Outcome::Confirm(_) => Outcome::Confirm(commands()),
        other => other,
    }
}

fn valid_branch_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && !name.contains(char::is_whitespace)
        && !name.contains("..")
        && !name.starts_with('-')
        && !name.ends_with('/')
        && !name.ends_with(".lock")
        && !name
            .chars()
            .any(|c| matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\'))
}

fn valid_remote_name(name: &str) -> bool {
    valid_branch_name(name) && !name.contains('/')
}

fn text_dialog(ui: &mut Ui, state: &mut TextDialog) -> Outcome {
    let (heading, label, confirm_label, note) = state.texts();
    title(ui, &heading);
    field(ui, label, &mut state.value, true);
    if let TextAction::MoveWorktree { from } = &state.action
        && ui.button("Choose folder…").clicked()
        && let Some(parent) = rfd::FileDialog::new()
            .set_title("Move the worktree into")
            .pick_folder()
    {
        let name = std::path::Path::new(from)
            .file_name()
            .map(|n| n.to_owned())
            .unwrap_or_default();
        state.value = parent.join(name).to_string_lossy().into_owned();
    }
    if let Some(note) = note {
        ui.label(RichText::new(note).size(12.0).color(theme::text_muted()));
    }
    let op = state.op();
    preview(ui, &op);
    with(buttons(ui, confirm_label, state.valid(), false), || {
        vec![Command::Run(op)]
    })
}

fn new_tag(
    ui: &mut Ui,
    commit: &str,
    commit_label: &str,
    name: &mut String,
    annotated: &mut bool,
    message: &mut String,
    (push, remote, remotes): (&mut bool, &mut String, &[String]),
) -> Outcome {
    title(ui, "New tag");
    field(ui, "Tag name", name, true);
    ui.label(
        RichText::new(format!("On {commit_label}"))
            .size(12.0)
            .color(theme::text_muted()),
    );
    ui.checkbox(annotated, "Annotated, with a message");
    if *annotated {
        field(ui, "Message", message, false);
    }
    if !remotes.is_empty() {
        ui.horizontal(|ui| {
            ui.checkbox(push, "Push tag to");
            ui.add_enabled_ui(*push, |ui| {
                egui::ComboBox::from_id_salt("new-tag-remote")
                    .selected_text(remote.clone())
                    .show_ui(ui, |ui| {
                        for option in remotes {
                            ui.selectable_value(remote, option.clone(), option);
                        }
                    });
            });
        });
    }
    let op = tag_op(commit, name, message, *annotated, (*push, remote, remotes));
    preview(ui, &op);
    let enabled = valid_branch_name(name) && (!*annotated || !message.trim().is_empty());
    with(buttons(ui, "Create tag", enabled, false), || {
        vec![Command::Run(op)]
    })
}

fn tag_op(
    commit: &str,
    name: &str,
    message: &str,
    annotated: bool,
    (push, remote, remotes): (bool, &str, &[String]),
) -> Op {
    Op::CreateTag {
        name: name.trim().to_string(),
        commit: commit.to_string(),
        message: annotated.then(|| message.trim().to_string()),
        push_to: (push && !remotes.is_empty()).then(|| remote.to_string()),
    }
}

fn add_remote(ui: &mut Ui, name: &mut String, url: &mut String) -> Outcome {
    title(ui, "Add remote");
    field(ui, "Name", name, true);
    field(ui, "URL", url, false);
    let op = Op::AddRemote {
        name: name.trim().to_string(),
        url: url.trim().to_string(),
    };
    preview(ui, &op);
    let enabled = valid_remote_name(name) && !url.trim().is_empty();
    with(buttons(ui, "Add remote", enabled, false), || {
        vec![
            Command::Run(op),
            Command::Run(Op::FetchRemote(name.trim().to_string())),
        ]
    })
}

fn new_branch(
    ui: &mut Ui,
    name: &mut String,
    start: &str,
    start_label: &str,
    switch: &mut bool,
) -> Outcome {
    title(ui, "New branch");
    field(ui, "Branch name", name, true);
    ui.label(
        RichText::new(format!("Starts from {start_label}"))
            .size(12.0)
            .color(theme::text_muted()),
    );
    ui.checkbox(switch, "Check it out");
    let op = Op::CreateBranch {
        name: name.trim().to_string(),
        start: start.to_string(),
        switch: *switch,
    };
    preview(ui, &op);
    with(
        buttons(ui, "Create branch", valid_branch_name(name), false),
        || vec![Command::Run(op)],
    )
}

fn rename(ui: &mut Ui, from: &str, to: &mut String) -> Outcome {
    title(ui, &format!("Rename {from}"));
    field(ui, "New name", to, true);
    let op = Op::RenameBranch {
        from: from.to_string(),
        to: to.trim().to_string(),
    };
    preview(ui, &op);
    let enabled = valid_branch_name(to) && to.trim() != from;
    with(buttons(ui, "Rename", enabled, false), || {
        vec![Command::Run(op)]
    })
}

fn delete(
    ui: &mut Ui,
    name: &str,
    force: &mut bool,
    remote: &Option<String>,
    delete_remote: &mut bool,
) -> Outcome {
    title(ui, &format!("Delete {name}?"));
    ui.label(RichText::new("The branch is removed from this repository. Commits that are only on it can be lost if you force it.").color(theme::text_muted()));
    ui.checkbox(force, "Force, even if it is not merged");
    if let Some(remote) = remote {
        ui.checkbox(delete_remote, format!("Also delete {remote}/{name}"));
    }
    let op = Op::DeleteBranch {
        name: name.to_string(),
        force: *force,
    };
    preview(ui, &op);
    let remote_op = remote
        .as_ref()
        .filter(|_| *delete_remote)
        .map(|r| Op::DeleteRemoteBranch {
            remote: r.clone(),
            branch: name.to_string(),
        });
    if let Some(remote_op) = &remote_op {
        preview(ui, remote_op);
    }
    with(buttons(ui, "Delete branch", true, true), || {
        std::iter::once(Command::Run(op))
            .chain(remote_op.map(Command::Run))
            .collect()
    })
}

fn new_worktree(ui: &mut Ui, state: &mut NewWorktree) -> Outcome {
    title(ui, "New worktree");
    ui.horizontal(|ui| {
        let before = state.create_branch;
        ui.selectable_value(&mut state.create_branch, true, "Create new branch");
        ui.selectable_value(&mut state.create_branch, false, "Use existing branch");
        if before != state.create_branch {
            state.suggest_folder();
        }
    });
    if state.create_branch {
        if field(ui, "Branch name", &mut state.branch, true).changed() {
            state.suggest_folder();
        }
        ui.label(
            RichText::new(format!("Starts from {}", state.start))
                .size(12.0)
                .color(theme::text_muted()),
        );
    } else {
        ui.label(RichText::new("Branch").size(12.0).color(theme::text_body()));
        let before = state.existing.clone();
        egui::ComboBox::from_id_salt("worktree-branch")
            .width(ui.available_width())
            .selected_text(state.existing.clone())
            .show_ui(ui, |ui| {
                for branch in state.branches.clone() {
                    ui.selectable_value(&mut state.existing, branch.clone(), branch);
                }
            });
        if before != state.existing {
            state.suggest_folder();
        }
    }
    let folder_before = state.folder.clone();
    field(
        ui,
        "Folder (relative to this repo)",
        &mut state.folder,
        false,
    );
    if state.folder != folder_before {
        state.folder_edited = true;
    }
    ui.checkbox(&mut state.open_after, "Switch to it when done");
    let op = state.op();
    preview(ui, &op);
    let enabled = !state.folder.trim().is_empty()
        && (!state.create_branch || valid_branch_name(&state.branch))
        && (state.create_branch || !state.existing.is_empty());
    let open_after = state.open_after;
    let folder = state.folder.clone();
    with(buttons(ui, "Create worktree", enabled, false), move || {
        let mut commands = vec![Command::Run(op)];
        if open_after {
            commands.push(Command::OpenWorktree(folder.into()));
        }
        commands
    })
}

fn confirm(ui: &mut Ui, title_text: &str, body: &str, op: &Op, danger: bool) -> Outcome {
    title(ui, title_text);
    if !body.is_empty() {
        ui.label(RichText::new(body).color(theme::text_muted()));
    }
    preview(ui, op);
    let op = op.clone();
    with(
        buttons(
            ui,
            title_text.split('?').next().unwrap_or("OK"),
            true,
            danger,
        ),
        || vec![Command::Run(op)],
    )
}

fn push_to(ui: &mut Ui, branch: &str, remote: &mut String, remotes: &[String]) -> Outcome {
    title(ui, &format!("Push {branch}"));
    ui.label(
        RichText::new("This branch has no upstream yet. Pick the remote to push to and track.")
            .color(theme::text_muted()),
    );
    ui.label(RichText::new("Remote").size(12.0).color(theme::text_body()));
    egui::ComboBox::from_id_salt("push-remote")
        .width(ui.available_width())
        .selected_text(remote.clone())
        .show_ui(ui, |ui| {
            for name in remotes {
                ui.selectable_value(remote, name.clone(), name);
            }
        });
    let op = Op::Push {
        branch: branch.to_string(),
        remote: remote.clone(),
        set_upstream: true,
        force_with_lease: false,
    };
    preview(ui, &op);
    with(buttons(ui, "Push", true, false), || vec![Command::Run(op)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show_with(ctx: &egui::Context, dialog: &mut Dialog, events: Vec<egui::Event>) -> Outcome {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            )),
            events,
            ..Default::default()
        };
        let mut outcome = Outcome::Keep;
        let _ = ctx.run_ui(input, |ui| outcome = show(ui.ctx(), dialog));
        outcome
    }

    fn enter() -> Vec<egui::Event> {
        [true, false]
            .map(|pressed| egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .to_vec()
    }

    fn confirm_dialog(danger: bool) -> Dialog {
        Dialog::Confirm {
            title: "Drop stash?".into(),
            body: String::new(),
            op: Op::StashDrop("stash@{0}".into()),
            danger,
        }
    }

    #[test]
    fn a_new_tag_pushes_only_when_asked_and_a_remote_exists() {
        let remotes = ["origin".to_string()];
        let op =
            |push, remotes: &[String]| tag_op("HEAD", "v1", "", false, (push, "origin", remotes));
        let pushed = op(true, &remotes);
        assert_eq!(
            pushed.command_line(),
            "git tag v1 HEAD && git push origin refs/tags/v1"
        );
        assert_eq!(op(false, &remotes).command_line(), "git tag v1 HEAD");
        assert_eq!(op(true, &[]).command_line(), "git tag v1 HEAD");
    }

    #[test]
    fn a_blocked_worktree_removal_offers_to_delete_the_changes() {
        let op = Op::WorktreeRemove {
            path: "/tmp/wt".into(),
            force: false,
        };
        let error = "git worktree remove /tmp/wt failed: fatal: '/tmp/wt' contains modified or untracked files, use --force to delete it";
        match recovery(&op, error) {
            Some(Dialog::Confirm { op, danger, .. }) => {
                assert!(danger);
                assert_eq!(
                    op,
                    Op::WorktreeRemove {
                        path: "/tmp/wt".into(),
                        force: true
                    }
                );
            }
            _ => panic!("expected a confirm"),
        }
    }

    #[test]
    fn an_unmerged_branch_offers_a_force_delete_and_other_errors_offer_nothing() {
        let op = Op::DeleteBranch {
            name: "feat/x".into(),
            force: false,
        };
        let error = "git branch -d feat/x failed: error: the branch 'feat/x' is not fully merged";
        assert!(matches!(
            recovery(&op, error),
            Some(Dialog::Confirm {
                op: Op::DeleteBranch { force: true, .. },
                ..
            })
        ));
        assert!(recovery(&op, "fatal: something else").is_none());
        assert!(recovery(&Op::Fetch, "fatal: not fully merged").is_none());
    }

    #[test]
    fn enter_confirms_a_safe_dialog_but_not_a_dangerous_one() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut safe = confirm_dialog(false);
        show_with(&ctx, &mut safe, vec![]);
        assert!(matches!(
            show_with(&ctx, &mut safe, enter()),
            Outcome::Confirm(_)
        ));

        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut danger = confirm_dialog(true);
        show_with(&ctx, &mut danger, vec![]);
        assert!(matches!(
            show_with(&ctx, &mut danger, enter()),
            Outcome::Keep
        ));
    }

    #[test]
    fn enter_types_into_the_name_field_and_confirms_from_there() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut dialog = Dialog::NewBranch {
            name: String::new(),
            start: "main".into(),
            start_label: "main".into(),
            switch: true,
        };
        show_with(&ctx, &mut dialog, vec![]);
        show_with(&ctx, &mut dialog, vec![egui::Event::Text("feat/x".into())]);
        let Dialog::NewBranch { name, .. } = &dialog else {
            unreachable!()
        };
        assert_eq!(name, "feat/x", "the name field has focus when it opens");
        assert!(matches!(
            show_with(&ctx, &mut dialog, enter()),
            Outcome::Confirm(_)
        ));
    }

    #[test]
    fn enter_on_a_focused_checkbox_does_not_confirm() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut dialog = Dialog::NewBranch {
            name: "feat/x".into(),
            start: "main".into(),
            start_label: "main".into(),
            switch: true,
        };
        show_with(&ctx, &mut dialog, vec![]);
        let tab = |pressed| egui::Event::Key {
            key: Key::Tab,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        show_with(&ctx, &mut dialog, vec![tab(true), tab(false)]);
        show_with(&ctx, &mut dialog, vec![]);
        assert!(
            !crate::list_keys::is_typing(&ctx),
            "focus left the name field"
        );
        assert!(!crate::list_keys::nothing_focused(&ctx));
        assert!(matches!(
            show_with(&ctx, &mut dialog, enter()),
            Outcome::Keep
        ));
    }

    fn body(dialog: Dialog) -> String {
        match dialog {
            Dialog::Confirm { body, .. } => body,
            _ => unreachable!(),
        }
    }

    #[test]
    fn hard_reset_says_what_is_lost() {
        assert_eq!(
            body(reset_hard_dialog("main", "abc1234def", 2, 1)),
            "main moves to abc1234. 2 commits will no longer be on main and uncommitted \
             changes in 1 file are discarded. Untracked files stay."
        );
        assert_eq!(
            body(reset_hard_dialog("main", "abc1234def", 0, 3)),
            "main moves to abc1234. Uncommitted changes in 3 files are discarded. \
             Untracked files stay."
        );
        assert_eq!(
            body(reset_hard_dialog("HEAD", "abc1234def", 0, 0)),
            "HEAD moves to abc1234. Nothing is lost. Untracked files stay."
        );
    }

    #[test]
    fn branch_names_follow_git_rules() {
        assert!(valid_branch_name("feat/stash-view"));
        assert!(!valid_branch_name("has space"));
        assert!(!valid_branch_name("a..b"));
        assert!(!valid_branch_name("-x"));
        assert!(!valid_branch_name("x.lock"));
        assert!(!valid_branch_name(""));
    }

    #[test]
    fn worktree_folder_follows_branch_name() {
        let mut d = NewWorktree::new("kelp".into(), "main".into(), None, vec![]);
        d.branch = "feat/stash-view".into();
        d.suggest_folder();
        assert_eq!(d.folder, "../kelp-stash-view");
        assert_eq!(
            d.op().command_line(),
            "git worktree add -b feat/stash-view ../kelp-stash-view main"
        );
    }
}
