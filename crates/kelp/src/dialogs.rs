use eframe::egui::{self, Color32, Key, Margin, RichText, Stroke, Ui};
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
        .frame(
            egui::Frame::new()
                .fill(Color32::from_rgb(0x1f, 0x24, 0x2d))
                .stroke(Stroke::new(1.0, Color32::from_rgb(0x3a, 0x42, 0x50)))
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
            .color(theme::TEXT_STRONG),
    );
}

fn field(ui: &mut Ui, label: &str, value: &mut String, focus: bool) -> egui::Response {
    ui.label(
        RichText::new(label)
            .size(12.0)
            .color(egui::Color32::from_rgb(0xb4, 0xb9, 0xc2)),
    );
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
    ui.label(RichText::new("Runs").size(11.0).color(theme::TEXT_FAINT));
    egui::Frame::new()
        .fill(Color32::from_rgb(0x11, 0x13, 0x18))
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(6)
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(op.command_line())
                    .monospace()
                    .size(12.0)
                    .color(egui::Color32::from_rgb(0xb4, 0xb9, 0xc2)),
            );
        });
}

fn buttons(ui: &mut Ui, confirm_label: &str, enabled: bool, danger: bool) -> Outcome {
    let enter = ui.input(|i| i.key_pressed(Key::Enter));
    let mut outcome = Outcome::Keep;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let fill = if danger {
                Color32::from_rgb(0xc2, 0x4a, 0x40)
            } else {
                theme::ACCENT
            };
            let ink = if danger {
                theme::TEXT_STRONG
            } else {
                Color32::from_rgb(0x10, 0x13, 0x1a)
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
            .color(theme::TEXT_MUTED),
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
    ui.label(RichText::new("The branch is removed from this repository. Commits that are only on it can be lost if you force it.").color(theme::TEXT_MUTED));
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
                .color(theme::TEXT_MUTED),
        );
    } else {
        ui.label(
            RichText::new("Branch")
                .size(12.0)
                .color(egui::Color32::from_rgb(0xb4, 0xb9, 0xc2)),
        );
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
    ui.checkbox(&mut state.open_after, "Open in a new tab when done");
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
            commands.push(Command::OpenRepo(folder.into()));
        }
        commands
    })
}

fn confirm(ui: &mut Ui, title_text: &str, body: &str, op: &Op, danger: bool) -> Outcome {
    title(ui, title_text);
    if !body.is_empty() {
        ui.label(RichText::new(body).color(theme::TEXT_MUTED));
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
            .color(theme::TEXT_MUTED),
    );
    ui.label(
        RichText::new("Remote")
            .size(12.0)
            .color(egui::Color32::from_rgb(0xb4, 0xb9, 0xc2)),
    );
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
