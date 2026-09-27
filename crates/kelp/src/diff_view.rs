use std::collections::HashMap;
use std::path::Path;

use eframe::egui::{
    self, Align2, Color32, FontId, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2,
};
use gix::ObjectId;
use kelp_core::diff::{self, Body, FileDiff, Line, LineKind};
use kelp_core::ops::Op;
use kelp_core::review::{Anchor, Comment, Review, Side, Thread};

use crate::theme;

const LINE_H: f32 = 22.0;
const HUNK_H: f32 = 28.0;
const NUM_W: f32 = 46.0;
const GUTTER_W: f32 = 24.0;
const HEADER_H: f32 = 48.0;
const THREAD_INDENT: f32 = NUM_W * 2.0 + GUTTER_W + 10.0;
const THREAD_W: f32 = 640.0;
const RESOLVED_H: f32 = 34.0;
const COMPOSER_H: f32 = 132.0;
const ADDED_BG: Color32 = Color32::from_rgb(0x16, 0x30, 0x2a);
const REMOVED_BG: Color32 = Color32::from_rgb(0x3a, 0x1f, 0x22);
const NUM_COLOR: Color32 = Color32::from_rgb(0x5e, 0x65, 0x73);
const CONTEXT_TEXT: Color32 = Color32::from_rgb(0xb4, 0xb9, 0xc2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSource {
    Commit(ObjectId),
    Unstaged,
    Staged,
    File(ObjectId),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Diff,
    File,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layout {
    Unified,
    Split,
}

#[derive(Clone, Copy)]
enum Item {
    Hunk(usize),
    Line(usize),
    Split(Option<usize>, Option<usize>),
    FileLine(usize),
    Thread(u64),
    Composer,
    OutsideHeader,
}

pub struct DiffView {
    pub source: DiffSource,
    pub diff: FileDiff,
    hunks: Vec<String>,
    lines: Vec<(usize, Line)>,
    old_lines: Vec<String>,
    new_lines: Vec<String>,
    mode: Mode,
    layout: Layout,
    composer: Option<(Side, u32)>,
    draft: String,
    replies: HashMap<u64, String>,
    expanded: HashMap<u64, bool>,
    pending: Option<Op>,
}

pub enum Event {
    None,
    Close,
    Changed,
    Run(Op),
}

impl DiffView {
    pub fn load(
        repo: &gix::Repository,
        workdir: Option<&Path>,
        source: DiffSource,
        path: &str,
    ) -> anyhow::Result<Self> {
        let diff = match (source, workdir) {
            (DiffSource::Commit(id), _) => diff::commit_file(repo, id, path)?,
            (DiffSource::Unstaged, Some(workdir)) => diff::unstaged_file(repo, workdir, path)?,
            (DiffSource::Staged, Some(_)) => diff::staged_file(repo, path)?,
            (DiffSource::Unstaged | DiffSource::Staged, None) => {
                anyhow::bail!("this repository has no working tree")
            }
            (DiffSource::File(id), _) => {
                let content = diff::file_at(repo, id, path)?;
                diff::build(path, content.as_deref(), content.as_deref())
            }
        };
        let mut hunks = Vec::new();
        let mut lines = Vec::new();
        if let Body::Text(list) = &diff.body {
            for (h, hunk) in list.iter().enumerate() {
                hunks.push(hunk.header.clone());
                lines.extend(hunk.lines.iter().cloned().map(|l| (h, l)));
            }
        }
        let split = |t: &Option<String>| {
            t.as_deref()
                .map(|t| t.lines().map(|l| l.replace('\t', "    ")).collect())
                .unwrap_or_default()
        };
        let mode = if matches!(source, DiffSource::File(_)) {
            Mode::File
        } else {
            Mode::Diff
        };
        Ok(Self {
            source,
            old_lines: split(&diff.old_text),
            new_lines: split(&diff.new_text),
            diff,
            hunks,
            lines,
            mode,
            layout: Layout::Unified,
            composer: None,
            draft: String::new(),
            replies: HashMap::new(),
            expanded: HashMap::new(),
            pending: None,
        })
    }

    pub fn path(&self) -> &str {
        &self.diff.path
    }

    pub fn is_staged(&self) -> bool {
        self.source == DiffSource::Staged
    }

    pub fn is_working(&self) -> bool {
        matches!(self.source, DiffSource::Unstaged | DiffSource::Staged)
    }

    pub fn reload(&mut self, repo: &gix::Repository, workdir: Option<&Path>) -> anyhow::Result<()> {
        let fresh = Self::load(repo, workdir, self.source, &self.diff.path.clone())?;
        let (mode, layout) = (self.mode, self.layout);
        *self = Self {
            mode,
            layout,
            ..fresh
        };
        Ok(())
    }

    fn hunk_action(&self) -> Option<&'static str> {
        let whole_file = self.diff.old_text.as_deref().is_none_or(str::is_empty)
            || self.diff.new_text.as_deref().is_none_or(str::is_empty);
        match self.source {
            _ if whole_file => None,
            DiffSource::Unstaged => Some("Stage hunk"),
            DiffSource::Staged => Some("Unstage hunk"),
            _ => None,
        }
    }

    pub fn show_split(&mut self) {
        self.layout = Layout::Split;
    }

    fn commit(&self) -> Option<String> {
        match self.source {
            DiffSource::Commit(id) | DiffSource::File(id) => Some(id.to_string()),
            DiffSource::Unstaged | DiffSource::Staged => None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, review: &mut Review, author: &str) -> Event {
        if let Some(text) = &self.diff.new_text {
            crate::fonts::ensure_fallback(ui.ctx(), text);
        }
        let mut event = self.header(ui, review);
        match &self.diff.body {
            Body::Binary => notice(ui, "Binary file, no text to show."),
            Body::TooLarge => notice(ui, "This file is too large to show here."),
            Body::Text(_) if self.mode == Mode::Diff && self.lines.is_empty() => {
                notice(ui, "No changes in this file.")
            }
            Body::Text(_) => {
                if self.body(ui, review, author) {
                    event = Event::Changed;
                }
                if let Some(op) = self.pending.take() {
                    event = Event::Run(op);
                }
            }
        }
        event
    }

    fn header(&mut self, ui: &mut Ui, review: &Review) -> Event {
        let mut event = Event::None;
        egui::Frame::new()
            .fill(theme::PANEL)
            .inner_margin(Margin::symmetric(16, 0))
            .show(ui, |ui| {
                ui.set_height(HEADER_H);
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    if ui
                        .add(
                            egui::Button::new(RichText::new("‹  Graph").size(12.0))
                                .corner_radius(5),
                        )
                        .on_hover_text("Back to the graph (Esc)")
                        .clicked()
                    {
                        event = Event::Close;
                    }
                    let path = self.path().to_string();
                    let (dir, name) = path
                        .rsplit_once('/')
                        .map_or(("", path.as_str()), |(d, n)| (d, n));
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
                    if self.mode == Mode::Diff {
                        ui.label(
                            RichText::new(format!("+{}", self.diff.added))
                                .size(12.0)
                                .color(theme::ADDED),
                        );
                        ui.label(
                            RichText::new(format!("−{}", self.diff.removed))
                                .size(12.0)
                                .color(theme::DELETED),
                        );
                    }
                    let origin = match self.source {
                        DiffSource::Commit(id) | DiffSource::File(id) => {
                            format!("in {}", id.to_hex_with_len(7))
                        }
                        DiffSource::Unstaged => "unstaged".into(),
                        DiffSource::Staged => "staged".into(),
                    };
                    ui.label(RichText::new(origin).size(12.0).color(theme::TEXT_FAINT));
                    let comments = review.count_for(self.path());
                    if comments > 0 {
                        ui.label(
                            RichText::new(format!(
                                "{comments} comment{}",
                                if comments == 1 { "" } else { "s" }
                            ))
                            .size(12.0)
                            .color(theme::ACCENT),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.mode == Mode::Diff {
                            segmented(
                                ui,
                                &mut self.layout,
                                &[(Layout::Split, "Split"), (Layout::Unified, "Unified")],
                            );
                        }
                        if !matches!(self.source, DiffSource::File(_)) {
                            segmented(
                                ui,
                                &mut self.mode,
                                &[(Mode::File, "File"), (Mode::Diff, "Diff")],
                            );
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

    fn items(&self, review: &Review) -> Vec<Item> {
        let threads: Vec<&Thread> = review.for_path(self.path()).collect();
        let mut placed: HashMap<(Side, u32), Vec<u64>> = HashMap::new();
        let mut outside = Vec::new();
        let old: Vec<&str> = self.old_lines.iter().map(String::as_str).collect();
        let new: Vec<&str> = self.new_lines.iter().map(String::as_str).collect();
        for thread in &threads {
            let text = if thread.anchor.side == Side::Old {
                &old
            } else {
                &new
            };
            match thread.anchor.locate(text) {
                Some(i) => placed
                    .entry((thread.anchor.side, i as u32 + 1))
                    .or_default()
                    .push(thread.id),
                None => outside.push(thread.id),
            }
        }
        let mut items = Vec::new();
        let attach = |items: &mut Vec<Item>, key: (Side, u32), shown: &mut Vec<(Side, u32)>| {
            if let Some(ids) = placed.get(&key) {
                items.extend(ids.iter().map(|id| Item::Thread(*id)));
            }
            if self.composer == Some(key) {
                items.push(Item::Composer);
            }
            shown.push(key);
        };
        let mut shown = Vec::new();
        match self.mode {
            Mode::File => {
                for i in 0..self.new_lines.len() {
                    items.push(Item::FileLine(i));
                    attach(&mut items, (Side::New, i as u32 + 1), &mut shown);
                }
            }
            Mode::Diff => {
                let mut current_hunk = usize::MAX;
                let mut i = 0;
                while i < self.lines.len() {
                    let (hunk, line) = &self.lines[i];
                    if *hunk != current_hunk {
                        current_hunk = *hunk;
                        items.push(Item::Hunk(*hunk));
                    }
                    match self.layout {
                        Layout::Unified => {
                            items.push(Item::Line(i));
                            if let Some(key) = key_of(line) {
                                attach(&mut items, key, &mut shown);
                            }
                            i += 1;
                        }
                        Layout::Split => {
                            let block_end = |from: usize, kind: LineKind| {
                                let mut j = from;
                                while j < self.lines.len()
                                    && self.lines[j].1.kind == kind
                                    && self.lines[j].0 == *hunk
                                {
                                    j += 1;
                                }
                                j
                            };
                            if line.kind == LineKind::Context {
                                items.push(Item::Split(Some(i), Some(i)));
                                if let Some(key) = key_of(line) {
                                    attach(&mut items, key, &mut shown);
                                }
                                i += 1;
                                continue;
                            }
                            let removed_end = block_end(i, LineKind::Removed);
                            let added_end = block_end(removed_end, LineKind::Added);
                            let removed: Vec<usize> = (i..removed_end).collect();
                            let added: Vec<usize> = (removed_end..added_end).collect();
                            for k in 0..removed.len().max(added.len()) {
                                let (l, r) = (removed.get(k).copied(), added.get(k).copied());
                                items.push(Item::Split(l, r));
                                for side in [l, r].into_iter().flatten() {
                                    if let Some(key) = key_of(&self.lines[side].1) {
                                        attach(&mut items, key, &mut shown);
                                    }
                                }
                            }
                            i = added_end.max(i + 1);
                        }
                    }
                }
            }
        }
        let hidden: Vec<u64> = placed
            .iter()
            .filter(|(key, _)| !shown.contains(key))
            .flat_map(|(_, ids)| ids.iter().copied())
            .chain(outside)
            .collect();
        if !hidden.is_empty() {
            let mut top = vec![Item::OutsideHeader];
            top.extend(hidden.into_iter().map(Item::Thread));
            top.extend(items);
            items = top;
        }
        items
    }

    fn height(&self, ui: &Ui, item: &Item, review: &Review) -> f32 {
        match item {
            Item::Hunk(_) | Item::OutsideHeader => HUNK_H,
            Item::Line(_) | Item::Split(..) | Item::FileLine(_) => LINE_H,
            Item::Composer => COMPOSER_H,
            Item::Thread(id) => {
                let Some(thread) = review.threads.iter().find(|t| t.id == *id) else {
                    return 0.0;
                };
                if thread.resolved && !self.expanded.get(id).copied().unwrap_or(false) {
                    return RESOLVED_H;
                }
                let body_w = THREAD_W - 64.0;
                let comments: f32 = thread
                    .comments
                    .iter()
                    .map(|c| {
                        let g = ui.painter().layout(
                            c.body.clone(),
                            FontId::proportional(13.0),
                            theme::TEXT,
                            body_w,
                        );
                        26.0 + g.size().y + 14.0
                    })
                    .sum();
                20.0 + comments + 92.0
            }
        }
    }

    fn body(&mut self, ui: &mut Ui, review: &mut Review, author: &str) -> bool {
        let items = self.items(review);
        let heights: Vec<f32> = items.iter().map(|i| self.height(ui, i, review)).collect();
        let mut offsets = Vec::with_capacity(items.len() + 1);
        let mut y = 0.0;
        for h in &heights {
            offsets.push(y);
            y += h;
        }
        let total = y;
        let mut changed = false;
        let font = FontId::monospace(12.5);
        egui::ScrollArea::both()
            .auto_shrink(false)
            .show_viewport(ui, |ui, viewport| {
                let width = ui.available_width().max(
                    if self.layout == Layout::Split && self.mode == Mode::Diff {
                        900.0
                    } else {
                        1100.0
                    },
                );
                let (full, _) = ui.allocate_exact_size(vec2(width, total), Sense::hover());
                let origin = full.min;
                let first = offsets.partition_point(|o| *o + LINE_H * 4.0 < viewport.min.y);
                for (k, item) in items.iter().enumerate().skip(first) {
                    let top = offsets[k];
                    if top > viewport.max.y {
                        break;
                    }
                    let rect =
                        Rect::from_min_size(origin + vec2(0.0, top), vec2(width, heights[k]));
                    match *item {
                        Item::Hunk(h) => {
                            ui.painter().rect_filled(
                                rect,
                                0.0,
                                Color32::from_rgb(0x1a, 0x22, 0x30),
                            );
                            ui.painter().text(
                                pos2(rect.left() + 16.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                &self.hunks[h],
                                font.clone(),
                                Color32::from_rgb(0x8f, 0xb4, 0xe8),
                            );
                            if let Some(label) = self.hunk_action() {
                                let visible_right = rect.left() + viewport.max.x - viewport.min.x;
                                let button_rect = Rect::from_min_size(
                                    pos2(visible_right.min(rect.right()) - 130.0, rect.top() + 3.0),
                                    vec2(118.0, rect.height() - 6.0),
                                );
                                let button = egui::Button::new(RichText::new(label).size(12.0))
                                    .corner_radius(5)
                                    .fill(Color32::from_rgb(0x24, 0x2c, 0x3a));
                                if ui.put(button_rect, button).clicked()
                                    && let Some(patch) = diff::hunk_patch(&self.diff, h)
                                {
                                    let reverse = self.source == DiffSource::Staged;
                                    self.pending = Some(Op::ApplyToIndex { patch, reverse });
                                }
                            }
                        }
                        Item::OutsideHeader => {
                            ui.painter().rect_filled(
                                rect,
                                0.0,
                                Color32::from_rgb(0x1f, 0x23, 0x2b),
                            );
                            ui.painter().text(
                                pos2(rect.left() + 16.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                "Comments on lines not shown here",
                                FontId::proportional(12.0),
                                theme::TEXT_MUTED,
                            );
                        }
                        Item::Line(i) => {
                            let line = self.lines[i].1.clone();
                            self.unified_line(ui, rect, &line, &font);
                        }
                        Item::Split(l, r) => {
                            let half = rect.width() / 2.0;
                            let left = Rect::from_min_size(rect.min, vec2(half, rect.height()));
                            let right = Rect::from_min_size(
                                rect.min + vec2(half, 0.0),
                                vec2(half, rect.height()),
                            );
                            let (ll, rl) = (
                                l.map(|i| self.lines[i].1.clone()),
                                r.map(|i| self.lines[i].1.clone()),
                            );
                            self.split_side(ui, left, ll.as_ref(), Side::Old, &font);
                            self.split_side(ui, right, rl.as_ref(), Side::New, &font);
                            ui.painter().vline(
                                right.left(),
                                rect.y_range(),
                                Stroke::new(1.0, theme::BORDER),
                            );
                        }
                        Item::FileLine(i) => {
                            let text = self.new_lines[i].clone();
                            let number = i as u32 + 1;
                            self.code_row(
                                ui,
                                rect,
                                Numbers::One(Some(number)),
                                "",
                                &text,
                                Color32::TRANSPARENT,
                                theme::TEXT_FAINT,
                                CONTEXT_TEXT,
                                Side::New,
                                &font,
                            );
                        }
                        Item::Thread(id) => {
                            if self.thread(ui, rect, id, review, author) {
                                changed = true;
                            }
                        }
                        Item::Composer => {
                            if self.composer_ui(ui, rect, review, author) {
                                changed = true;
                            }
                        }
                    }
                }
            });
        changed
    }

    fn unified_line(&mut self, ui: &mut Ui, rect: Rect, line: &Line, font: &FontId) {
        let (bg, mark, mark_color) = style(line.kind);
        let side = if line.kind == LineKind::Removed {
            Side::Old
        } else {
            Side::New
        };
        let text_color = if line.kind == LineKind::Context {
            CONTEXT_TEXT
        } else {
            theme::TEXT
        };
        self.code_row(
            ui,
            rect,
            Numbers::Both(line.old, line.new),
            mark,
            &line.text,
            bg,
            mark_color,
            text_color,
            side,
            font,
        );
    }

    fn split_side(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        line: Option<&Line>,
        side: Side,
        font: &FontId,
    ) {
        let Some(line) = line else {
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_rgb(0x18, 0x1b, 0x21));
            return;
        };
        let (bg, mark, mark_color) = style(line.kind);
        let number = if side == Side::Old {
            line.old
        } else {
            line.new
        };
        let text_color = if line.kind == LineKind::Context {
            CONTEXT_TEXT
        } else {
            theme::TEXT
        };
        self.code_row(
            ui,
            rect,
            Numbers::One(number),
            mark,
            &line.text,
            bg,
            mark_color,
            text_color,
            side,
            font,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn code_row(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        numbers: Numbers,
        mark: &str,
        text: &str,
        bg: Color32,
        mark_color: Color32,
        text_color: Color32,
        side: Side,
        font: &FontId,
    ) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, bg);
        let mid = rect.center().y;
        let num = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
        let (numbers_w, line_no) = match numbers {
            Numbers::Both(old, new) => {
                painter.text(
                    pos2(rect.left() + NUM_W - 8.0, mid),
                    Align2::RIGHT_CENTER,
                    num(old),
                    font.clone(),
                    NUM_COLOR,
                );
                painter.text(
                    pos2(rect.left() + NUM_W * 2.0 - 8.0, mid),
                    Align2::RIGHT_CENTER,
                    num(new),
                    font.clone(),
                    NUM_COLOR,
                );
                (NUM_W * 2.0, if side == Side::Old { old } else { new })
            }
            Numbers::One(n) => {
                painter.text(
                    pos2(rect.left() + NUM_W - 8.0, mid),
                    Align2::RIGHT_CENTER,
                    num(n),
                    font.clone(),
                    NUM_COLOR,
                );
                (NUM_W, n)
            }
        };
        let gutter = Rect::from_min_size(
            pos2(rect.left() + numbers_w, rect.top()),
            vec2(GUTTER_W, rect.height()),
        );
        if let Some(line_no) = line_no
            && ui.rect_contains_pointer(rect)
        {
            let button = Rect::from_center_size(gutter.center(), vec2(18.0, 18.0));
            let response = ui.interact(
                button,
                ui.id()
                    .with(("comment", side as u8, line_no, rect.left() as i32)),
                Sense::click(),
            );
            painter.rect_filled(
                button,
                4.0,
                if response.hovered() {
                    theme::TEXT_STRONG
                } else {
                    theme::ACCENT
                },
            );
            painter.text(
                button.center(),
                Align2::CENTER_CENTER,
                "+",
                FontId::proportional(14.0),
                Color32::from_rgb(0x10, 0x13, 0x1a),
            );
            if response.on_hover_text("Add a comment").clicked() {
                self.composer = Some((side, line_no));
                self.draft.clear();
            }
        }
        painter.text(
            pos2(gutter.right() + 4.0, mid),
            Align2::LEFT_CENTER,
            mark,
            font.clone(),
            mark_color,
        );
        painter.text(
            pos2(gutter.right() + 20.0, mid),
            Align2::LEFT_CENTER,
            text,
            font.clone(),
            text_color,
        );
    }

    fn thread(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        id: u64,
        review: &mut Review,
        author: &str,
    ) -> bool {
        let Some(thread) = review.threads.iter().find(|t| t.id == id).cloned() else {
            return false;
        };
        let mut changed = false;
        let expanded = self.expanded.get(&id).copied().unwrap_or(false);
        let card = Rect::from_min_size(
            pos2(rect.left() + THREAD_INDENT, rect.top() + 6.0),
            vec2(THREAD_W, rect.height() - 12.0),
        );
        if thread.resolved && !expanded {
            let chip = Rect::from_min_size(card.min, vec2(220.0, 24.0));
            let response = ui.interact(chip, ui.id().with(("resolved", id)), Sense::click());
            ui.painter().rect(
                chip,
                12.0,
                Color32::from_rgb(0x1e, 0x24, 0x2e),
                Stroke::new(1.0, Color32::from_rgb(0x2c, 0x34, 0x42)),
                egui::StrokeKind::Inside,
            );
            let label = format!(
                "✔ {} resolved comment{} · line {}",
                thread.comments.len(),
                if thread.comments.len() == 1 { "" } else { "s" },
                thread.anchor.line_hint
            );
            ui.painter().text(
                pos2(chip.left() + 12.0, chip.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(12.0),
                theme::TEXT_MUTED,
            );
            if response.on_hover_text("Show").clicked() {
                self.expanded.insert(id, true);
            }
            return false;
        }
        ui.painter().rect(
            card,
            8.0,
            Color32::from_rgb(0x1e, 0x24, 0x2e),
            Stroke::new(1.0, Color32::from_rgb(0x33, 0x40, 0x55)),
            egui::StrokeKind::Inside,
        );
        let mut y = card.top() + 12.0;
        for comment in &thread.comments {
            let avatar = pos2(card.left() + 26.0, y + 12.0);
            let (fill, ink) = theme::generated_avatar(&comment.author);
            ui.painter().circle(
                avatar,
                13.0,
                fill,
                Stroke::new(1.5, theme::with_alpha(ink, 0x66)),
            );
            ui.painter().text(
                avatar,
                Align2::CENTER_CENTER,
                kelp_core::avatar::initials(&comment.author),
                FontId::new(10.0, theme::semibold()),
                ink,
            );
            let header = ui.painter().text(
                pos2(card.left() + 50.0, y + 4.0),
                Align2::LEFT_TOP,
                &comment.author,
                FontId::proportional(13.0),
                theme::TEXT_STRONG,
            );
            let when = kelp_core::commit::relative_time(comment.time, now());
            ui.painter().text(
                pos2(header.right() + 8.0, y + 5.0),
                Align2::LEFT_TOP,
                when,
                FontId::proportional(12.0),
                theme::TEXT_FAINT,
            );
            let body = ui.painter().layout(
                comment.body.clone(),
                FontId::proportional(13.0),
                Color32::from_rgb(0xd5, 0xd7, 0xdc),
                THREAD_W - 64.0,
            );
            let body_h = body.size().y;
            ui.painter().galley(
                pos2(card.left() + 50.0, y + 24.0),
                body,
                Color32::from_rgb(0xd5, 0xd7, 0xdc),
            );
            y += 26.0 + body_h + 14.0;
        }
        let area = Rect::from_min_max(
            pos2(card.left() + 14.0, y),
            pos2(card.right() - 14.0, card.bottom() - 10.0),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(area)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.spacing_mut().item_spacing.y = 8.0;
        let draft = self.replies.entry(id).or_default();
        child.add(
            egui::TextEdit::multiline(draft)
                .hint_text("Reply…  (⌘ Enter to send)")
                .desired_rows(1)
                .desired_width(f32::INFINITY),
        );
        let send_shortcut = child.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter))
            && !draft.trim().is_empty();
        child.horizontal(|ui| {
            ui.label(
                RichText::new("Saved in this repo only · not pushed")
                    .size(11.0)
                    .color(theme::TEXT_FAINT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_send = !self.replies.get(&id).is_some_and(|d| d.trim().is_empty());
                let reply = egui::Button::new(
                    RichText::new("Reply")
                        .family(theme::semibold())
                        .color(Color32::from_rgb(0x10, 0x13, 0x1a)),
                )
                .fill(theme::ACCENT)
                .corner_radius(5);
                if (ui.add_enabled(can_send, reply).clicked() || send_shortcut) && can_send {
                    let body = self.replies.remove(&id).unwrap_or_default();
                    review.reply(
                        id,
                        Comment {
                            author: author.to_string(),
                            time: now(),
                            body: body.trim().to_string(),
                        },
                    );
                    changed = true;
                }
                let resolve_label = if thread.resolved { "Reopen" } else { "Resolve" };
                if ui
                    .add(egui::Button::new(resolve_label).corner_radius(5))
                    .clicked()
                {
                    review.set_resolved(id, !thread.resolved);
                    self.expanded.remove(&id);
                    changed = true;
                }
                if ui
                    .add(
                        egui::Button::new(RichText::new("Delete").color(theme::DELETED))
                            .frame(false),
                    )
                    .clicked()
                {
                    review.delete(id);
                    changed = true;
                }
            });
        });
        changed
    }

    fn composer_ui(&mut self, ui: &mut Ui, rect: Rect, review: &mut Review, author: &str) -> bool {
        let Some((side, line_no)) = self.composer else {
            return false;
        };
        let card = Rect::from_min_size(
            pos2(rect.left() + THREAD_INDENT, rect.top() + 6.0),
            vec2(THREAD_W, rect.height() - 12.0),
        );
        ui.painter().rect(
            card,
            8.0,
            Color32::from_rgb(0x1e, 0x24, 0x2e),
            Stroke::new(1.0, theme::with_alpha(theme::ACCENT, 0x99)),
            egui::StrokeKind::Inside,
        );
        let area = card.shrink(12.0);
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(area)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.spacing_mut().item_spacing.y = 8.0;
        child.label(
            RichText::new(format!("Comment on line {line_no}"))
                .size(12.0)
                .color(theme::TEXT_MUTED),
        );
        let edit = child.add(
            egui::TextEdit::multiline(&mut self.draft)
                .hint_text("Write a comment…  (⌘ Enter to save)")
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
        if !edit.has_focus() && child.memory(|m| m.focused().is_none()) {
            edit.request_focus();
        }
        let save_shortcut = child.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
        let cancel_shortcut = child.input(|i| i.key_pressed(egui::Key::Escape));
        let mut changed = false;
        let mut close = cancel_shortcut;
        child.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let enabled = !self.draft.trim().is_empty();
                let save = egui::Button::new(
                    RichText::new("Comment")
                        .family(theme::semibold())
                        .color(Color32::from_rgb(0x10, 0x13, 0x1a)),
                )
                .fill(theme::ACCENT)
                .corner_radius(5);
                let clicked = ui.add_enabled(enabled, save).clicked();
                if enabled && (clicked || save_shortcut) {
                    let lines: Vec<&str> = if side == Side::Old {
                        &self.old_lines
                    } else {
                        &self.new_lines
                    }
                    .iter()
                    .map(String::as_str)
                    .collect();
                    let index = (line_no as usize)
                        .saturating_sub(1)
                        .min(lines.len().saturating_sub(1));
                    if !lines.is_empty() {
                        let anchor = Anchor::at(&lines, index, side);
                        let comment = Comment {
                            author: author.to_string(),
                            time: now(),
                            body: self.draft.trim().to_string(),
                        };
                        review.add(self.path(), self.commit(), anchor, comment);
                        changed = true;
                    }
                    close = true;
                }
                if ui
                    .add(egui::Button::new("Cancel").corner_radius(5))
                    .clicked()
                {
                    close = true;
                }
            });
        });
        if close {
            self.composer = None;
            self.draft.clear();
        }
        changed
    }
}

#[derive(Clone, Copy)]
enum Numbers {
    Both(Option<u32>, Option<u32>),
    One(Option<u32>),
}

fn key_of(line: &Line) -> Option<(Side, u32)> {
    match line.kind {
        LineKind::Removed => line.old.map(|n| (Side::Old, n)),
        _ => line.new.map(|n| (Side::New, n)),
    }
}

fn style(kind: LineKind) -> (Color32, &'static str, Color32) {
    match kind {
        LineKind::Added => (ADDED_BG, "+", theme::ADDED),
        LineKind::Removed => (REMOVED_BG, "-", theme::DELETED),
        LineKind::Context => (Color32::TRANSPARENT, "", theme::TEXT_FAINT),
    }
}

fn segmented<T: Copy + PartialEq>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) {
    egui::Frame::new()
        .fill(Color32::from_rgb(0x12, 0x15, 0x1a))
        .stroke(Stroke::new(1.0, Color32::from_rgb(0x2c, 0x31, 0x3b)))
        .corner_radius(6)
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (option, label) in options {
                let active = *value == *option;
                let button = egui::Button::new(RichText::new(*label).size(12.0).color(if active {
                    theme::TEXT_STRONG
                } else {
                    theme::TEXT_MUTED
                }))
                .fill(if active {
                    Color32::from_rgb(0x2b, 0x32, 0x40)
                } else {
                    Color32::TRANSPARENT
                })
                .corner_radius(4);
                if ui.add(button).clicked() {
                    *value = *option;
                }
            }
        });
}

fn notice(ui: &mut Ui, text: &str) {
    ui.centered_and_justified(|ui| ui.label(RichText::new(text).color(theme::TEXT_MUTED)));
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}
