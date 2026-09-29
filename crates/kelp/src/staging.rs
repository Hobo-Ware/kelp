use std::collections::BTreeSet;

use eframe::egui::{
    self, Align2, FontId, Key, Margin, Modifiers, RichText, Sense, Stroke, Ui, pos2, vec2,
};
use kelp_core::commit::{ChangeKind, FileChange};
use kelp_core::ops::Op;

use crate::commands::Command;
use crate::commit_helpers;
use crate::dialogs::Dialog;
use crate::icons::Icon;
use crate::menus;
use crate::repo_view::{Center, Repo};
use crate::theme;

const ROW_H: f32 = 28.0;
const SUMMARY_LIMIT: usize = 72;

enum Action {
    Open(String, bool),
    OpenConflict(String),
    Run(Op),
    Confirm(Dialog),
    Command(Command),
    Stash(StashPrompt),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Section {
    Unstaged,
    Staged,
}

#[derive(Clone, Default, Debug)]
struct Picked {
    section: Option<Section>,
    paths: BTreeSet<String>,
    anchor: Option<usize>,
}

impl Picked {
    fn click(
        &mut self,
        section: Section,
        index: usize,
        list: &[&str],
        modifiers: Modifiers,
    ) -> bool {
        let path = list[index].to_string();
        let same_section = self.section == Some(section);
        if modifiers.shift
            && same_section
            && let Some(anchor) = self.anchor
        {
            let (from, to) = (anchor.min(index), anchor.max(index));
            self.paths
                .extend(list[from..=to].iter().map(|p| p.to_string()));
            return false;
        }
        if modifiers.command && same_section {
            if !self.paths.remove(&path) {
                self.paths.insert(path);
            }
            self.anchor = Some(index);
            return false;
        }
        self.section = Some(section);
        self.paths = BTreeSet::from([path]);
        self.anchor = Some(index);
        !(modifiers.command || modifiers.shift)
    }

    fn in_section(&self, section: Section) -> Vec<String> {
        if self.section == Some(section) {
            self.paths.iter().cloned().collect()
        } else {
            Vec::new()
        }
    }

    fn keep_only(&mut self, section: Section, list: &[&str]) {
        if self.section == Some(section) {
            self.paths.retain(|p| list.contains(&p.as_str()));
        }
    }

    fn is_picked(&self, section: Section, path: &str) -> bool {
        self.section == Some(section) && self.paths.contains(path)
    }
}

#[derive(Clone, Debug)]
struct StashPrompt {
    paths: Vec<String>,
    staged_only: bool,
    message: String,
}

impl StashPrompt {
    fn op(&self) -> Op {
        if self.staged_only {
            Op::StashStaged {
                message: self.message.clone(),
            }
        } else {
            Op::StashFiles {
                paths: self.paths.clone(),
                message: self.message.clone(),
            }
        }
    }

    fn title(&self) -> String {
        match (self.staged_only, self.paths.len()) {
            (true, _) => "Stash staged changes".into(),
            (false, 1) => format!("Stash {}", self.paths[0]),
            (false, n) => format!("Stash {n} files"),
        }
    }
}

fn picked_from_env() -> Picked {
    let Ok(list) = std::env::var("KELP_PICK") else {
        return Picked::default();
    };
    Picked {
        section: Some(Section::Unstaged),
        paths: list.split(',').map(str::to_string).collect(),
        anchor: None,
    }
}

fn plural(n: usize) -> String {
    format!("{n} file{}", if n == 1 { "" } else { "s" })
}

pub fn ui(ui: &mut Ui, repo: &mut Repo) {
    let mut actions = Vec::new();
    let picked_id = egui::Id::new(("staging-picked", &repo.dir));
    let prompt_id = egui::Id::new(("staging-stash", &repo.dir));
    let mut picked: Picked = ui.data(|d| d.get_temp(picked_id)).unwrap_or_default();
    if picked.paths.is_empty() && !repo.status.unstaged.is_empty() {
        picked = picked_from_env();
    }
    let paths_of = |changes: &[FileChange]| -> Vec<String> {
        changes.iter().map(|c| c.path.clone()).collect()
    };
    let (unstaged_owned, staged_owned) = (
        paths_of(&repo.status.unstaged),
        paths_of(&repo.status.staged),
    );
    let unstaged_paths: Vec<&str> = unstaged_owned.iter().map(String::as_str).collect();
    let staged_paths: Vec<&str> = staged_owned.iter().map(String::as_str).collect();
    picked.keep_only(Section::Unstaged, &unstaged_paths);
    picked.keep_only(Section::Staged, &staged_paths);
    let modifiers = ui.input(|i| i.modifiers);
    egui::Panel::bottom("commit-box")
        .frame(egui::Frame::NONE)
        .show(ui, |ui| commit_box(ui, repo));
    let list_h = ui.available_height().max(120.0);
    ui.allocate_ui(vec2(ui.available_width(), list_h), |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                header(ui, repo);
                ui.spacing_mut().item_spacing.y = 0.0;
                let active = match &repo.center {
                    Center::Diff(view) if view.is_working() => {
                        Some((view.path().to_string(), view.is_staged()))
                    }
                    _ => None,
                };
                if !repo.status.conflicted.is_empty() {
                    section(
                        ui,
                        "Conflicts",
                        repo.status.conflicted.len(),
                        None,
                        None,
                        &mut actions,
                    );
                    let open_conflict = match &repo.center {
                        Center::Conflict(view) => Some(view.path.clone()),
                        _ => None,
                    };
                    for path in &repo.status.conflicted {
                        let change = FileChange {
                            path: path.clone(),
                            kind: ChangeKind::Modified,
                        };
                        let is_open = open_conflict.as_deref() == Some(path.as_str());
                        let (row, _) = file_row(
                            ui,
                            egui::Id::new(("staging-conflict", path)),
                            &change,
                            is_open,
                            false,
                            None,
                            Some("resolve"),
                        );
                        if row.on_hover_text("Pick a side for each conflict").clicked() {
                            actions.push(Action::OpenConflict(path.clone()));
                        }
                    }
                    ui.add_space(10.0);
                }
                let has_head = repo.has_head();
                section(
                    ui,
                    "Unstaged",
                    repo.status.unstaged.len(),
                    (!repo.status.unstaged.is_empty()).then_some(("Stage all", Op::StageAll)),
                    None,
                    &mut actions,
                );
                let mut nav_rows = Vec::new();
                let mut nav_items: Vec<(Section, usize, String)> = Vec::new();
                let chosen = picked.in_section(Section::Unstaged);
                for (index, change) in repo.status.unstaged.iter().enumerate() {
                    let untracked = repo.status.untracked.contains(&change.path);
                    let is_active = active
                        .as_ref()
                        .is_some_and(|(p, staged)| p == &change.path && !staged);
                    let is_picked =
                        chosen.len() > 1 && picked.is_picked(Section::Unstaged, &change.path);
                    let (row, staged) = file_row(
                        ui,
                        row_id(Section::Unstaged, &change.path),
                        change,
                        is_active,
                        is_picked,
                        Some("Stage"),
                        None,
                    );
                    nav_rows.push((row.id, row.rect));
                    nav_items.push((Section::Unstaged, index, change.path.clone()));
                    if staged {
                        actions.push(Action::Run(Op::Stage(vec![change.path.clone()])));
                    } else if row.clicked()
                        && picked.click(Section::Unstaged, index, &unstaged_paths, modifiers)
                    {
                        actions.push(Action::Open(change.path.clone(), false));
                    }
                    crate::menus::context_menu(&row, |ui| {
                        crate::menus::menu_width(ui, 210.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if menus::row(ui, Some(Icon::Plus), "Stage", None, false) {
                            actions.push(Action::Run(Op::Stage(vec![change.path.clone()])));
                        }
                        if menus::row(ui, Some(Icon::Stash), "Stash this file…", None, false) {
                            actions.push(Action::Stash(StashPrompt {
                                paths: vec![change.path.clone()],
                                staged_only: false,
                                message: String::new(),
                            }));
                        }
                        let mut commands = Vec::new();
                        menus::file_items(ui, &change.path, &mut commands);
                        actions.extend(commands.into_iter().map(Action::Command));
                        menus::separator(ui);
                        let label = if untracked {
                            "Delete file…"
                        } else {
                            "Discard changes…"
                        };
                        if menus::row(ui, Some(Icon::Trash), label, None, true) {
                            actions.push(Action::Confirm(discard_dialog(&change.path, untracked)));
                        }
                    });
                }
                if chosen.len() > 1 {
                    let (untracked, tracked): (Vec<String>, Vec<String>) = chosen
                        .iter()
                        .cloned()
                        .partition(|p| repo.status.untracked.contains(p));
                    selection_bar(
                        ui,
                        &chosen,
                        Op::Stage(chosen.clone()),
                        "Stage",
                        Some(discard_files_dialog(tracked, untracked)),
                        &mut picked,
                        &mut actions,
                    );
                }
                if repo.status.unstaged.is_empty() {
                    empty_note(ui, "Nothing to stage");
                }
                ui.add_space(10.0);
                let stash_staged = (!repo.status.staged.is_empty()
                    && kelp_core::git_cli::supports_stash_staged())
                .then(|| StashPrompt {
                    paths: Vec::new(),
                    staged_only: true,
                    message: String::new(),
                });
                section(
                    ui,
                    "Staged",
                    repo.status.staged.len(),
                    (!repo.status.staged.is_empty())
                        .then_some(("Unstage all", Op::UnstageAll { has_head })),
                    stash_staged.map(|prompt| ("Stash", prompt)),
                    &mut actions,
                );
                let chosen = picked.in_section(Section::Staged);
                for (index, change) in repo.status.staged.iter().enumerate() {
                    let is_active = active
                        .as_ref()
                        .is_some_and(|(p, staged)| p == &change.path && *staged);
                    let is_picked =
                        chosen.len() > 1 && picked.is_picked(Section::Staged, &change.path);
                    let (row, unstaged) = file_row(
                        ui,
                        row_id(Section::Staged, &change.path),
                        change,
                        is_active,
                        is_picked,
                        Some("Unstage"),
                        None,
                    );
                    nav_rows.push((row.id, row.rect));
                    nav_items.push((Section::Staged, index, change.path.clone()));
                    if unstaged {
                        actions.push(Action::Run(Op::Unstage {
                            paths: vec![change.path.clone()],
                            has_head,
                        }));
                    } else if row.clicked()
                        && picked.click(Section::Staged, index, &staged_paths, modifiers)
                    {
                        actions.push(Action::Open(change.path.clone(), true));
                    }
                    crate::menus::context_menu(&row, |ui| {
                        crate::menus::menu_width(ui, 210.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if menus::row(ui, Some(Icon::Minus), "Unstage", None, false) {
                            actions.push(Action::Run(Op::Unstage {
                                paths: vec![change.path.clone()],
                                has_head,
                            }));
                        }
                        let mut commands = Vec::new();
                        menus::file_items(ui, &change.path, &mut commands);
                        actions.extend(commands.into_iter().map(Action::Command));
                    });
                }
                if chosen.len() > 1 {
                    selection_bar(
                        ui,
                        &chosen,
                        Op::Unstage {
                            paths: chosen.clone(),
                            has_head,
                        },
                        "Unstage",
                        None,
                        &mut picked,
                        &mut actions,
                    );
                }
                if repo.status.staged.is_empty() {
                    empty_note(ui, "Stage files or hunks to commit them");
                }
                let open = active.as_ref().and_then(|(path, staged)| {
                    nav_items
                        .iter()
                        .position(|(s, _, p)| p == path && (*s == Section::Staged) == *staged)
                });
                let owns_keys = open.is_some() && crate::list_keys::nothing_focused(ui.ctx());
                let ctx = ui.ctx().clone();
                if let Some(at) = crate::list_keys::current(&ctx, &nav_rows, open, owns_keys) {
                    let (section, _, path) = &nav_items[at];
                    let path = vec![path.clone()];
                    match section {
                        Section::Unstaged if crate::list_keys::pressed(&ctx, Key::S) => {
                            actions.push(Action::Run(Op::Stage(path)));
                        }
                        Section::Staged if crate::list_keys::pressed(&ctx, Key::U) => {
                            actions.push(Action::Run(Op::Unstage {
                                paths: path,
                                has_head,
                            }));
                        }
                        _ => {}
                    }
                }
                if let Some(at) = crate::list_keys::step(ui, &nav_rows, open, owns_keys) {
                    let (section, index, path) = &nav_items[at];
                    let list = match section {
                        Section::Unstaged => &unstaged_paths,
                        Section::Staged => &staged_paths,
                    };
                    picked.click(*section, *index, list, Modifiers::NONE);
                    actions.push(Action::Open(path.clone(), *section == Section::Staged));
                }
            });
    });

    let mut prompt: Option<StashPrompt> = ui.data(|d| d.get_temp(prompt_id)).or_else(|| {
        let chosen = picked.in_section(Section::Unstaged);
        (std::env::var_os("KELP_OPEN_STASH").is_some() && !chosen.is_empty()).then(|| StashPrompt {
            paths: chosen,
            staged_only: false,
            message: String::new(),
        })
    });
    for action in actions {
        match action {
            Action::Open(path, staged) => repo.open_working_diff(&path, staged),
            Action::OpenConflict(path) => repo.open_conflict(&path),
            Action::Run(op) => {
                picked = Picked::default();
                repo.run_op(op);
            }
            Action::Confirm(dialog) => repo.dialog = Some(dialog),
            Action::Command(command) => repo.execute(ui.ctx(), vec![command]),
            Action::Stash(stash) => prompt = Some(stash),
        }
    }
    if let Some(open) = &mut prompt {
        match stash_prompt(ui.ctx(), open) {
            PromptOutcome::Keep => {}
            PromptOutcome::Cancel => prompt = None,
            PromptOutcome::Stash(op) => {
                prompt = None;
                picked = Picked::default();
                repo.run_op(op);
            }
        }
    }
    ui.data_mut(|d| {
        d.insert_temp(picked_id, picked);
        match prompt {
            Some(open) => {
                d.insert_temp(prompt_id, open);
            }
            None => d.remove::<StashPrompt>(prompt_id),
        }
    });
}

enum PromptOutcome {
    Keep,
    Cancel,
    Stash(Op),
}

fn stash_prompt(ctx: &egui::Context, prompt: &mut StashPrompt) -> PromptOutcome {
    let mut outcome = PromptOutcome::Keep;
    let modal = egui::Modal::new(egui::Id::new("stash-prompt"))
        .backdrop_color(theme::backdrop())
        .show(ctx, |ui| {
            ui.set_width(380.0);
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.label(
                RichText::new(prompt.title())
                    .size(16.0)
                    .family(theme::semibold())
                    .color(theme::text_strong()),
            );
            let field = ui.add(
                egui::TextEdit::singleline(&mut prompt.message)
                    .hint_text("Message (optional)")
                    .desired_width(f32::INFINITY)
                    .margin(Margin::symmetric(10, 8)),
            );
            field.request_focus();
            ui.label(
                RichText::new(prompt.op().command_line())
                    .monospace()
                    .size(11.0)
                    .color(theme::text_faint()),
            );
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let stash = egui::Button::new(
                        RichText::new("Stash")
                            .family(theme::semibold())
                            .color(theme::on_accent()),
                    )
                    .fill(theme::accent())
                    .corner_radius(5);
                    if ui.add(stash).clicked() || enter {
                        outcome = PromptOutcome::Stash(prompt.op());
                    }
                    if ui.button("Cancel").clicked() {
                        outcome = PromptOutcome::Cancel;
                    }
                });
            });
        });
    if modal.should_close() && matches!(outcome, PromptOutcome::Keep) {
        outcome = PromptOutcome::Cancel;
    }
    outcome
}

fn selection_bar(
    ui: &mut Ui,
    chosen: &[String],
    move_op: Op,
    move_label: &str,
    discard: Option<Dialog>,
    picked: &mut Picked,
    actions: &mut Vec<Action>,
) {
    egui::Frame::new()
        .fill(theme::with_alpha(theme::accent(), 0x14))
        .inner_margin(Margin::symmetric(16, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} selected", chosen.len()))
                        .size(12.0)
                        .color(theme::text_strong()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let clear = RichText::new("Clear").size(11.5).color(theme::text_muted());
                    if crate::widgets::text_button(ui, clear).clicked() {
                        *picked = Picked::default();
                    }
                    if let Some(dialog) = discard {
                        let label = RichText::new("Discard…").size(11.5).color(theme::deleted());
                        if crate::widgets::text_button(ui, label).clicked() {
                            actions.push(Action::Confirm(dialog));
                        }
                    }
                    let stash = RichText::new("Stash…").size(11.5).color(theme::text());
                    if crate::widgets::text_button(ui, stash).clicked() {
                        actions.push(Action::Stash(StashPrompt {
                            paths: chosen.to_vec(),
                            staged_only: false,
                            message: String::new(),
                        }));
                    }
                    let label = RichText::new(format!("{move_label} {}", chosen.len()))
                        .size(11.5)
                        .color(theme::accent());
                    if crate::widgets::text_button(ui, label).clicked() {
                        actions.push(Action::Run(move_op));
                    }
                });
            });
        });
}

fn discard_files_dialog(tracked: Vec<String>, untracked: Vec<String>) -> Dialog {
    let total = tracked.len() + untracked.len();
    let body = match untracked.len() {
        0 => "Their changes go away on disk. Cmd+Z brings them back.".to_string(),
        n => format!(
            "Their changes go away on disk, and {} deleted. Cmd+Z brings them back.",
            if n == 1 {
                "1 new file is".to_string()
            } else {
                format!("{n} new files are")
            }
        ),
    };
    Dialog::Confirm {
        title: format!("Discard {}?", plural(total)),
        body,
        op: Op::DiscardFiles { tracked, untracked },
        danger: true,
    }
}

fn discard_dialog(path: &str, untracked: bool) -> Dialog {
    let (title, op) = if untracked {
        (
            format!("Delete {path}?"),
            Op::DeleteUntracked(vec![path.to_string()]),
        )
    } else {
        (
            format!("Discard changes to {path}?"),
            Op::DiscardChanges(vec![path.to_string()]),
        )
    };
    Dialog::Confirm {
        title,
        body: "Cmd+Z brings it back.".into(),
        op,
        danger: true,
    }
}

fn header(ui: &mut Ui, repo: &Repo) {
    egui::Frame::new()
        .inner_margin(Margin {
            left: 16,
            right: 16,
            top: 16,
            bottom: 10,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.label(
                RichText::new("Uncommitted changes")
                    .size(17.0)
                    .family(theme::semibold())
                    .color(theme::text_strong()),
            );
            let branch = repo.current_branch().unwrap_or("detached HEAD");
            ui.label(RichText::new(format!("on {branch}")).color(theme::text_muted()));
        });
}

fn section(
    ui: &mut Ui,
    title: &str,
    count: usize,
    button: Option<(&str, Op)>,
    stash: Option<(&str, StashPrompt)>,
    actions: &mut Vec<Action>,
) {
    egui::Frame::new()
        .inner_margin(Margin::symmetric(16, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(title.to_uppercase())
                        .size(11.0)
                        .family(theme::semibold())
                        .color(theme::text_muted()),
                );
                ui.label(
                    RichText::new(count.to_string())
                        .size(11.0)
                        .color(theme::text_faint()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some((label, op)) = button {
                        let b = RichText::new(label).size(11.0).color(theme::accent());
                        if crate::widgets::text_button(ui, b).clicked() {
                            actions.push(Action::Run(op));
                        }
                    }
                    if let Some((label, prompt)) = stash {
                        let b = RichText::new(label).size(11.0).color(theme::text());
                        if crate::widgets::text_button(ui, b)
                            .on_hover_text("Stash only the staged changes")
                            .clicked()
                        {
                            actions.push(Action::Stash(prompt));
                        }
                    }
                });
            });
        });
}

fn empty_note(ui: &mut Ui, text: &str) {
    egui::Frame::new()
        .inner_margin(Margin::symmetric(16, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).color(theme::text_faint()));
        });
}

fn row_id(section: Section, path: &str) -> egui::Id {
    egui::Id::new(("staging-row", section == Section::Staged, path))
}

fn summary_id(dir: &std::path::Path) -> egui::Id {
    egui::Id::new(("commit-summary", dir))
}

#[allow(clippy::too_many_arguments)]
fn file_row(
    ui: &mut Ui,
    id: egui::Id,
    change: &FileChange,
    active: bool,
    picked: bool,
    button: Option<&str>,
    note: Option<&str>,
) -> (egui::Response, bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    let response = ui.interact(rect, id, Sense::click());
    let painter = ui.painter_at(rect);
    let hovered = response.hovered() || ui.rect_contains_pointer(rect);
    if picked {
        painter.rect_filled(rect, 0.0, theme::with_alpha(theme::accent(), 0x24));
    } else if active {
        painter.rect_filled(rect, 0.0, theme::sidebar_selected());
    } else if hovered {
        painter.rect_filled(rect, 0.0, theme::overlay(0x08));
    }
    let y = rect.center().y;
    let (mark, color) = match change.kind {
        ChangeKind::Added => ("A", theme::added()),
        ChangeKind::Deleted => ("D", theme::deleted()),
        ChangeKind::Modified => ("M", theme::modified()),
        ChangeKind::Renamed => ("R", theme::modified()),
    };
    let font = FontId::monospace(12.0);
    painter.text(
        pos2(rect.left() + 20.0, y),
        Align2::CENTER_CENTER,
        mark,
        font.clone(),
        color,
    );

    let mut right = rect.right() - 12.0;
    let mut clicked = false;
    if let Some(label) = button
        && hovered
    {
        let g = painter.layout_no_wrap(
            label.to_string(),
            FontId::proportional(11.5),
            theme::text_strong(),
        );
        let button_rect = egui::Rect::from_min_size(
            pos2(right - g.size().x - 16.0, y - 10.0),
            vec2(g.size().x + 16.0, 20.0),
        );
        let b = ui.interact(button_rect, response.id.with("action"), Sense::click());
        let fill = if b.hovered() {
            theme::accent()
        } else {
            theme::card_hover()
        };
        let ink = if b.hovered() {
            theme::on_accent()
        } else {
            theme::text_strong()
        };
        painter.rect(
            button_rect,
            5.0,
            fill,
            Stroke::NONE,
            egui::StrokeKind::Inside,
        );
        painter.text(
            button_rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(11.5),
            ink,
        );
        clicked = b.clicked();
        right = button_rect.left() - 8.0;
    } else if let Some(note) = note {
        let g = painter.layout_no_wrap(
            note.to_string(),
            FontId::proportional(11.0),
            theme::text_faint(),
        );
        painter.galley(
            pos2(right - g.size().x, y - g.size().y / 2.0),
            g.clone(),
            theme::text_faint(),
        );
        right -= g.size().x + 8.0;
    }
    let (dir, name) = change
        .path
        .rsplit_once('/')
        .map_or(("", change.path.as_str()), |(d, n)| (d, n));
    let left = rect.left() + 36.0;
    let name_galley = crate::graph_view::truncated(
        &painter,
        name.to_string(),
        font.clone(),
        theme::text(),
        (right - left).max(0.0),
    );
    let name_w = name_galley.size().x;
    let dir_w = (right - left - name_w).max(0.0);
    let mut x = left;
    if !dir.is_empty() && dir_w > 24.0 {
        let g = crate::graph_view::truncated(
            &painter,
            format!("{dir}/"),
            font,
            theme::text_faint(),
            dir_w,
        );
        painter.galley(
            pos2(x, y - g.size().y / 2.0),
            g.clone(),
            theme::text_faint(),
        );
        x += g.size().x;
    }
    painter.galley(
        pos2(x, y - name_galley.size().y / 2.0),
        name_galley,
        theme::text(),
    );
    crate::widgets::focus_ring(ui, &response, 0.0);
    crate::focus_areas::offer(
        ui.ctx(),
        crate::focus_areas::Area::Details,
        response.id,
        active,
    );
    crate::widgets::describe_selected(
        &response,
        format!("changed file {}, {mark}", change.path),
        active || picked,
    );
    (response.on_hover_text(&change.path), clicked)
}

fn commit_box(ui: &mut Ui, repo: &mut Repo) {
    let staged = repo.status.staged.len();
    egui::Frame::new()
        .fill(theme::inset())
        .stroke(Stroke::new(1.0, theme::card_active()))
        .corner_radius(10)
        .inner_margin(Margin::same(14))
        .outer_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.horizontal(|ui| {
                let title = if repo.amend {
                    "Amend last commit"
                } else {
                    "Commit"
                };
                ui.label(
                    RichText::new(title)
                        .family(theme::semibold())
                        .color(theme::text_strong()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let len = repo.commit_summary.chars().count();
                    let color = if len > SUMMARY_LIMIT {
                        theme::modified()
                    } else {
                        theme::text_faint()
                    };
                    ui.label(
                        RichText::new(format!("{len}/{SUMMARY_LIMIT}"))
                            .size(11.0)
                            .color(color),
                    );
                    commit_helpers::conventional_toggle(ui, repo);
                });
            });
            commit_helpers::type_row(ui, repo);
            let summary = ui.add(
                egui::TextEdit::singleline(&mut repo.commit_summary)
                    .id(summary_id(&repo.dir))
                    .hint_text("Summary (required)")
                    .desired_width(f32::INFINITY)
                    .margin(Margin::symmetric(10, 8)),
            );
            let body = ui.add(
                egui::TextEdit::multiline(&mut repo.commit_body)
                    .hint_text("Description")
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .margin(Margin::symmetric(10, 8)),
            );
            commit_helpers::co_author_row(ui, repo);
            let mut amend = repo.amend;
            if ui
                .checkbox(
                    &mut amend,
                    RichText::new("Amend previous commit")
                        .size(12.0)
                        .color(theme::text_muted()),
                )
                .changed()
            {
                repo.toggle_amend();
            }
            let ready = !repo.commit_summary.trim().is_empty()
                && (staged > 0 || repo.amend)
                && !repo.committing();
            let label = match (repo.amend, staged) {
                (true, _) => "Amend commit".to_string(),
                (false, 1) => "Commit 1 file".to_string(),
                (false, n) => format!("Commit {n} files"),
            };
            let ink = if ready {
                theme::on_accent()
            } else {
                theme::text_faint()
            };
            let button =
                egui::Button::new(RichText::new(label).family(theme::semibold()).color(ink))
                    .fill(if ready {
                        theme::accent()
                    } else {
                        theme::card_hover()
                    })
                    .corner_radius(6)
                    .min_size(vec2(ui.available_width() - 36.0, 34.0));
            let (shortcut, push_shortcut) = if summary.has_focus() || body.has_focus() {
                ui.input(|i| {
                    let enter = i.modifiers.command && i.key_pressed(Key::Enter);
                    (enter && !i.modifiers.shift, enter && i.modifiers.shift)
                })
            } else {
                (false, false)
            };
            let hint = if staged == 0 && !repo.amend {
                "Stage changes first"
            } else {
                "Commit (⌘ Enter)"
            };
            let mut push = false;
            let mut clicked = false;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                clicked = ui.add_enabled(ready, button).on_hover_text(hint).clicked();
                let more = commit_helpers::more_button(
                    ui,
                    commit_helpers::more_button_id(&repo.dir),
                    ready,
                )
                .on_hover_text("More commit options");
                egui::Popup::menu(&more).show(|ui| {
                    if ui.button("Commit and push (⌘ Shift Enter)").clicked() {
                        push = true;
                        ui.close();
                    }
                });
            });
            if !ready {
                return;
            }
            if push || push_shortcut {
                repo.commit_and_push();
            } else if clicked || shortcut {
                repo.commit();
            }
        });
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, vec2};
    use kelp_core::git_cli::run;

    use super::{Picked, Section, row_id, summary_id};
    use crate::commit_helpers;
    use crate::repo_view::{Repo, Selection};

    const LIST: [&str; 4] = ["a.txt", "b.txt", "c.txt", "d.txt"];

    #[test]
    fn plain_click_picks_one_and_opens_it() {
        let mut picked = Picked::default();
        assert!(picked.click(Section::Unstaged, 1, &LIST, Modifiers::NONE));
        assert_eq!(picked.in_section(Section::Unstaged), ["b.txt"]);
        assert!(picked.click(Section::Unstaged, 3, &LIST, Modifiers::NONE));
        assert_eq!(picked.in_section(Section::Unstaged), ["d.txt"]);
    }

    #[test]
    fn command_click_toggles_and_shift_click_adds_a_range() {
        let mut picked = Picked::default();
        picked.click(Section::Unstaged, 0, &LIST, Modifiers::NONE);
        assert!(!picked.click(Section::Unstaged, 2, &LIST, Modifiers::COMMAND));
        assert_eq!(picked.in_section(Section::Unstaged), ["a.txt", "c.txt"]);
        assert!(!picked.click(Section::Unstaged, 0, &LIST, Modifiers::COMMAND));
        assert_eq!(picked.in_section(Section::Unstaged), ["c.txt"]);

        picked.click(Section::Unstaged, 1, &LIST, Modifiers::NONE);
        assert!(!picked.click(Section::Unstaged, 3, &LIST, Modifiers::SHIFT));
        assert_eq!(
            picked.in_section(Section::Unstaged),
            ["b.txt", "c.txt", "d.txt"]
        );
    }

    #[test]
    fn picking_stays_inside_one_section_and_drops_vanished_files() {
        let mut picked = Picked::default();
        picked.click(Section::Unstaged, 0, &LIST, Modifiers::NONE);
        picked.click(Section::Unstaged, 1, &LIST, Modifiers::COMMAND);
        picked.click(Section::Staged, 2, &LIST, Modifiers::COMMAND);
        assert_eq!(picked.in_section(Section::Staged), ["c.txt"]);
        assert!(picked.in_section(Section::Unstaged).is_empty());
        picked.keep_only(Section::Staged, &["a.txt"]);
        assert!(picked.in_section(Section::Staged).is_empty());
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.email", "t@example.com"],
            &["config", "user.name", "T"],
        ] {
            run(&dir, args).unwrap();
        }
        for name in LIST {
            std::fs::write(dir.join(name), "one\n").unwrap();
        }
        run(&dir, &["add", "."]).unwrap();
        run(&dir, &["commit", "-q", "-m", "base"]).unwrap();
        for name in LIST {
            std::fs::write(dir.join(name), "two\n").unwrap();
        }
        dir
    }

    fn frame(ctx: &egui::Context, repo: &mut Repo, events: Vec<Event>, modifiers: Modifiers) {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 900.0))),
            events,
            modifiers,
            ..Default::default()
        };
        let _ = ctx.run_ui(input, |ui| super::ui(ui, repo));
    }

    fn click_row(ctx: &egui::Context, repo: &mut Repo, path: &str, modifiers: Modifiers) {
        let rect = ctx
            .read_response(row_id(Section::Unstaged, path))
            .expect("row was drawn")
            .rect;
        let at = rect.left_center() + vec2(60.0, 0.0);
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        };
        frame(ctx, repo, vec![Event::PointerMoved(at)], modifiers);
        frame(ctx, repo, vec![press(true)], modifiers);
        frame(ctx, repo, vec![press(false)], modifiers);
        frame(ctx, repo, vec![], Modifiers::NONE);
    }

    #[test]
    fn rows_answer_click_command_click_and_shift_click() {
        let dir = scratch("kelp-staging-ui");
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let (git, history) = kelp_core::history::History::open(&dir).unwrap();
        let mut repo = Repo::new(&ctx, git, history, Duration::ZERO);
        repo.selected = Some(Selection::Wip);
        let started = Instant::now();
        while repo.status.unstaged.len() < LIST.len() && started.elapsed() < Duration::from_secs(5)
        {
            repo.poll(&ctx, None);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(repo.status.unstaged.len(), LIST.len());
        frame(&ctx, &mut repo, vec![], Modifiers::NONE);

        click_row(&ctx, &mut repo, "a.txt", Modifiers::NONE);
        click_row(&ctx, &mut repo, "c.txt", Modifiers::COMMAND);
        click_row(&ctx, &mut repo, "d.txt", Modifiers::SHIFT);
        let picked: Picked = ctx
            .data(|d| d.get_temp(egui::Id::new(("staging-picked", &repo.dir))))
            .unwrap();
        assert_eq!(
            picked.in_section(Section::Unstaged),
            ["a.txt", "c.txt", "d.txt"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn press(ctx: &egui::Context, repo: &mut Repo, key: egui::Key) {
        let event = |pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        frame(ctx, repo, vec![event(true), event(false)], Modifiers::NONE);
        frame(ctx, repo, vec![], Modifiers::NONE);
    }

    fn open_path(repo: &Repo) -> Option<(String, bool)> {
        match &repo.center {
            crate::repo_view::Center::Diff(view) => {
                Some((view.path().to_string(), view.is_staged()))
            }
            _ => None,
        }
    }

    #[test]
    fn arrows_walk_the_changed_files_and_s_stages_the_open_one() {
        let dir = scratch("kelp-staging-keys");
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let (git, history) = kelp_core::history::History::open(&dir).unwrap();
        let mut repo = Repo::new(&ctx, git, history, Duration::ZERO);
        repo.selected = Some(Selection::Wip);
        let started = Instant::now();
        while repo.status.unstaged.len() < LIST.len() && started.elapsed() < Duration::from_secs(5)
        {
            repo.poll(&ctx, None);
            std::thread::sleep(Duration::from_millis(20));
        }
        frame(&ctx, &mut repo, vec![], Modifiers::NONE);
        click_row(&ctx, &mut repo, "a.txt", Modifiers::NONE);
        assert_eq!(open_path(&repo), Some(("a.txt".into(), false)));

        press(&ctx, &mut repo, egui::Key::ArrowDown);
        assert_eq!(open_path(&repo), Some(("b.txt".into(), false)));
        press(&ctx, &mut repo, egui::Key::End);
        assert_eq!(open_path(&repo), Some(("d.txt".into(), false)));
        press(&ctx, &mut repo, egui::Key::ArrowUp);
        assert_eq!(open_path(&repo), Some(("c.txt".into(), false)));

        press(&ctx, &mut repo, egui::Key::S);
        let started = Instant::now();
        while repo.status.staged.is_empty() && started.elapsed() < Duration::from_secs(5) {
            repo.poll(&ctx, None);
            std::thread::sleep(Duration::from_millis(20));
        }
        let staged: Vec<&str> = repo.status.staged.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(staged, ["c.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn click_id(ctx: &egui::Context, repo: &mut Repo, id: egui::Id) {
        for _ in 0..2 {
            frame(ctx, repo, vec![], Modifiers::NONE);
        }
        let at = ctx
            .read_response(id)
            .expect("widget was drawn")
            .rect
            .center();
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(ctx, repo, vec![Event::PointerMoved(at)], Modifiers::NONE);
        frame(ctx, repo, vec![press(true)], Modifiers::NONE);
        frame(ctx, repo, vec![press(false)], Modifiers::NONE);
        frame(ctx, repo, vec![], Modifiers::NONE);
    }

    fn open_with_staged_change(dir: &std::path::Path, ctx: &egui::Context) -> Repo {
        run(dir, &["add", "a.txt"]).unwrap();
        let (git, history) = kelp_core::history::History::open(dir).unwrap();
        let mut repo = Repo::new(ctx, git, history, Duration::ZERO);
        repo.selected = Some(Selection::Wip);
        let started = Instant::now();
        while repo.status.staged.is_empty() && started.elapsed() < Duration::from_secs(5) {
            repo.poll(ctx, None);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(repo.status.staged.len(), 1);
        repo
    }

    #[test]
    fn the_type_picker_rewrites_the_summary_prefix() {
        let dir = scratch("kelp-commit-type");
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut repo = open_with_staged_change(&dir, &ctx);
        repo.commit_prefs.conventional = Some(true);
        repo.commit_summary = "add arcs".into();
        frame(&ctx, &mut repo, vec![], Modifiers::NONE);

        click_id(&ctx, &mut repo, commit_helpers::type_button_id(&dir));
        click_id(&ctx, &mut repo, commit_helpers::type_item_id(&dir, "fix"));
        assert_eq!(repo.commit_summary, "fix: add arcs");

        click_id(&ctx, &mut repo, commit_helpers::type_button_id(&dir));
        click_id(&ctx, &mut repo, commit_helpers::type_item_id(&dir, "docs"));
        assert_eq!(repo.commit_summary, "docs: add arcs");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_shift_enter_commits_then_pushes() {
        let dir = scratch("kelp-commit-push");
        let remote = dir.with_extension("git");
        let _ = std::fs::remove_dir_all(&remote);
        run(&dir, &["init", "-q", "--bare", remote.to_str().unwrap()]).unwrap();
        run(&dir, &["remote", "add", "origin", remote.to_str().unwrap()]).unwrap();
        run(&dir, &["push", "-q", "-u", "origin", "main"]).unwrap();
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut repo = open_with_staged_change(&dir, &ctx);
        repo.commit_summary = "ship it".into();
        frame(&ctx, &mut repo, vec![], Modifiers::NONE);
        ctx.memory_mut(|m| m.request_focus(summary_id(&dir)));
        frame(&ctx, &mut repo, vec![], Modifiers::NONE);

        let chord = Modifiers::COMMAND | Modifiers::SHIFT;
        let enter = Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: chord,
        };
        frame(&ctx, &mut repo, vec![enter], chord);
        assert!(repo.committing());
        assert!(repo.push_after_commit);

        let head = || run(&dir, &["rev-parse", "HEAD"]).unwrap();
        let pushed = || run(&remote, &["rev-parse", "main"]).unwrap();
        let started = Instant::now();
        while (repo.committing() || pushed() != head())
            && started.elapsed() < Duration::from_secs(10)
        {
            repo.poll(&ctx, None);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            run(&dir, &["log", "-1", "--format=%s"]).unwrap().trim(),
            "ship it"
        );
        assert_eq!(pushed(), head());
        assert!(!repo.push_after_commit);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&remote);
    }
}
