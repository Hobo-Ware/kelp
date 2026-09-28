use std::collections::HashSet;
use std::path::Path;

use eframe::egui::{
    self, Align, Color32, CornerRadius, CursorIcon, FontId, Margin, Rect, RichText, Sense, Stroke,
    Ui, vec2,
};
use kelp_core::conflict::{
    self, Choice, InProgress, Operation, Parsed, Region, Side, Stages, Step,
};

use crate::{theme, widgets};

const HEADER_H: f32 = 48.0;
const BANNER_H: f32 = 44.0;
const CONTEXT_EDGE: usize = 3;
const COLUMN_GAP: f32 = 12.0;
const CARD_W: f32 = 1100.0;
const KEPT_BG: Color32 = Color32::from_rgb(0x16, 0x30, 0x2a);
const DROPPED_BG: Color32 = Color32::from_rgb(0x3a, 0x1f, 0x22);
const PENDING_BG: Color32 = Color32::from_rgb(0x17, 0x1a, 0x20);
const CONTEXT_TEXT: Color32 = Color32::from_rgb(0x8f, 0x96, 0xa2);

pub enum Job {
    Resolve {
        path: String,
        content: Option<String>,
    },
    TakeSide {
        path: String,
        side: Side,
    },
    Step(Operation, Step),
}

impl Job {
    pub fn label(&self) -> String {
        match self {
            Job::Resolve { path, .. } => format!("Resolving {path}"),
            Job::TakeSide { path, side } => {
                format!("Taking {} for {path}", side_name(*side))
            }
            Job::Step(operation, step) => step.label(*operation),
        }
    }

    pub fn run(self, dir: &Path) -> anyhow::Result<String> {
        match self {
            Job::Resolve { path, content } => {
                conflict::mark_resolved(dir, &path, content.as_deref()).map(|()| String::new())
            }
            Job::TakeSide { path, side } => {
                conflict::take_side(dir, &path, side).map(|()| String::new())
            }
            Job::Step(operation, step) => conflict::run_step(dir, operation, step),
        }
    }
}

pub enum Event {
    None,
    Close,
    Run(Job),
}

fn side_name(side: Side) -> &'static str {
    match side {
        Side::Ours => "ours",
        Side::Theirs => "theirs",
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Conflicts,
    Result,
}

enum Body {
    Text(Parsed),
    Binary,
    DeletedOnOneSide,
}

pub struct ConflictView {
    pub path: String,
    body: Body,
    stages: Stages,
    choices: Vec<Option<Choice>>,
    focus: Option<usize>,
    current: usize,
    mode: Mode,
    open_bases: HashSet<usize>,
}

impl ConflictView {
    pub fn load(workdir: &Path, path: &str) -> anyhow::Result<Self> {
        let stages = conflict::stages(workdir, path).unwrap_or_default();
        let one_side_deleted = stages.ours != stages.theirs;
        let body = match std::fs::read(workdir.join(path)) {
            _ if one_side_deleted => Body::DeletedOnOneSide,
            Err(_) => Body::DeletedOnOneSide,
            Ok(bytes) if bytes.iter().take(8000).any(|&b| b == 0) => Body::Binary,
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => Body::Text(conflict::parse(&text)),
                Err(_) => Body::Binary,
            },
        };
        let count = match &body {
            Body::Text(parsed) => parsed.conflict_count(),
            Body::Binary | Body::DeletedOnOneSide => 0,
        };
        Ok(Self {
            path: path.to_string(),
            body,
            stages,
            choices: vec![None; count],
            focus: None,
            current: 0,
            mode: Mode::Conflicts,
            open_bases: HashSet::new(),
        })
    }

    pub fn preselect(&mut self, picks: &str) {
        let (picks, show_result) = match picks.strip_suffix(":result") {
            Some(picks) => (picks, true),
            None => (picks, false),
        };
        let parsed: Vec<Option<Choice>> = picks
            .split(',')
            .map(|pick| match pick.trim() {
                "ours" => Some(Choice::Ours),
                "theirs" => Some(Choice::Theirs),
                "both" => Some(Choice::Both),
                _ => None,
            })
            .collect();
        for (slot, pick) in self.choices.iter_mut().zip(parsed) {
            *slot = pick;
        }
        self.current = self.choices.iter().position(Option::is_none).unwrap_or(0);
        if show_result {
            self.mode = Mode::Result;
        }
    }

    fn resolved(&self) -> Option<String> {
        match &self.body {
            Body::Text(parsed) if self.choices.iter().all(Option::is_some) => {
                Some(parsed.render(&self.choices))
            }
            _ => None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) -> Event {
        let mut event = self.header(ui);
        match &self.body {
            Body::Text(_) if self.mode == Mode::Result => self.result(ui),
            Body::Text(_) => self.conflicts(ui),
            Body::Binary => {
                if let Some(side) = whole_file_notice(
                    ui,
                    "This file is binary, so there are no lines to pick from.",
                    &self.stages,
                ) {
                    event = self.take(side);
                }
            }
            Body::DeletedOnOneSide => {
                let note = if self.stages.ours {
                    "Deleted on their side, changed on ours."
                } else {
                    "Deleted on our side, changed on theirs."
                };
                if let Some(side) = whole_file_notice(ui, note, &self.stages) {
                    event = self.take(side);
                }
            }
        }
        event
    }

    fn take(&self, side: Side) -> Event {
        Event::Run(Job::TakeSide {
            path: self.path.clone(),
            side,
        })
    }

    fn header(&mut self, ui: &mut Ui) -> Event {
        let mut event = Event::None;
        let total = self.choices.len();
        let left = self.choices.iter().filter(|c| c.is_none()).count();
        egui::Frame::new()
            .fill(theme::PANEL)
            .inner_margin(Margin::symmetric(16, 0))
            .show(ui, |ui| {
                ui.set_height(HEADER_H);
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    let (dir, name) = self
                        .path
                        .rsplit_once('/')
                        .map_or(("", self.path.as_str()), |(d, n)| (d, n));
                    if !dir.is_empty() {
                        ui.label(
                            RichText::new(format!("{dir}/"))
                                .monospace()
                                .color(theme::TEXT_FAINT),
                        );
                        ui.add_space(-12.0);
                    }
                    ui.label(
                        RichText::new(name)
                            .monospace()
                            .family(theme::semibold())
                            .color(theme::TEXT_STRONG),
                    );
                    if matches!(self.body, Body::Text(_)) {
                        let summary = match (total, left) {
                            (0, _) => "no conflict markers left".to_string(),
                            (t, 0) => format!("{t} resolved"),
                            (t, l) => format!("{l} of {t} left"),
                        };
                        let color = if left == 0 {
                            theme::ADDED
                        } else {
                            theme::MODIFIED
                        };
                        ui.label(RichText::new(summary).size(12.0).color(color));
                    }
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if widgets::close_button(ui, "Close (Esc)") {
                            event = Event::Close;
                        }
                        ui.add_space(4.0);
                        let Body::Text(parsed) = &self.body else {
                            return;
                        };
                        let ready = self.choices.iter().all(Option::is_some);
                        let mark = ui
                            .add_enabled(ready, accent_button("Mark resolved"))
                            .on_hover_text("Writes the file and stages it")
                            .on_disabled_hover_text("Pick a side for every conflict first");
                        if mark.clicked() {
                            let content =
                                (parsed.conflict_count() > 0).then(|| parsed.render(&self.choices));
                            event = Event::Run(Job::Resolve {
                                path: self.path.clone(),
                                content,
                            });
                        }
                        for side in [Side::Theirs, Side::Ours] {
                            let label = format!("Take all {}", side_name(side));
                            if ui
                                .add(plain_button(&label))
                                .on_hover_text(format!(
                                    "git checkout --{} -- {}",
                                    side_name(side),
                                    self.path
                                ))
                                .clicked()
                            {
                                event = self.take(side);
                            }
                        }
                        if total > 0 {
                            widgets::segmented(
                                ui,
                                &mut self.mode,
                                &[(Mode::Conflicts, "Conflicts"), (Mode::Result, "Result")],
                            );
                        }
                        if total > 1 {
                            if arrow_button(ui, false, "Next conflict").clicked() {
                                self.current = (self.current + 1) % total;
                                self.focus = Some(self.current);
                                self.mode = Mode::Conflicts;
                            }
                            if arrow_button(ui, true, "Previous conflict").clicked() {
                                self.current = (self.current + total - 1) % total;
                                self.focus = Some(self.current);
                                self.mode = Mode::Conflicts;
                            }
                        }
                    });
                });
            });
        let rect = ui.min_rect();
        ui.painter().hline(
            rect.x_range(),
            rect.bottom(),
            Stroke::new(1.0, theme::BORDER),
        );
        event
    }

    fn conflicts(&mut self, ui: &mut Ui) {
        let Body::Text(parsed) = &self.body else {
            return;
        };
        let regions = parsed.regions.clone();
        let last = regions.len().saturating_sub(1);
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(24, 18))
                    .show(ui, |ui| {
                        ui.set_max_width(ui.available_width().min(CARD_W));
                        ui.spacing_mut().item_spacing.y = 10.0;
                        if regions.iter().all(|r| matches!(r, Region::Clean(_))) {
                            ui.label(
                                RichText::new(
                                    "No conflict markers left in this file. Mark it resolved \
                                     to stage it.",
                                )
                                .color(theme::TEXT_MUTED),
                            );
                        }
                        let mut index = 0;
                        for (i, region) in regions.iter().enumerate() {
                            match region {
                                Region::Clean(text) => context(ui, text, i == 0, i == last),
                                Region::Conflict(c) => {
                                    let rect = self.card(ui, index, c);
                                    if self.focus == Some(index) {
                                        ui.scroll_to_rect(rect, Some(Align::TOP));
                                        self.focus = None;
                                    }
                                    index += 1;
                                }
                            }
                        }
                    });
            });
    }

    fn card(&mut self, ui: &mut Ui, index: usize, c: &conflict::Conflict) -> Rect {
        let total = self.choices.len();
        let choice = self.choices[index];
        let current = self.current == index;
        let border = if current {
            theme::with_alpha(theme::ACCENT, 0x88)
        } else {
            theme::BORDER
        };
        egui::Frame::new()
            .fill(theme::PANEL)
            .stroke(Stroke::new(1.0, border))
            .corner_radius(8)
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Conflict {} of {total}", index + 1))
                            .size(12.0)
                            .family(theme::semibold())
                            .color(theme::TEXT_STRONG),
                    );
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        let mut picked = choice;
                        widgets::segmented(
                            ui,
                            &mut picked,
                            &[
                                (Some(Choice::Ours), "Use ours"),
                                (Some(Choice::Theirs), "Use theirs"),
                                (Some(Choice::Both), "Use both"),
                            ],
                        );
                        if picked != choice {
                            self.choices[index] = picked;
                            self.current = index;
                            if let Some(next) = self.choices.iter().position(Option::is_none) {
                                self.current = next;
                            }
                        }
                    });
                });
                let column_w = (ui.available_width() - COLUMN_GAP) / 2.0;
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = COLUMN_GAP;
                    let keeps = |side: Choice| match choice {
                        None => None,
                        Some(Choice::Both) => Some(true),
                        Some(picked) => Some(picked == side),
                    };
                    column(
                        ui,
                        column_w,
                        "Ours",
                        &c.ours_label,
                        &c.ours,
                        keeps(Choice::Ours),
                    );
                    column(
                        ui,
                        column_w,
                        "Theirs",
                        &c.theirs_label,
                        &c.theirs,
                        keeps(Choice::Theirs),
                    );
                });
                if let Some(base) = &c.base {
                    let open = self.open_bases.contains(&index);
                    let lines = base.lines().count();
                    let toggle = ui
                        .add(
                            egui::Label::new(
                                RichText::new(format!(
                                    "{}  Common ancestor · {lines} line{}",
                                    if open { "▾" } else { "▸" },
                                    if lines == 1 { "" } else { "s" }
                                ))
                                .size(12.0)
                                .color(theme::TEXT_MUTED),
                            )
                            .sense(Sense::click()),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand);
                    if toggle.clicked() {
                        if open {
                            self.open_bases.remove(&index);
                        } else {
                            self.open_bases.insert(index);
                        }
                    }
                    if open {
                        code_block(ui, base, PENDING_BG, CONTEXT_TEXT);
                    }
                }
            })
            .response
            .rect
    }

    fn result(&self, ui: &mut Ui) {
        let Body::Text(parsed) = &self.body else {
            return;
        };
        let text = parsed.render(&self.choices);
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(Margin::symmetric(24, 18))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for line in text.lines() {
                        let is_marker = ["<<<<<<<", "|||||||", "=======", ">>>>>>>"]
                            .iter()
                            .any(|m| line.starts_with(m));
                        let color = if is_marker {
                            theme::MODIFIED
                        } else {
                            theme::TEXT
                        };
                        ui.label(
                            RichText::new(if line.is_empty() { " " } else { line })
                                .font(FontId::monospace(12.5))
                                .color(color),
                        );
                    }
                    if self.resolved().is_none() {
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new("Conflicts without a pick still show their markers.")
                                .size(12.0)
                                .color(theme::TEXT_FAINT),
                        );
                    }
                });
        });
    }
}

fn column(ui: &mut Ui, width: f32, side: &str, label: &str, text: &str, kept: Option<bool>) {
    ui.vertical(|ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = 4.0;
        let title = if label.is_empty() {
            side.to_string()
        } else {
            format!("{side} · {label}")
        };
        let title_color = match kept {
            Some(true) => theme::ADDED,
            Some(false) => theme::TEXT_FAINT,
            None => theme::TEXT_MUTED,
        };
        ui.label(RichText::new(title).size(12.0).color(title_color));
        let (fill, ink) = match kept {
            Some(true) => (KEPT_BG, theme::TEXT),
            Some(false) => (DROPPED_BG, theme::TEXT_FAINT),
            None => (PENDING_BG, theme::TEXT),
        };
        if text.is_empty() {
            code_block(ui, "(no lines)", fill, theme::TEXT_FAINT);
        } else {
            code_block(ui, text, fill, ink);
        }
    });
}

fn code_block(ui: &mut Ui, text: &str, fill: Color32, ink: Color32) {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                egui::Label::new(
                    RichText::new(text.trim_end_matches(['\n', '\r']))
                        .font(FontId::monospace(12.5))
                        .color(ink),
                )
                .wrap(),
            );
        });
}

fn context(ui: &mut Ui, text: &str, at_start: bool, at_end: bool) {
    let lines: Vec<&str> = text.lines().collect();
    let keep_head = if at_start { 0 } else { CONTEXT_EDGE };
    let keep_tail = if at_end { 0 } else { CONTEXT_EDGE };
    let shown: Vec<String> = if lines.len() <= keep_head + keep_tail + 1 {
        lines.iter().map(|l| l.to_string()).collect()
    } else {
        let hidden = lines.len() - keep_head - keep_tail;
        let mut out: Vec<String> = lines[..keep_head].iter().map(|l| l.to_string()).collect();
        out.push(format!(
            "⋯ {hidden} unchanged line{}",
            if hidden == 1 { "" } else { "s" }
        ));
        out.extend(
            lines[lines.len() - keep_tail..]
                .iter()
                .map(|l| l.to_string()),
        );
        out
    };
    if shown.is_empty() {
        return;
    }
    ui.add(
        egui::Label::new(
            RichText::new(shown.join("\n"))
                .font(FontId::monospace(12.5))
                .color(CONTEXT_TEXT),
        )
        .wrap(),
    );
}

fn whole_file_notice(ui: &mut Ui, note: &str, stages: &Stages) -> Option<Side> {
    let mut picked = None;
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() / 3.0);
        ui.label(RichText::new(note).color(theme::TEXT_MUTED));
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let width = 290.0;
            ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
            for (side, present) in [(Side::Ours, stages.ours), (Side::Theirs, stages.theirs)] {
                let label = if present {
                    format!("Keep {}", side_name(side))
                } else {
                    format!("Delete, like {}", side_name(side))
                };
                if ui.add(plain_button(&label)).clicked() {
                    picked = Some(side);
                }
            }
        });
    });
    picked
}

pub struct Banner {
    confirm_abort: bool,
}

impl Banner {
    pub fn new() -> Self {
        Self {
            confirm_abort: false,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, op: &InProgress, conflicted: usize) -> Option<Job> {
        let mut job = None;
        let operation = op.operation;
        egui::Frame::new()
            .fill(theme::with_alpha(theme::MODIFIED, 0x1c))
            .inner_margin(Margin::symmetric(16, 0))
            .show(ui, |ui| {
                ui.set_height(BANNER_H);
                ui.set_width(ui.available_width());
                ui.horizontal_centered(|ui| {
                    ui.label(
                        RichText::new(op.title())
                            .family(theme::semibold())
                            .color(theme::TEXT_STRONG),
                    );
                    let (note, color) = match conflicted {
                        0 if op.editing.is_some() => {
                            ("amend, then Continue".to_string(), theme::TEXT_MUTED)
                        }
                        0 => ("all conflicts resolved".to_string(), theme::ADDED),
                        1 => ("1 conflicted file".to_string(), theme::MODIFIED),
                        n => (format!("{n} conflicted files"), theme::MODIFIED),
                    };
                    ui.label(RichText::new(format!("·  {note}")).color(color));
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        let hint = conflict::command_line(operation, Step::Continue);
                        if ui
                            .add_enabled(conflicted == 0, accent_button("Continue"))
                            .on_hover_text(hint)
                            .on_disabled_hover_text("Resolve every conflicted file first")
                            .clicked()
                        {
                            job = Some(Job::Step(operation, Step::Continue));
                        }
                        if operation.can_skip()
                            && ui
                                .add(plain_button("Skip"))
                                .on_hover_text(conflict::command_line(operation, Step::Skip))
                                .clicked()
                        {
                            job = Some(Job::Step(operation, Step::Skip));
                        }
                        if ui
                            .add(danger_button("Abort"))
                            .on_hover_text(conflict::command_line(operation, Step::Abort))
                            .clicked()
                        {
                            self.confirm_abort = true;
                        }
                    });
                });
            });
        if self.confirm_abort && self.confirm(ui.ctx(), op) {
            job = Some(Job::Step(operation, Step::Abort));
        }
        job
    }

    fn confirm(&mut self, ctx: &egui::Context, op: &InProgress) -> bool {
        let mut abort = false;
        let modal = egui::Modal::new(egui::Id::new("kelp-abort-operation"))
            .frame(
                egui::Frame::new()
                    .fill(theme::POPUP)
                    .stroke(Stroke::new(1.0, theme::POPUP_BORDER))
                    .corner_radius(10)
                    .inner_margin(Margin::same(22)),
            )
            .show(ctx, |ui| {
                ui.set_width(440.0);
                ui.spacing_mut().item_spacing.y = 12.0;
                let noun = op.title().replace(" in progress", "");
                ui.label(
                    RichText::new(format!("Abort the {}?", noun.to_lowercase()))
                        .size(16.0)
                        .family(theme::semibold())
                        .color(theme::TEXT_STRONG),
                );
                ui.label(
                    RichText::new(
                        "Your branch goes back to where it was before it started. Conflicts \
                         you already resolved are thrown away.",
                    )
                    .color(theme::TEXT_MUTED),
                );
                ui.label(
                    RichText::new(conflict::command_line(op.operation, Step::Abort))
                        .monospace()
                        .color(theme::TEXT_FAINT),
                );
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(danger_fill_button("Abort")).clicked() {
                            abort = true;
                            self.confirm_abort = false;
                        }
                        if ui.add(plain_button("Cancel")).clicked() {
                            self.confirm_abort = false;
                        }
                    });
                });
            });
        if modal.should_close() {
            self.confirm_abort = false;
        }
        abort
    }
}

fn arrow_button(ui: &mut Ui, up: bool, hint: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    let response = response
        .on_hover_text(hint)
        .on_hover_cursor(CursorIcon::PointingHand);
    let color = if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), theme::CONTROL_HOVER);
        theme::TEXT_STRONG
    } else {
        theme::TEXT_MUTED
    };
    let c = rect.center();
    let d = if up { -3.0 } else { 3.0 };
    let stroke = Stroke::new(1.5, color);
    ui.painter()
        .line_segment([c + vec2(-5.0, -d), c + vec2(0.0, d)], stroke);
    ui.painter()
        .line_segment([c + vec2(0.0, d), c + vec2(5.0, -d)], stroke);
    response
}

fn plain_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).size(12.0))
        .corner_radius(5)
        .min_size(vec2(0.0, 30.0))
}

fn danger_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).size(12.0).color(theme::DELETED))
        .corner_radius(5)
        .min_size(vec2(0.0, 30.0))
}

fn danger_fill_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(
        RichText::new(text)
            .size(12.0)
            .family(theme::semibold())
            .color(Color32::from_rgb(0x1a, 0x10, 0x12)),
    )
    .fill(theme::DELETED)
    .corner_radius(5)
    .min_size(vec2(0.0, 30.0))
}

fn accent_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(
        RichText::new(text)
            .size(12.0)
            .family(theme::semibold())
            .color(Color32::from_rgb(0x10, 0x13, 0x1a)),
    )
    .fill(theme::ACCENT)
    .corner_radius(5)
    .min_size(vec2(0.0, 30.0))
}
