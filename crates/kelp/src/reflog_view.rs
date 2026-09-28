use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Margin, Rect, RichText, Sense, Ui, pos2, vec2,
};
use gix::ObjectId;
use kelp_core::commit::{self, Details};
use kelp_core::ops::{Op, ResetMode};
use kelp_core::reflog::{self, Action, Entry};
use kelp_core::review::Review;

use crate::commands::Command;
use crate::dialogs::Dialog;
use crate::diff_view::{self, DiffSource, DiffView};
use crate::{theme, widgets};

const LIST_W: f32 = 560.0;
const ROW_H: f32 = 30.0;
const PILL_W: f32 = 84.0;
const FILE_ROW_H: f32 = 26.0;
const HEAD: &str = "HEAD";

enum Loaded {
    Loading(Receiver<Result<Vec<Entry>, String>>),
    Ready(Vec<Entry>),
    Failed(String),
}

pub struct ReflogView {
    dir: PathBuf,
    reference: String,
    branches: Vec<String>,
    loaded: Loaded,
    selected: Option<usize>,
    details: Option<Details>,
    diff: Option<Box<DiffView>>,
}

pub enum Event {
    None,
    Close,
    Command(Command),
    Diff(diff_view::Event),
}

impl ReflogView {
    pub fn open(ctx: &egui::Context, dir: PathBuf, reference: String) -> Self {
        let branches = reflog::local_branches(&dir);
        Self {
            loaded: load(ctx, &dir, &reference),
            dir,
            reference,
            branches,
            selected: None,
            details: None,
            diff: None,
        }
    }

    pub fn reload(&mut self, ctx: &egui::Context) {
        self.loaded = load(ctx, &self.dir, &self.reference);
    }

    fn poll(&mut self, repo: &gix::Repository) {
        let Loaded::Loading(rx) = &self.loaded else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(entries)) => {
                let keep = self.selected_id();
                self.loaded = Loaded::Ready(entries);
                let row = keep
                    .and_then(|id| self.entries().iter().position(|e| e.id == id))
                    .unwrap_or(0);
                self.select(repo, row);
            }
            Ok(Err(e)) => self.loaded = Loaded::Failed(e),
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.loaded = Loaded::Failed("the reflog could not be read".into())
            }
        }
    }

    fn entries(&self) -> &[Entry] {
        match &self.loaded {
            Loaded::Ready(entries) => entries,
            _ => &[],
        }
    }

    fn selected_id(&self) -> Option<ObjectId> {
        self.selected
            .and_then(|i| self.entries().get(i))
            .map(|e| e.id)
    }

    fn select(&mut self, repo: &gix::Repository, row: usize) {
        let Some(id) = self.entries().get(row).map(|e| e.id) else {
            self.selected = None;
            self.details = None;
            return;
        };
        self.selected = Some(row);
        self.diff = None;
        self.details = commit::details(repo, id).ok();
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        repo: &gix::Repository,
        workdir: Option<&Path>,
        review: &mut Review,
        author: &str,
        current_branch: Option<&str>,
    ) -> Event {
        self.poll(repo);
        let mut event = Event::None;
        if self.header(ui) {
            event = Event::Close;
        }
        let mut picked = None;
        egui::Panel::left("reflog-entries")
            .default_size(LIST_W)
            .min_size(360.0)
            .frame(egui::Frame::new().fill(theme::PANEL))
            .show(ui, |ui| picked = self.list(ui));
        if let Some(row) = picked {
            self.select(repo, row);
        }
        let Some(entry) = self.selected.and_then(|i| self.entries().get(i)).cloned() else {
            let text = match &self.loaded {
                Loaded::Loading(_) => "Reading the reflog…".to_string(),
                Loaded::Failed(e) => e.clone(),
                Loaded::Ready(_) => "This reflog is empty.".to_string(),
            };
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new(text).color(theme::TEXT_MUTED));
            });
            return event;
        };
        if let Some(diff) = &mut self.diff {
            match diff.ui(ui, review, author) {
                diff_view::Event::Close => self.diff = None,
                diff_view::Event::None => {}
                other => event = Event::Diff(other),
            }
            return event;
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(20, 16))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 10.0;
                        let at_head = repo.head_id().ok().map(|id| id.detach()) == Some(entry.id);
                        let (open, command) =
                            self.details_card(ui, &entry, current_branch, at_head);
                        if let Some(command) = command {
                            event = Event::Command(command);
                        }
                        if let Some(path) = open {
                            match DiffView::load(repo, workdir, DiffSource::Commit(entry.id), &path)
                            {
                                Ok(view) => self.diff = Some(Box::new(view)),
                                Err(e) => {
                                    ui.label(RichText::new(format!("{e:#}")).color(theme::DELETED));
                                }
                            }
                        }
                    });
            });
        event
    }

    fn header(&mut self, ui: &mut Ui) -> bool {
        let mut close = false;
        let mut switch_to = None;
        egui::Frame::new()
            .fill(theme::PANEL)
            .inner_margin(Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Reflog")
                            .size(15.0)
                            .family(theme::semibold())
                            .color(theme::TEXT_STRONG),
                    );
                    egui::ComboBox::from_id_salt("reflog-ref")
                        .selected_text(&self.reference)
                        .show_ui(ui, |ui| {
                            for name in std::iter::once(HEAD).chain(self.branches.iter().map(String::as_str)) {
                                if ui.selectable_label(self.reference == name, name).clicked() {
                                    switch_to = Some(name.to_string());
                                }
                            }
                        });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close = widgets::close_button(ui, "Close (Esc)");
                    });
                });
                ui.label(
                    RichText::new(
                        "Every place HEAD or a branch pointed to, newest first. Commits marked lost are on no branch any more; restore them from here.",
                    )
                    .size(12.0)
                    .color(theme::TEXT_MUTED),
                );
            });
        if let Some(reference) = switch_to
            && reference != self.reference
        {
            self.reference = reference;
            self.selected = None;
            self.details = None;
            self.diff = None;
            self.reload(ui.ctx());
        }
        close
    }

    fn list(&self, ui: &mut Ui) -> Option<usize> {
        let entries = self.entries();
        let now = now();
        let mut picked = None;
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(
            ui,
            ROW_H,
            entries.len(),
            |ui, rows| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for row in rows {
                    if entry_row(ui, &entries[row], self.selected == Some(row), now) {
                        picked = Some(row);
                    }
                }
            },
        );
        picked
    }

    fn details_card(
        &self,
        ui: &mut Ui,
        entry: &Entry,
        current_branch: Option<&str>,
        at_head: bool,
    ) -> (Option<String>, Option<Command>) {
        ui.label(
            RichText::new(&entry.title)
                .size(16.0)
                .family(theme::semibold())
                .color(theme::TEXT_STRONG),
        );
        ui.label(
            RichText::new(format!(
                "{}  ·  {}  ·  {}",
                entry.action.label(),
                entry.message,
                entry.id.to_hex_with_len(7)
            ))
            .size(12.0)
            .color(theme::TEXT_MUTED),
        );
        let command = action_bar(ui, entry, current_branch, at_head);
        let Some(details) = self.details.as_ref() else {
            return (None, command);
        };
        ui.label(
            RichText::new(format!(
                "{} · {}",
                details.author,
                commit::relative_time(details.time, now())
            ))
            .color(theme::TEXT),
        );
        if !details.body.is_empty() {
            ui.label(RichText::new(&details.body).color(theme::TEXT_MUTED));
        }
        ui.separator();
        let mut open = None;
        ui.spacing_mut().item_spacing.y = 0.0;
        for change in &details.changes {
            if file_row(ui, change) {
                open = Some(change.path.clone());
            }
        }
        if details.changes.is_empty() {
            ui.label(RichText::new("No file changes").color(theme::TEXT_FAINT));
        }
        (open, command)
    }
}

fn load(ctx: &egui::Context, dir: &Path, reference: &str) -> Loaded {
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    let dir = dir.to_path_buf();
    let reference = reference.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(reflog::load(&dir, &reference).map_err(|e| format!("{e:#}")));
        ctx.request_repaint();
    });
    Loaded::Loading(rx)
}

pub fn restore_command(entry: &Entry, current_branch: Option<&str>) -> Command {
    let short = entry.id.to_hex_with_len(7).to_string();
    let target = current_branch.unwrap_or(HEAD);
    Command::Open(Dialog::Confirm {
        title: format!("Restore {target} to {short}?"),
        body: format!(
            "Moves {target} to this commit with git reset --keep. Uncommitted changes are kept; if one would be overwritten nothing changes. Cmd+Z undoes it."
        ),
        op: Op::Reset {
            commit: entry.id.to_string(),
            mode: ResetMode::Keep,
        },
        danger: false,
    })
}

pub fn action_bar(
    ui: &mut Ui,
    entry: &Entry,
    current_branch: Option<&str>,
    at_head: bool,
) -> Option<Command> {
    let mut picked = None;
    let id = entry.id.to_string();
    ui.horizontal_wrapped(|ui| {
        let label = match current_branch {
            Some(branch) => format!("Restore {branch} here"),
            None => "Restore HEAD here".to_string(),
        };
        let restore = ui
            .add_enabled(
                !at_head,
                egui::Button::new(
                    RichText::new(label)
                        .family(theme::semibold())
                        .color(Color32::from_rgb(0x10, 0x13, 0x1a)),
                )
                .fill(theme::ACCENT),
            )
            .on_disabled_hover_text("Already here");
        if restore.clicked() {
            picked = Some(restore_command(entry, current_branch));
        }
        if ui.button("Create branch here…").clicked() {
            picked = Some(Command::Open(Dialog::NewBranch {
                name: String::new(),
                start: id.clone(),
                start_label: id[..7].to_string(),
                switch: true,
            }));
        }
        if ui.button("Check out (detached)").clicked() {
            picked = Some(Command::Run(Op::SwitchDetached(id.clone())));
        }
        if ui.button("Cherry-pick").clicked() {
            picked = Some(Command::Run(Op::CherryPick {
                commit: id.clone(),
                merge: false,
            }));
        }
    });
    picked
}

fn action_color(action: Action) -> Color32 {
    match action {
        Action::Commit | Action::Amend => theme::ADDED,
        Action::Checkout | Action::Branch | Action::Clone => theme::LANES[3],
        Action::Rebase | Action::CherryPick | Action::Revert => theme::LANES[2],
        Action::Reset => theme::DELETED,
        Action::Merge | Action::Pull => theme::LANES[1],
        Action::Other => theme::TEXT_MUTED,
    }
}

fn entry_row(ui: &mut Ui, entry: &Entry, selected: bool, now: i64) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 0.0, theme::SELECTED_ROW);
    } else if response.hovered() {
        painter.rect_filled(rect, 0.0, theme::with_alpha(Color32::WHITE, 0x06));
    }
    let color = action_color(entry.action);
    let pill = Rect::from_min_size(
        pos2(rect.left() + 12.0, rect.center().y - 10.0),
        vec2(PILL_W, 20.0),
    );
    painter.rect_filled(pill, CornerRadius::same(5), theme::with_alpha(color, 0x26));
    painter.text(
        pill.center(),
        Align2::CENTER_CENTER,
        entry.action.label(),
        FontId::proportional(11.5),
        color,
    );
    let hash_x = pill.right() + 10.0;
    painter.text(
        pos2(hash_x, rect.center().y),
        Align2::LEFT_CENTER,
        entry.id.to_hex_with_len(7).to_string(),
        FontId::monospace(12.0),
        theme::TEXT_MUTED,
    );
    let when = commit::relative_time(entry.time, now);
    let when = painter.layout_no_wrap(when, FontId::proportional(11.5), theme::TEXT_FAINT);
    let when_x = rect.right() - 12.0 - when.size().x;
    painter.galley(
        pos2(when_x, rect.center().y - when.size().y / 2.0),
        when,
        theme::TEXT_FAINT,
    );
    let mut title_right = when_x - 10.0;
    if !entry.reachable {
        let lost =
            painter.layout_no_wrap("lost".into(), FontId::proportional(11.0), theme::DELETED);
        let badge = Rect::from_min_size(
            pos2(title_right - lost.size().x - 12.0, rect.center().y - 9.0),
            vec2(lost.size().x + 12.0, 18.0),
        );
        painter.rect_filled(
            badge,
            CornerRadius::same(9),
            theme::with_alpha(theme::DELETED, 0x26),
        );
        painter.galley(badge.center() - lost.size() / 2.0, lost, theme::DELETED);
        title_right = badge.left() - 8.0;
    }
    let title_x = hash_x + 70.0;
    let title = crate::graph_view::truncated(
        &painter,
        entry.title.clone(),
        FontId::proportional(13.0),
        if entry.reachable {
            theme::TEXT
        } else {
            theme::TEXT_STRONG
        },
        (title_right - title_x).max(0.0),
    );
    painter.galley(
        pos2(title_x, rect.center().y - title.size().y / 2.0),
        title,
        theme::TEXT,
    );
    response
        .on_hover_text(format!("{}: {}", entry.action.label(), entry.message))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

fn file_row(ui: &mut Ui, change: &commit::FileChange) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), FILE_ROW_H), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 4.0, theme::with_alpha(Color32::WHITE, 0x08));
    }
    let (letter, color) = crate::details::change_letter(change.kind);
    ui.painter().text(
        rect.left_center() + vec2(8.0, 0.0),
        Align2::LEFT_CENTER,
        letter,
        FontId::monospace(12.0),
        color,
    );
    ui.painter().text(
        rect.left_center() + vec2(26.0, 0.0),
        Align2::LEFT_CENTER,
        &change.path,
        FontId::monospace(12.0),
        theme::TEXT,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use eframe::egui::{self, Event, PointerButton, Pos2, RawInput, Rect, vec2};

    use super::*;

    fn entry() -> Entry {
        Entry {
            id: ObjectId::from_hex("a".repeat(40).as_bytes()).unwrap(),
            action: Action::Reset,
            message: "moving to HEAD~1".into(),
            time: 0,
            title: "third".into(),
            reachable: false,
        }
    }

    fn frame(ctx: &egui::Context, events: Vec<Event>, start: &mut Pos2) -> Option<Command> {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 200.0))),
            events,
            ..Default::default()
        };
        let mut picked = None;
        let _ = ctx.run_ui(input, |ui| {
            *start = ui.cursor().min;
            picked = action_bar(ui, &entry(), Some("main"), false);
        });
        picked
    }

    #[test]
    fn restore_asks_first_and_resets_with_keep() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut start = Pos2::ZERO;
        frame(&ctx, vec![], &mut start);
        let at = start + vec2(24.0, 12.0);
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(&ctx, vec![Event::PointerMoved(at)], &mut start);
        frame(&ctx, vec![press(true)], &mut start);
        match frame(&ctx, vec![press(false)], &mut start) {
            Some(Command::Open(Dialog::Confirm {
                title, op, danger, ..
            })) => {
                assert_eq!(title, "Restore main to aaaaaaa?");
                assert!(!danger);
                assert_eq!(
                    op,
                    Op::Reset {
                        commit: "a".repeat(40),
                        mode: ResetMode::Keep,
                    }
                );
            }
            _ => panic!("restore should open a confirm"),
        }
    }
}
