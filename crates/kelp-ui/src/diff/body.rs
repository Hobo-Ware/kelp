use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};

use egui::{
    self, Align2, Color32, CursorIcon, FontId, Key, Margin, Modifiers, Rect, RichText, Sense,
    Stroke, Ui, pos2, vec2,
};
use kelp_core::diff::{Body, FileDiff, Line, LineKind};
use kelp_core::review::{Anchor, Comment, Side, Thread};

use self::composer::Composer;
use self::range::{Candidate, LineRange, anchor_label, head_at};
use super::style::{DiffStyle, LineLook, hunk_label};
use crate::theme;

mod composer;
mod note;
mod range;

pub const LINE_H: f32 = 22.0;
pub const HUNK_H: f32 = 28.0;
const NUM_W: f32 = 46.0;
const GUTTER_W: f32 = 24.0;
const THREAD_INDENT: f32 = NUM_W * 2.0 + GUTTER_W + 10.0;
const THREAD_W: f32 = 640.0;
const THREAD_MIN_W: f32 = 240.0;
const THREAD_MARGIN: f32 = 16.0;
const RESOLVED_H: f32 = 34.0;

/// Side-by-side or single-column diff.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, serde::Serialize, serde::Deserialize)]
pub enum DiffLayout {
    #[default]
    Unified,
    Split,
}

/// What the body lists: only the changed hunks, or the whole new file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DiffMode {
    Diff,
    File,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Jump {
    Previous,
    Next,
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

struct Rows {
    card_w: f32,
    items: Vec<Item>,
    heights: Vec<f32>,
    offsets: Vec<f32>,
    total: f32,
}

/// A button in a hunk header. Buttons are laid out right to left in the order given, and a click
/// comes back as [`DiffAction::HunkButton`] with the button's index.
#[derive(Debug, Clone, PartialEq)]
pub struct HunkButton {
    pub label: String,
    pub hint: Option<String>,
    pub danger: bool,
    pub width: f32,
}

/// Everything the body draws in one frame.
pub struct DiffInput<'a> {
    pub diff: &'a FileDiff,
    /// Review threads of the host, for any file; the body shows those whose path matches the
    /// diff's.
    pub threads: &'a [Thread],
    /// Name used on comments written in the composer.
    pub author: &'a str,
    pub mode: DiffMode,
    /// Draw without an inner vertical scroll area, so several bodies can share one scroll.
    pub stacked: bool,
    pub hunk_buttons: &'a [HunkButton],
    /// When set, changed lines can be selected by clicking their line numbers, and this is the
    /// tooltip. Read the result with [`DiffBody::selection`].
    pub line_select_hint: Option<&'a str>,
    /// Host-specific wording of the thread cards and the composer.
    pub texts: &'a DiffTexts,
    /// How the body looks. `DiffStyle::default()` is Kelp's own look.
    pub style: DiffStyle,
}

/// Text the body shows that depends on the host: where comments are stored and which shortcuts
/// send them. The default is Kelp's wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffTexts {
    /// Under a thread's reply box, saying where comments live.
    pub saved_hint: Cow<'static, str>,
    /// Placeholder of a thread's reply box.
    pub reply_hint: Cow<'static, str>,
    /// Placeholder of the new comment box. The embedded look ignores it.
    pub comment_hint: Cow<'static, str>,
}

impl Default for DiffTexts {
    fn default() -> Self {
        Self {
            saved_hint: Cow::Borrowed("Saved in this repo only · not pushed"),
            reply_hint: Cow::Owned(format!(
                "Reply…  ({} to send)",
                crate::widgets::chord_label(cfg!(target_os = "macos"), "⌘ Enter")
            )),
            comment_hint: Cow::Borrowed("Leave a comment"),
        }
    }
}

/// The width of a thread card or composer in a pane `available` points wide: 640 when there is
/// room, narrower when the pane is, never below a readable minimum.
fn thread_card_width(available: f32) -> f32 {
    (available - THREAD_INDENT - THREAD_MARGIN).clamp(THREAD_MIN_W, THREAD_W)
}

/// An edit the user made in the body. The body never changes the host's threads itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffAction {
    /// A new thread; the host adds the file path and commit when it stores it.
    AddThread {
        anchor: Anchor,
        comment: Comment,
    },
    Reply {
        thread: u64,
        comment: Comment,
    },
    SetResolved {
        thread: u64,
        resolved: bool,
    },
    DeleteThread(u64),
    HunkButton {
        hunk: usize,
        button: usize,
        shift: bool,
    },
}

#[derive(Default)]
struct Prepared {
    hunks: Vec<String>,
    hunk_labels: Vec<String>,
    lines: Vec<(usize, Line)>,
    old_lines: Vec<String>,
    new_lines: Vec<String>,
    file_change_starts: Vec<u32>,
}

impl Prepared {
    fn of(diff: &FileDiff) -> Self {
        let mut hunks = Vec::new();
        let mut hunk_labels = Vec::new();
        let mut lines = Vec::new();
        if let Body::Text(list) = &diff.body {
            for (h, hunk) in list.iter().enumerate() {
                hunks.push(hunk.header.clone());
                hunk_labels.push(hunk_label(&hunk.header).unwrap_or_else(|| hunk.header.clone()));
                lines.extend(hunk.lines.iter().cloned().map(|l| (h, l)));
            }
        }
        let split = |t: &Option<String>| {
            t.as_deref()
                .map(|t| t.lines().map(|l| l.replace('\t', "    ")).collect())
                .unwrap_or_default()
        };
        let file_change_starts = change_starts(&lines);
        Self {
            old_lines: split(&diff.old_text),
            new_lines: split(&diff.new_text),
            hunks,
            hunk_labels,
            lines,
            file_change_starts,
        }
    }
}

/// A diff view and the state it keeps between frames (layout, scroll, drafts, selection).
pub struct DiffBody {
    data: Prepared,
    layout: DiffLayout,
    composer: Option<Composer>,
    composer_focused: bool,
    range: Option<LineRange>,
    range_drag: bool,
    range_fresh: bool,
    lit: Option<u64>,
    lit_next: Option<u64>,
    lit_span: Option<(Side, u32, u32)>,
    draft: String,
    replies: HashMap<u64, String>,
    expanded: HashMap<u64, bool>,
    selected_lines: BTreeSet<usize>,
    select_anchor: Option<usize>,
    scroll_y: f32,
    jump: Option<Jump>,
}

impl DiffBody {
    /// Prepares the body for `diff`.
    pub fn new(diff: &FileDiff) -> Self {
        Self {
            data: Prepared::of(diff),
            layout: DiffLayout::default(),
            composer: None,
            composer_focused: false,
            range: None,
            range_drag: false,
            range_fresh: false,
            lit: None,
            lit_next: None,
            lit_span: None,
            draft: String::new(),
            replies: HashMap::new(),
            expanded: HashMap::new(),
            selected_lines: BTreeSet::new(),
            select_anchor: None,
            scroll_y: 0.0,
            jump: None,
        }
    }

    /// Switches to a fresh `diff` of the same file, keeping drafts, layout and scroll. The line
    /// selection survives only if the lines are unchanged.
    pub fn reload(&mut self, diff: &FileDiff) {
        let fresh = Prepared::of(diff);
        if !same_lines(&self.data.lines, &fresh.lines) {
            self.selected_lines.clear();
            self.select_anchor = None;
            self.range = None;
            self.range_drag = false;
        }
        self.data = fresh;
        self.jump = None;
    }

    pub fn layout(&self) -> DiffLayout {
        self.layout
    }

    pub fn set_layout(&mut self, layout: DiffLayout) {
        self.layout = layout;
    }

    /// Whether there is a change to jump to in `mode`.
    pub fn has_changes_to_jump_to(&self, mode: DiffMode) -> bool {
        match mode {
            DiffMode::Diff => !self.data.hunks.is_empty(),
            DiffMode::File => !self.data.file_change_starts.is_empty(),
        }
    }

    /// Scrolls to the next or previous change on the next frame.
    pub fn jump_to_change(&mut self, next: bool) {
        self.jump = Some(if next { Jump::Next } else { Jump::Previous });
    }

    /// Handles Alt+Up and Alt+Down unless a text field has the keyboard.
    pub fn jump_with_keys(&mut self, ui: &Ui) {
        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowUp)) {
            self.jump = Some(Jump::Previous);
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowDown)) {
            self.jump = Some(Jump::Next);
        }
    }

    /// Indexes of the selected lines, counted across all hunks.
    pub fn selection(&self) -> &BTreeSet<usize> {
        &self.selected_lines
    }

    pub fn clear_selection(&mut self) {
        self.selected_lines.clear();
        self.select_anchor = None;
    }

    pub fn toggle_line(&mut self, index: usize, extend: bool) {
        match self.select_anchor.filter(|_| extend) {
            Some(anchor) => {
                let (from, to) = (anchor.min(index), anchor.max(index));
                let changed =
                    (from..=to).filter(|&i| self.data.lines[i].1.kind != LineKind::Context);
                self.selected_lines.extend(changed);
            }
            None => {
                if !self.selected_lines.remove(&index) {
                    self.selected_lines.insert(index);
                }
            }
        }
        self.select_anchor = Some(index);
    }

    /// Opens the comment composer under line `last` of `side`, covering `first` to `last`
    /// (1-based, inclusive).
    pub fn open_composer(&mut self, side: Side, first: u32, last: u32) {
        let (first, last) = (first.min(last), first.max(last));
        let hunk = self.hunk_of(side, first);
        let mut range = LineRange::new(side, hunk, first);
        range.head = last;
        self.range = Some(range);
        self.range_fresh = false;
        self.composer = Some(Composer { side, first, last });
        self.composer_focused = false;
        self.draft.clear();
    }

    fn hunk_of(&self, side: Side, line: u32) -> usize {
        self.data
            .lines
            .iter()
            .find(|(_, l)| key_of(l) == Some((side, line)))
            .map_or(0, |(hunk, _)| *hunk)
    }

    pub fn scroll_y(&self) -> f32 {
        self.scroll_y
    }

    /// Draws the diff (or a notice for binary, oversized and submodule changes) and returns what
    /// the user did.
    pub fn ui(&mut self, ui: &mut Ui, input: DiffInput<'_>) -> Vec<DiffAction> {
        if let Some(text) = &input.diff.new_text {
            crate::fonts::ensure_fallback(ui.ctx(), text);
        }
        let mut actions = Vec::new();
        match &input.diff.body {
            Body::Submodule(change) => submodule_card(ui, change),
            Body::Binary => notice(ui, "Binary file, no text to show."),
            Body::TooLarge => notice(ui, "This file is too large to show here."),
            Body::Text(_) if input.mode == DiffMode::Diff && self.data.lines.is_empty() => {
                notice(ui, "No changes in this file.")
            }
            Body::Text(_) => {
                if input.stacked {
                    self.flowing_body(ui, &input, &mut actions);
                } else {
                    self.body(ui, &input, &mut actions);
                }
            }
        }
        actions
    }

    fn items(&self, input: &DiffInput<'_>) -> Vec<Item> {
        let threads: Vec<&Thread> = input
            .threads
            .iter()
            .filter(|t| t.path == input.diff.path)
            .collect();
        let mut placed: HashMap<(Side, u32), Vec<u64>> = HashMap::new();
        let mut outside = Vec::new();
        let old: Vec<&str> = self.data.old_lines.iter().map(String::as_str).collect();
        let new: Vec<&str> = self.data.new_lines.iter().map(String::as_str).collect();
        for thread in &threads {
            let text = if thread.anchor.side == Side::Old {
                &old
            } else {
                &new
            };
            match thread.anchor.locate(text) {
                Some(i) => placed
                    .entry((
                        thread.anchor.side,
                        thread.anchor.last_index(i, text.len()) as u32 + 1,
                    ))
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
            if self.composer.map(|c| (c.side, c.last)) == Some(key) {
                items.push(Item::Composer);
            }
            shown.push(key);
        };
        let mut shown = Vec::new();
        match input.mode {
            DiffMode::File => {
                for i in 0..self.data.new_lines.len() {
                    items.push(Item::FileLine(i));
                    attach(&mut items, (Side::New, i as u32 + 1), &mut shown);
                }
            }
            DiffMode::Diff => {
                let mut current_hunk = usize::MAX;
                let mut i = 0;
                while i < self.data.lines.len() {
                    let (hunk, line) = &self.data.lines[i];
                    if *hunk != current_hunk {
                        current_hunk = *hunk;
                        items.push(Item::Hunk(*hunk));
                    }
                    match self.layout {
                        DiffLayout::Unified => {
                            items.push(Item::Line(i));
                            if let Some(key) = key_of(line) {
                                attach(&mut items, key, &mut shown);
                            }
                            i += 1;
                        }
                        DiffLayout::Split => {
                            let block_end = |from: usize, kind: LineKind| {
                                let mut j = from;
                                while j < self.data.lines.len()
                                    && self.data.lines[j].1.kind == kind
                                    && self.data.lines[j].0 == *hunk
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
                                    if let Some(key) = key_of(&self.data.lines[side].1) {
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

    fn height(&self, ui: &Ui, item: &Item, card_w: f32, input: &DiffInput<'_>) -> f32 {
        match item {
            Item::Hunk(_) | Item::OutsideHeader => HUNK_H,
            Item::Line(_) | Item::Split(..) | Item::FileLine(_) => LINE_H,
            Item::Composer => composer::height(ui, &self.draft, card_w),
            Item::Thread(id) => {
                let Some(thread) = input
                    .threads
                    .iter()
                    .find(|t| t.id == *id && t.path == input.diff.path)
                else {
                    return 0.0;
                };
                if thread.resolved && !self.expanded.get(id).copied().unwrap_or(false) {
                    return RESOLVED_H;
                }
                if input.style.note_card {
                    return note::measured_height(ui, thread, card_w);
                }
                let body_w = card_w - 64.0;
                let comments: f32 = thread
                    .comments
                    .iter()
                    .map(|c| {
                        let g = ui.painter().layout(
                            c.body.clone(),
                            FontId::proportional(13.0),
                            theme::text(),
                            body_w,
                        );
                        26.0 + g.size().y + 14.0
                    })
                    .sum();
                20.0 + comments + 92.0
            }
        }
    }

    fn body(&mut self, ui: &mut Ui, input: &DiffInput<'_>, actions: &mut Vec<DiffAction>) {
        let rows = self.rows(ui, input);
        let mut scroll = egui::ScrollArea::both().auto_shrink(false);
        if let Some(jump) = self.jump.take()
            && let Some(target) = self.jump_target(&rows.items, &rows.offsets, jump)
        {
            scroll = scroll.vertical_scroll_offset(target);
        }
        scroll.show_viewport(ui, |ui, viewport| {
            self.scroll_y = viewport.min.y;
            let width = ui.available_width().max(self.min_width(input));
            let (full, _) = ui.allocate_exact_size(vec2(width, rows.total), Sense::hover());
            self.draw_rows(ui, &rows, full.min, width, viewport, input, actions);
        });
    }

    fn flowing_body(&mut self, ui: &mut Ui, input: &DiffInput<'_>, actions: &mut Vec<DiffAction>) {
        self.jump = None;
        let rows = self.rows(ui, input);
        egui::ScrollArea::horizontal()
            .auto_shrink([false, true])
            .show_viewport(ui, |ui, viewport| {
                let width = ui.available_width().max(self.min_width(input));
                let (full, _) = ui.allocate_exact_size(vec2(width, rows.total), Sense::hover());
                let clip = ui.clip_rect();
                let visible = Rect::from_min_max(
                    pos2(viewport.min.x, clip.top() - full.top()),
                    pos2(viewport.max.x, clip.bottom() - full.top()),
                );
                self.draw_rows(ui, &rows, full.min, width, visible, input, actions);
            });
    }

    fn min_width(&self, input: &DiffInput<'_>) -> f32 {
        if self.layout == DiffLayout::Split && input.mode == DiffMode::Diff {
            900.0
        } else {
            1100.0
        }
    }

    fn rows(&self, ui: &Ui, input: &DiffInput<'_>) -> Rows {
        let items = self.items(input);
        let card_w = thread_card_width(ui.available_width());
        let heights: Vec<f32> = items
            .iter()
            .map(|i| self.height(ui, i, card_w, input))
            .collect();
        let mut offsets = Vec::with_capacity(items.len() + 1);
        let mut y = 0.0;
        for h in &heights {
            offsets.push(y);
            y += h;
        }
        Rows {
            card_w,
            items,
            heights,
            offsets,
            total: y,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_rows(
        &mut self,
        ui: &mut Ui,
        rows: &Rows,
        origin: egui::Pos2,
        width: f32,
        viewport: Rect,
        input: &DiffInput<'_>,
        actions: &mut Vec<DiffAction>,
    ) {
        let Rows {
            card_w,
            items,
            heights,
            offsets,
            ..
        } = rows;
        let card_w = *card_w;
        let font = FontId::monospace(12.5);
        self.lit = self.lit_next.take();
        self.lit_span = self.lit.and_then(|id| self.thread_span(input, id));
        self.drive_range(ui, rows, origin);
        let first = offsets.partition_point(|o| *o + LINE_H * 4.0 < viewport.min.y);
        for (k, item) in items.iter().enumerate().skip(first) {
            let top = offsets[k];
            if top > viewport.max.y {
                break;
            }
            let rect = Rect::from_min_size(origin + vec2(0.0, top), vec2(width, heights[k]));
            match *item {
                Item::Hunk(h) => {
                    let visible_right = rect.left() + viewport.max.x - viewport.min.x;
                    let mut right = visible_right.min(rect.right()) - 12.0;
                    if input.style.hunk_divider {
                        let buttons_w: f32 = input.hunk_buttons.iter().map(|b| b.width + 8.0).sum();
                        hunk_divider(ui, rect, &self.data.hunk_labels[h], right - buttons_w);
                    } else {
                        ui.painter().rect_filled(rect, 0.0, theme::hunk_bg());
                        ui.painter().text(
                            pos2(rect.left() + 16.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            &self.data.hunks[h],
                            font.clone(),
                            theme::hunk_text(),
                        );
                    }
                    for (index, spec) in input.hunk_buttons.iter().enumerate() {
                        let button_rect = Rect::from_min_size(
                            pos2(right - spec.width, rect.top() + 3.0),
                            vec2(spec.width, rect.height() - 6.0),
                        );
                        right = button_rect.left() - 8.0;
                        let label = RichText::new(&spec.label).size(12.0);
                        let button = if spec.danger {
                            egui::Button::new(label.color(theme::deleted()))
                                .fill(theme::with_alpha(theme::deleted(), 0x1c))
                        } else {
                            egui::Button::new(label)
                                .fill(theme::tone(Color32::from_rgb(0x24, 0x2c, 0x3a)))
                        }
                        .corner_radius(5);
                        let mut response = ui.put(button_rect, button);
                        if let Some(hint) = &spec.hint {
                            response = response.on_hover_text(hint);
                        }
                        if response.clicked() {
                            let shift = ui.input(|i| i.modifiers.shift);
                            actions.push(DiffAction::HunkButton {
                                hunk: h,
                                button: index,
                                shift,
                            });
                        }
                    }
                }
                Item::OutsideHeader => {
                    ui.painter().rect_filled(
                        rect,
                        0.0,
                        theme::tone(Color32::from_rgb(0x1f, 0x23, 0x2b)),
                    );
                    ui.painter().text(
                        pos2(rect.left() + 16.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        "Comments on lines not shown here",
                        FontId::proportional(12.0),
                        theme::text_muted(),
                    );
                }
                Item::Line(i) => self.unified_line(ui, rect, i, &font, input),
                Item::Split(l, r) => {
                    let half = rect.width() / 2.0;
                    let left = Rect::from_min_size(rect.min, vec2(half, rect.height()));
                    let right =
                        Rect::from_min_size(rect.min + vec2(half, 0.0), vec2(half, rect.height()));
                    self.split_side(ui, left, l, Side::Old, &font, input);
                    self.split_side(ui, right, r, Side::New, &font, input);
                    ui.painter().vline(
                        right.left(),
                        rect.y_range(),
                        Stroke::new(1.0, theme::border()),
                    );
                }
                Item::FileLine(i) => {
                    let text = self.data.new_lines[i].clone();
                    let number = i as u32 + 1;
                    self.code_row(
                        ui,
                        rect,
                        Row {
                            hunk: 0,
                            numbers: Numbers::One(Some(number)),
                            text: &text,
                            emphasis: &[],
                            look: input.style.look(LineKind::Context),
                            side: Side::New,
                            selectable: None,
                        },
                        &font,
                        input,
                    );
                }
                Item::Thread(id) => self.thread(ui, rect, card_w, id, input, actions),
                Item::Composer => self.composer_ui(ui, rect, card_w, input, actions),
            }
        }
        if self.lit != self.lit_next {
            ui.ctx().request_repaint();
        }
    }

    fn thread_span(&self, input: &DiffInput<'_>, id: u64) -> Option<(Side, u32, u32)> {
        let thread = input
            .threads
            .iter()
            .find(|t| t.id == id && t.path == input.diff.path)?;
        let anchor = &thread.anchor;
        if anchor.span == 0 {
            return None;
        }
        let text = if anchor.side == Side::Old {
            &self.data.old_lines
        } else {
            &self.data.new_lines
        };
        let lines: Vec<&str> = text.iter().map(String::as_str).collect();
        let first = anchor.locate(&lines)?;
        let last = anchor.last_index(first, lines.len());
        Some((anchor.side, first as u32 + 1, last as u32 + 1))
    }

    fn item_line(&self, item: &Item, side: Side) -> Option<(usize, u32)> {
        let index = match *item {
            Item::Line(i) => Some(i),
            Item::Split(old, new) => {
                if side == Side::Old {
                    old
                } else {
                    new
                }
            }
            Item::FileLine(i) => return (side == Side::New).then_some((0, i as u32 + 1)),
            _ => None,
        }?;
        let (hunk, line) = &self.data.lines[index];
        let number = if side == Side::Old {
            line.old
        } else {
            line.new
        };
        Some((*hunk, number?))
    }

    fn candidates(&self, rows: &Rows, side: Side, hunk: usize) -> Vec<Candidate> {
        rows.items
            .iter()
            .zip(&rows.offsets)
            .filter_map(|(item, &top)| {
                let (item_hunk, number) = self.item_line(item, side)?;
                (item_hunk == hunk).then_some(Candidate { top, number })
            })
            .collect()
    }

    fn press_line(&mut self, ui: &Ui, hunk: usize, side: Side, line: u32) {
        let shift = ui.input(|i| i.modifiers.shift);
        match &mut self.range {
            Some(range) if shift && range.extends_to(side, hunk) => range.head = line,
            _ => {
                self.range = Some(LineRange::new(side, hunk, line));
                self.range_fresh = true;
            }
        }
        self.range_drag = true;
        ui.ctx().request_repaint();
    }

    fn drive_range(&mut self, ui: &Ui, rows: &Rows, origin: egui::Pos2) {
        let Some(mut range) = self.range else {
            self.range_drag = false;
            return;
        };
        if self.composer.is_none()
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.range = None;
            self.range_drag = false;
            self.range_fresh = false;
            ui.ctx().request_repaint();
            return;
        }
        if !self.range_drag {
            return;
        }
        let before = range;
        let (pos, down) = ui.input(|i| (i.pointer.latest_pos(), i.pointer.primary_down()));
        if let Some(pos) = pos {
            let rows = self.candidates(rows, range.side, range.hunk);
            if let Some(line) = head_at(&rows, pos.y - origin.y) {
                range.head = line;
            }
        }
        self.range = Some(range);
        if !down {
            self.range_drag = false;
            if std::mem::take(&mut self.range_fresh) {
                self.draft.clear();
            }
            self.composer = Some(Composer {
                side: range.side,
                first: range.first(),
                last: range.last(),
            });
            self.composer_focused = false;
        }
        if range != before || !down {
            ui.ctx().request_repaint();
        }
    }

    fn jump_target(&self, items: &[Item], offsets: &[f32], jump: Jump) -> Option<f32> {
        let targets = items.iter().zip(offsets).filter_map(|(item, &top)| {
            let is_start = match *item {
                Item::Hunk(_) => true,
                Item::FileLine(i) => self.data.file_change_starts.contains(&(i as u32 + 1)),
                _ => false,
            };
            is_start.then_some((top - LINE_H * 2.0).max(0.0))
        });
        let here = self.scroll_y;
        match jump {
            Jump::Next => targets.into_iter().find(|&t| t > here + 1.0),
            Jump::Previous => targets.into_iter().rfind(|&t| t < here - 1.0),
        }
    }

    fn unified_line(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        index: usize,
        font: &FontId,
        input: &DiffInput<'_>,
    ) {
        let line = self.data.lines[index].1.clone();
        let look = input.style.look(line.kind);
        let side = if line.kind == LineKind::Removed {
            Side::Old
        } else {
            Side::New
        };
        let row = Row {
            hunk: self.data.lines[index].0,
            numbers: Numbers::Both(line.old, line.new),
            text: &line.text,
            emphasis: &line.emphasis,
            look,
            side,
            selectable: self.selectable(index, input),
        };
        self.code_row(ui, rect, row, font, input);
    }

    fn split_side(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        index: Option<usize>,
        side: Side,
        font: &FontId,
        input: &DiffInput<'_>,
    ) {
        let Some(index) = index else {
            let filler = if input.style.tinted() {
                Color32::TRANSPARENT
            } else {
                theme::tone(Color32::from_rgb(0x18, 0x1b, 0x21))
            };
            ui.painter().rect_filled(rect, 0.0, filler);
            return;
        };
        let line = self.data.lines[index].1.clone();
        let look = input.style.look(line.kind);
        let number = if side == Side::Old {
            line.old
        } else {
            line.new
        };
        let row = Row {
            hunk: self.data.lines[index].0,
            numbers: Numbers::One(number),
            text: &line.text,
            emphasis: &line.emphasis,
            look,
            side,
            selectable: self.selectable(index, input),
        };
        self.code_row(ui, rect, row, font, input);
    }

    fn selectable(&self, index: usize, input: &DiffInput<'_>) -> Option<usize> {
        (input.line_select_hint.is_some() && self.data.lines[index].1.kind != LineKind::Context)
            .then_some(index)
    }

    fn code_row(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        row: Row,
        font: &FontId,
        input: &DiffInput<'_>,
    ) {
        let painter = ui.painter_at(rect);
        let look = row.look;
        let accent = input.style.accent();
        painter.rect_filled(rect, 0.0, look.fill);
        if let Some(bar) = look.bar {
            painter.rect_filled(
                Rect::from_min_size(rect.min, vec2(input.style.bar_w, rect.height())),
                0.0,
                bar,
            );
        }
        let number_on = |side: Side| match row.numbers {
            Numbers::Both(old, new) => {
                if side == Side::Old {
                    old
                } else {
                    new
                }
            }
            Numbers::One(n) => n.filter(|_| row.side == side),
        };
        let picked = self
            .range
            .filter(LineRange::is_multi)
            .is_some_and(|r| number_on(r.side).is_some_and(|n| r.contains(n)));
        if picked {
            painter.rect_filled(rect, 0.0, theme::with_alpha(accent, 0x22));
            painter.rect_filled(
                Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
                0.0,
                accent,
            );
        } else if self.lit_span.is_some_and(|(side, first, last)| {
            number_on(side).is_some_and(|n| (first..=last).contains(&n))
        }) {
            painter.rect_filled(rect, 0.0, theme::with_alpha(accent, 0x12));
        }
        let mid = rect.center().y;
        let num = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
        let numbers_w = match row.numbers {
            Numbers::Both(..) => NUM_W * 2.0,
            Numbers::One(_) => NUM_W,
        };
        if let Some(index) = row.selectable {
            let numbers_rect = Rect::from_min_size(rect.min, vec2(numbers_w, rect.height()));
            let response = ui
                .interact(
                    numbers_rect,
                    ui.id().with(("select-line", index, rect.left() as i32)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            if self.selected_lines.contains(&index) {
                painter.rect_filled(numbers_rect, 0.0, theme::with_alpha(accent, 0x38));
                painter.rect_filled(
                    Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
                    0.0,
                    accent,
                );
            } else if response.hovered() {
                painter.rect_filled(numbers_rect, 0.0, theme::overlay(0x0c));
            }
            let hint = input.line_select_hint.unwrap_or_default();
            if response.on_hover_text(hint).clicked() {
                let extend = ui.input(|i| i.modifiers.shift);
                self.toggle_line(index, extend);
            }
        }
        let line_no = match row.numbers {
            Numbers::Both(old, new) => {
                painter.text(
                    pos2(rect.left() + NUM_W - 8.0, mid),
                    Align2::RIGHT_CENTER,
                    num(old),
                    font.clone(),
                    look.number,
                );
                painter.text(
                    pos2(rect.left() + NUM_W * 2.0 - 8.0, mid),
                    Align2::RIGHT_CENTER,
                    num(new),
                    font.clone(),
                    look.number,
                );
                if row.side == Side::Old { old } else { new }
            }
            Numbers::One(n) => {
                painter.text(
                    pos2(rect.left() + NUM_W - 8.0, mid),
                    Align2::RIGHT_CENTER,
                    num(n),
                    font.clone(),
                    look.number,
                );
                n
            }
        };
        let side = row.side;
        let gutter = Rect::from_min_size(
            pos2(rect.left() + numbers_w, rect.top()),
            vec2(GUTTER_W, rect.height()),
        );
        if let Some(line_no) = line_no {
            let button = Rect::from_center_size(gutter.center(), vec2(18.0, 18.0));
            let handle = if row.selectable.is_none() {
                Rect::from_min_max(rect.min, pos2(gutter.right(), rect.bottom()))
            } else {
                button
            };
            let response = ui.interact(
                handle,
                ui.id()
                    .with(("comment", side as u8, line_no, rect.left() as i32)),
                Sense::click_and_drag(),
            );
            let hovered = response.hovered();
            if hovered {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                if row.selectable.is_none() {
                    painter.rect_filled(
                        Rect::from_min_size(rect.min, vec2(numbers_w, rect.height())),
                        0.0,
                        theme::overlay(0x0c),
                    );
                }
                if ui.input(|i| i.pointer.primary_pressed()) {
                    self.press_line(ui, row.hunk, side, line_no);
                }
            }
            if ui.rect_contains_pointer(rect) {
                let on_button = hovered && ui.rect_contains_pointer(button);
                painter.rect_filled(
                    button,
                    4.0,
                    if on_button {
                        theme::text_strong()
                    } else {
                        accent
                    },
                );
                painter.text(
                    button.center(),
                    Align2::CENTER_CENTER,
                    "+",
                    FontId::proportional(14.0),
                    theme::on_accent(),
                );
                if on_button {
                    response.on_hover_text("Add a comment, or drag to cover several lines");
                }
            }
        }
        painter.text(
            pos2(gutter.right() + 4.0, mid),
            Align2::LEFT_CENTER,
            look.mark,
            font.clone(),
            look.mark_color,
        );
        let galley = painter.layout_no_wrap(row.text.to_string(), font.clone(), look.text);
        let origin = pos2(gutter.right() + 20.0, mid - galley.size().y / 2.0);
        for &(start, end) in row.emphasis {
            let x = |byte: usize| {
                let chars = row.text.get(..byte).map_or(0, |t| t.chars().count());
                galley
                    .pos_from_cursor(egui::text::CCursor::new(chars))
                    .min
                    .x
            };
            let span = Rect::from_x_y_ranges(
                origin.x + x(start)..=origin.x + x(end),
                rect.top() + 2.0..=rect.bottom() - 2.0,
            );
            painter.rect_filled(span, 3.0, look.emphasis);
        }
        painter.galley(origin, galley, look.text);
    }

    fn thread(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        card_w: f32,
        id: u64,
        input: &DiffInput<'_>,
        actions: &mut Vec<DiffAction>,
    ) {
        let Some(thread) = input
            .threads
            .iter()
            .find(|t| t.id == id && t.path == input.diff.path)
        else {
            return;
        };
        let expanded = self.expanded.get(&id).copied().unwrap_or(false);
        let card = Rect::from_min_size(
            pos2(rect.left() + THREAD_INDENT, rect.top() + 6.0),
            vec2(card_w, rect.height() - 12.0),
        );
        if thread.resolved && !expanded {
            let chip_w = if thread.anchor.span > 0 { 270.0 } else { 220.0 };
            let chip = Rect::from_min_size(card.min, vec2(chip_w, 24.0));
            let response = ui.interact(chip, ui.id().with(("resolved", id)), Sense::click());
            let (fill, border) = input
                .style
                .note(theme::tone(Color32::from_rgb(0x2c, 0x34, 0x42)));
            ui.painter().rect(
                chip,
                12.0,
                fill,
                Stroke::new(1.0, border),
                egui::StrokeKind::Inside,
            );
            let label = format!(
                "✔ {} resolved comment{} · {}",
                thread.comments.len(),
                if thread.comments.len() == 1 { "" } else { "s" },
                anchor_label(&thread.anchor).to_lowercase()
            );
            ui.painter().text(
                pos2(chip.left() + 12.0, chip.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(12.0),
                theme::text_muted(),
            );
            if response.on_hover_text("Show").clicked() {
                self.expanded.insert(id, true);
            }
            return;
        }
        note::paint_card(ui, card, &input.style);
        if ui.rect_contains_pointer(card) {
            self.lit_next = Some(id);
        }
        if input.style.note_card {
            self.embedded_thread(ui, card, thread, input, actions);
            return;
        }
        let mut y = card.top() + 12.0;
        for (n, comment) in thread.comments.iter().enumerate() {
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
                theme::text_strong(),
            );
            let when = kelp_core::commit::relative_time(comment.time, now());
            ui.painter().text(
                pos2(header.right() + 8.0, y + 5.0),
                Align2::LEFT_TOP,
                when,
                FontId::proportional(12.0),
                theme::text_faint(),
            );
            if n == 0 && thread.anchor.span > 0 {
                ui.painter().text(
                    pos2(card.right() - 14.0, y + 5.0),
                    Align2::RIGHT_TOP,
                    anchor_label(&thread.anchor),
                    FontId::proportional(12.0),
                    theme::text_faint(),
                );
            }
            let body = ui.painter().layout(
                comment.body.clone(),
                FontId::proportional(13.0),
                theme::text_control(),
                card_w - 64.0,
            );
            let body_h = body.size().y;
            ui.painter().galley(
                pos2(card.left() + 50.0, y + 24.0),
                body,
                theme::text_control(),
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
        input.style.paint_accent(&mut child);
        let draft = self.replies.entry(id).or_default();
        let reply_edit = child.add(
            egui::TextEdit::multiline(draft)
                .hint_text(input.texts.reply_hint.as_ref())
                .desired_rows(1)
                .desired_width(f32::INFINITY),
        );
        if reply_edit.has_focus() {
            self.lit_next = Some(id);
        }
        let send_shortcut = child.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter))
            && !draft.trim().is_empty();
        child.horizontal(|ui| {
            ui.label(
                RichText::new(input.texts.saved_hint.as_ref())
                    .size(11.0)
                    .color(theme::text_faint()),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_send = !self.replies.get(&id).is_some_and(|d| d.trim().is_empty());
                let reply = egui::Button::new(
                    RichText::new("Reply")
                        .family(theme::semibold())
                        .color(theme::on_accent()),
                )
                .fill(input.style.accent())
                .corner_radius(5);
                if (ui.add_enabled(can_send, reply).clicked() || send_shortcut) && can_send {
                    let body = self.replies.remove(&id).unwrap_or_default();
                    actions.push(DiffAction::Reply {
                        thread: id,
                        comment: Comment {
                            author: input.author.to_string(),
                            time: now(),
                            body: body.trim().to_string(),
                        },
                    });
                }
                let resolve_label = if thread.resolved { "Reopen" } else { "Resolve" };
                if ui
                    .add(egui::Button::new(resolve_label).corner_radius(5))
                    .clicked()
                {
                    actions.push(DiffAction::SetResolved {
                        thread: id,
                        resolved: !thread.resolved,
                    });
                    self.expanded.remove(&id);
                }
                if crate::widgets::text_button(ui, RichText::new("Delete").color(theme::deleted()))
                    .clicked()
                {
                    actions.push(DiffAction::DeleteThread(id));
                }
            });
        });
    }
}

struct Row<'a> {
    hunk: usize,
    numbers: Numbers,
    text: &'a str,
    emphasis: &'a [(usize, usize)],
    look: LineLook,
    side: Side,
    selectable: Option<usize>,
}

#[derive(Clone, Copy)]
enum Numbers {
    Both(Option<u32>, Option<u32>),
    One(Option<u32>),
}

fn hunk_divider(ui: &Ui, rect: Rect, label: &str, right: f32) {
    let painter = ui.painter_at(rect);
    let galley = painter.layout_no_wrap(
        label.to_string(),
        FontId::proportional(11.5),
        theme::text_faint(),
    );
    let left = rect.left() + 16.0;
    let mid = rect.center().y;
    let text_left = ((left + right - galley.size().x) / 2.0).max(left);
    let text_right = text_left + galley.size().x;
    let line = Stroke::new(1.0, theme::border());
    for (from, to) in [(left, text_left - 10.0), (text_right + 10.0, right)] {
        if to > from {
            painter.hline(from..=to, mid, line);
        }
    }
    painter.galley(
        pos2(text_left, mid - galley.size().y / 2.0),
        galley,
        theme::text_faint(),
    );
}

fn key_of(line: &Line) -> Option<(Side, u32)> {
    match line.kind {
        LineKind::Removed => line.old.map(|n| (Side::Old, n)),
        _ => line.new.map(|n| (Side::New, n)),
    }
}

fn change_starts(lines: &[(usize, Line)]) -> Vec<u32> {
    let mut starts = Vec::new();
    let mut last_new = 0;
    let mut in_change = false;
    for (_, line) in lines {
        match line.kind {
            LineKind::Context => in_change = false,
            _ if in_change => {}
            LineKind::Added => {
                in_change = true;
                starts.extend(line.new);
            }
            LineKind::Removed => {
                in_change = true;
                starts.push(last_new + 1);
            }
        }
        if let Some(n) = line.new {
            last_new = n;
        }
    }
    starts.dedup();
    starts
}

fn same_lines(a: &[(usize, Line)], b: &[(usize, Line)]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|((_, x), (_, y))| {
            x.kind == y.kind && x.raw == y.raw && x.old == y.old && x.new == y.new
        })
}

fn submodule_card(ui: &mut Ui, change: &kelp_core::submodules::Change) {
    let side = |id: Option<kelp_core::ObjectId>, title: &Option<String>| -> String {
        match (id, title) {
            (None, _) => "none".to_string(),
            (Some(id), Some(title)) => format!("{} {title}", id.to_hex_with_len(7)),
            (Some(id), None) => format!("{} (not checked out here)", id.to_hex_with_len(7)),
        }
    };
    egui::Frame::new()
        .inner_margin(Margin::symmetric(24, 20))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.label(
                RichText::new(format!("Submodule {}", change.path))
                    .size(14.0)
                    .family(theme::semibold())
                    .color(theme::text_strong()),
            );
            ui.label(
                RichText::new(format!("- {}", side(change.old, &change.old_title)))
                    .monospace()
                    .color(theme::deleted()),
            );
            ui.label(
                RichText::new(format!("+ {}", side(change.new, &change.new_title)))
                    .monospace()
                    .color(theme::added()),
            );
        });
}

fn notice(ui: &mut Ui, text: &str) {
    ui.centered_and_justified(|ui| ui.label(RichText::new(text).color(theme::text_muted())));
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
    use kelp_core::diff;
    use kelp_core::review::{Anchor, Comment, Side, Thread};

    use super::*;

    fn sample() -> FileDiff {
        diff::build("f.txt", Some(b"a\nb\n"), Some(b"a\nx\ny\nz\nb\n"))
    }

    fn thread(id: u64, anchor: Anchor, resolved: bool) -> Thread {
        Thread {
            id,
            path: "f.txt".into(),
            commit: None,
            anchor,
            resolved,
            comments: vec![Comment {
                author: "T".into(),
                time: 0,
                body: "hm".into(),
            }],
        }
    }

    struct Harness {
        ctx: egui::Context,
        body: DiffBody,
        diff: FileDiff,
        threads: Vec<Thread>,
        mode: DiffMode,
        hint: Option<&'static str>,
        buttons: Vec<HunkButton>,
        texts: DiffTexts,
        style: DiffStyle,
        fills: Vec<Color32>,
        other_field: Option<String>,
        height: f32,
    }

    impl Harness {
        fn new() -> Self {
            let ctx = egui::Context::default();
            crate::fonts::install(&ctx);
            let diff = sample();
            Self {
                ctx,
                body: DiffBody::new(&diff),
                diff,
                threads: Vec::new(),
                mode: DiffMode::Diff,
                hint: None,
                buttons: Vec::new(),
                texts: DiffTexts::default(),
                style: DiffStyle::default(),
                fills: Vec::new(),
                other_field: None,
                height: 150.0,
            }
        }

        fn frame(&mut self, events: Vec<Event>, modifiers: Modifiers) -> Vec<DiffAction> {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, self.height))),
                events,
                modifiers,
                ..Default::default()
            };
            let mut actions = Vec::new();
            let Self {
                ctx,
                body,
                diff,
                threads,
                mode,
                hint,
                buttons,
                texts,
                style,
                fills,
                other_field,
                ..
            } = self;
            let output = ctx.run_ui(input, |ui| {
                if let Some(text) = other_field {
                    let other = ui.add(
                        egui::TextEdit::singleline(text).id(egui::Id::new("some-other-field")),
                    );
                    if !other.has_focus() {
                        other.request_focus();
                    }
                }
                actions = body.ui(
                    ui,
                    DiffInput {
                        diff,
                        threads,
                        author: "T",
                        mode: *mode,
                        stacked: false,
                        hunk_buttons: buttons,
                        line_select_hint: *hint,
                        texts,
                        style: *style,
                    },
                );
            });
            *fills = output
                .shapes
                .iter()
                .filter_map(|clipped| match &clipped.shape {
                    egui::Shape::Rect(rect) => Some(rect.fill),
                    _ => None,
                })
                .collect();
            actions
        }

        fn click(&mut self, at: Pos2, modifiers: Modifiers) -> Vec<DiffAction> {
            let press = |pressed| Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers,
            };
            let mut actions = self.frame(vec![Event::PointerMoved(at)], modifiers);
            actions.extend(self.frame(vec![press(true)], modifiers));
            actions.extend(self.frame(vec![press(false)], modifiers));
            actions.extend(self.frame(vec![], Modifiers::NONE));
            actions
        }

        fn press(&mut self, at: Pos2, modifiers: Modifiers, pressed: bool) {
            let event = Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers,
            };
            self.frame(vec![event], modifiers);
        }

        fn drag(&mut self, from: Pos2, path: &[Pos2], modifiers: Modifiers) {
            self.frame(vec![Event::PointerMoved(from)], modifiers);
            self.press(from, modifiers, true);
            let mut at = from;
            for &next in path {
                at = next;
                self.frame(vec![Event::PointerMoved(next)], modifiers);
            }
            self.press(at, modifiers, false);
            self.frame(vec![], Modifiers::NONE);
        }

        fn cmd_enter(&mut self) -> Vec<DiffAction> {
            let key = Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            };
            self.frame(vec![key], Modifiers::COMMAND)
        }
    }

    fn line_center(index: usize) -> Pos2 {
        pos2(20.0, HUNK_H + index as f32 * LINE_H + LINE_H / 2.0)
    }

    fn kinds(body: &DiffBody, input: &DiffInput<'_>) -> Vec<&'static str> {
        body.items(input)
            .iter()
            .map(|item| match item {
                Item::Hunk(_) => "hunk",
                Item::Line(_) => "line",
                Item::Split(..) => "split",
                Item::FileLine(_) => "file-line",
                Item::Thread(_) => "thread",
                Item::Composer => "composer",
                Item::OutsideHeader => "outside",
            })
            .collect()
    }

    fn input<'a>(h: &'a Harness) -> DiffInput<'a> {
        DiffInput {
            diff: &h.diff,
            threads: &h.threads,
            author: "T",
            mode: h.mode,
            stacked: false,
            hunk_buttons: &h.buttons,
            line_select_hint: h.hint,
            texts: &h.texts,
            style: DiffStyle::default(),
        }
    }

    #[test]
    fn clicking_a_changed_line_number_selects_it_and_shift_extends_the_range() {
        let mut h = Harness::new();
        h.hint = Some("Select");
        h.frame(vec![], Modifiers::NONE);

        h.click(line_center(2), Modifiers::NONE);
        assert_eq!(h.body.selection().iter().copied().collect::<Vec<_>>(), [2]);
        h.click(line_center(3), Modifiers::SHIFT);
        assert_eq!(
            h.body.selection().iter().copied().collect::<Vec<_>>(),
            [2, 3]
        );
        h.click(line_center(0), Modifiers::NONE);
        assert_eq!(h.body.selection().len(), 2, "context lines stay unselected");
    }

    #[test]
    fn without_a_select_hint_lines_cannot_be_selected() {
        let mut h = Harness::new();
        h.frame(vec![], Modifiers::NONE);
        h.click(line_center(2), Modifiers::NONE);
        assert_eq!(h.body.selection().len(), 0);
    }

    #[test]
    fn a_hunk_button_click_reports_its_hunk_and_position() {
        let mut h = Harness::new();
        h.buttons = vec![
            HunkButton {
                label: "Stage hunk".into(),
                hint: None,
                danger: false,
                width: 118.0,
            },
            HunkButton {
                label: "Discard".into(),
                hint: Some("Undo".into()),
                danger: true,
                width: 76.0,
            },
        ];
        h.frame(vec![], Modifiers::NONE);
        let stage = h.click(pos2(1200.0 - 12.0 - 59.0, HUNK_H / 2.0), Modifiers::NONE);
        assert_eq!(
            stage,
            [DiffAction::HunkButton {
                hunk: 0,
                button: 0,
                shift: false
            }]
        );
        let discard = h.click(
            pos2(1200.0 - 12.0 - 118.0 - 8.0 - 38.0, HUNK_H / 2.0),
            Modifiers::SHIFT,
        );
        assert_eq!(
            discard,
            [DiffAction::HunkButton {
                hunk: 0,
                button: 1,
                shift: true
            }]
        );
    }

    #[test]
    fn saving_the_composer_asks_the_host_to_add_a_thread_anchored_on_the_line() {
        let mut h = Harness::new();
        h.body.open_composer(Side::New, 3, 3);
        h.body.draft = "  why y?  ".into();
        h.frame(vec![], Modifiers::NONE);
        let actions = h.cmd_enter();
        let [DiffAction::AddThread { anchor, comment }] = actions.as_slice() else {
            panic!("expected one AddThread, got {actions:?}")
        };
        assert_eq!(anchor.side, Side::New);
        assert_eq!(anchor.text, "y");
        assert_eq!(anchor.line_hint, 3);
        assert_eq!(comment.body, "why y?");
        assert_eq!(comment.author, "T");
        assert!(h.body.composer.is_none());
    }

    fn key(key: egui::Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn escape_cancels_a_focused_composer_and_drops_its_draft() {
        let mut h = Harness::new();
        h.body.open_composer(Side::New, 3, 3);
        h.body.draft = "draft".into();
        h.frame(vec![], Modifiers::NONE);
        let actions = h.frame(
            vec![key(egui::Key::Escape, Modifiers::NONE)],
            Modifiers::NONE,
        );
        assert_eq!(actions, []);
        assert!(h.body.composer.is_none());
        assert_eq!(h.body.draft, "");
    }

    #[test]
    fn the_composer_keeps_its_hands_off_keys_while_another_field_has_focus() {
        let mut h = Harness::new();
        h.other_field = Some(String::new());
        h.frame(vec![], Modifiers::NONE);
        h.body.open_composer(Side::New, 3, 3);
        h.body.draft = "draft".into();
        h.frame(vec![], Modifiers::NONE);
        h.frame(
            vec![key(egui::Key::Escape, Modifiers::NONE)],
            Modifiers::NONE,
        );
        assert!(h.body.composer.is_some());
        let actions = h.cmd_enter();
        assert_eq!(actions, []);
        assert_eq!(h.body.draft, "draft");
    }

    #[test]
    fn an_empty_comment_cannot_be_saved_with_the_shortcut() {
        let mut h = Harness::new();
        h.body.open_composer(Side::New, 3, 3);
        h.body.draft = "  ".into();
        h.frame(vec![], Modifiers::NONE);
        assert_eq!(h.cmd_enter(), []);
        assert!(h.body.composer.is_some());
    }

    #[test]
    fn the_composer_row_grows_with_the_draft_and_is_three_rows_tall_when_empty() {
        let mut h = Harness::new();
        h.body.open_composer(Side::New, 3, 3);
        let empty = composer_height(&h);
        h.body.draft = "one\ntwo\nthree".into();
        assert_eq!(composer_height(&h), empty);
        h.body.draft = "1\n2\n3\n4\n5\n6\n7\n8".into();
        assert!(composer_height(&h) > empty + 40.0);
    }

    fn composer_height(h: &Harness) -> f32 {
        let mut seen = 0.0;
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 400.0))),
            ..Default::default()
        };
        let _ = h.ctx.run_ui(raw, |ui| {
            let rows = h.body.rows(ui, &input(h));
            let at = rows
                .items
                .iter()
                .position(|i| matches!(i, Item::Composer))
                .expect("a composer row");
            seen = rows.heights[at];
        });
        seen
    }

    fn composer_range(h: &Harness) -> Option<(Side, u32, u32)> {
        h.body.composer.map(|c| (c.side, c.first, c.last))
    }

    fn tint(alpha: u8) -> Color32 {
        theme::with_alpha(theme::accent(), alpha)
    }

    #[test]
    fn dragging_down_a_number_gutter_selects_a_block_and_releasing_opens_the_composer() {
        let mut h = Harness::new();
        h.frame(vec![], Modifiers::NONE);
        h.frame(vec![Event::PointerMoved(line_center(1))], Modifiers::NONE);
        h.press(line_center(1), Modifiers::NONE, true);
        assert_eq!(h.body.range.map(|r| (r.first(), r.last())), Some((2, 2)));
        h.frame(vec![Event::PointerMoved(line_center(3))], Modifiers::NONE);
        assert_eq!(h.body.range.map(|r| (r.first(), r.last())), Some((2, 4)));
        assert!(
            h.body.composer.is_none(),
            "the composer waits for the release"
        );
        assert!(h.fills.contains(&tint(0x22)));
        h.press(line_center(3), Modifiers::NONE, false);
        h.frame(vec![], Modifiers::NONE);
        assert_eq!(composer_range(&h), Some((Side::New, 2, 4)));
        assert_eq!(
            kinds(&h.body, &input(&h)),
            ["hunk", "line", "line", "line", "line", "composer", "line"]
        );
    }

    #[test]
    fn dragging_up_selects_the_same_block() {
        let mut h = Harness::new();
        h.frame(vec![], Modifiers::NONE);
        h.drag(
            line_center(3),
            &[line_center(2), line_center(1)],
            Modifiers::NONE,
        );
        assert_eq!(composer_range(&h), Some((Side::New, 2, 4)));
    }

    #[test]
    fn a_single_click_on_a_number_comments_on_one_line() {
        let mut h = Harness::new();
        h.frame(vec![], Modifiers::NONE);
        h.click(line_center(2), Modifiers::NONE);
        assert_eq!(composer_range(&h), Some((Side::New, 3, 3)));
        assert!(!h.fills.contains(&tint(0x22)), "one line is not tinted");
    }

    #[test]
    fn shift_click_extends_from_the_first_line_and_keeps_the_draft() {
        let mut h = Harness::new();
        h.height = 400.0;
        h.frame(vec![], Modifiers::NONE);
        h.click(line_center(1), Modifiers::NONE);
        assert_eq!(composer_range(&h), Some((Side::New, 2, 2)));
        h.body.draft = "keep me".into();
        let below = line_center(3) + vec2(0.0, composer_height(&h));
        h.click(below, Modifiers::SHIFT);
        assert_eq!(composer_range(&h), Some((Side::New, 2, 4)));
        assert_eq!(h.body.draft, "keep me");
        h.click(line_center(0), Modifiers::SHIFT);
        assert_eq!(composer_range(&h), Some((Side::New, 1, 2)));
    }

    #[test]
    fn clicking_a_line_below_an_open_composer_picks_that_line_not_the_one_the_rows_shift_to() {
        let mut h = Harness::new();
        h.height = 400.0;
        h.frame(vec![], Modifiers::NONE);
        h.body.open_composer(Side::New, 2, 2);
        h.frame(vec![], Modifiers::NONE);
        let below = line_center(3) + vec2(0.0, composer_height(&h));
        h.click(below, Modifiers::NONE);
        assert_eq!(composer_range(&h), Some((Side::New, 4, 4)));
    }

    #[test]
    fn a_plain_click_elsewhere_starts_a_new_selection_and_drops_the_draft() {
        let mut h = Harness::new();
        h.frame(vec![], Modifiers::NONE);
        h.drag(line_center(1), &[line_center(3)], Modifiers::NONE);
        h.body.draft = "old".into();
        h.click(line_center(2), Modifiers::NONE);
        assert_eq!(composer_range(&h), Some((Side::New, 3, 3)));
        assert_eq!(h.body.draft, "");
    }

    #[test]
    fn escape_clears_a_selection_in_progress_and_a_cancelled_composer_leaves_none() {
        let mut h = Harness::new();
        h.frame(vec![], Modifiers::NONE);
        h.frame(vec![Event::PointerMoved(line_center(1))], Modifiers::NONE);
        h.press(line_center(1), Modifiers::NONE, true);
        h.frame(vec![Event::PointerMoved(line_center(2))], Modifiers::NONE);
        assert!(h.body.range.is_some());
        h.frame(
            vec![key(egui::Key::Escape, Modifiers::NONE)],
            Modifiers::NONE,
        );
        assert!(h.body.range.is_none());
        h.press(line_center(2), Modifiers::NONE, false);
        h.frame(vec![], Modifiers::NONE);
        assert!(
            h.body.composer.is_none(),
            "the release after Esc opens nothing"
        );
        assert!(!h.fills.contains(&tint(0x22)));

        h.drag(line_center(1), &[line_center(3)], Modifiers::NONE);
        assert!(h.body.composer.is_some());
        h.frame(
            vec![key(egui::Key::Escape, Modifiers::NONE)],
            Modifiers::NONE,
        );
        assert!(h.body.composer.is_none() && h.body.range.is_none());
        h.frame(vec![], Modifiers::NONE);
        assert!(!h.fills.contains(&tint(0x22)));
    }

    #[test]
    fn a_drag_that_leaves_the_hunk_stops_at_its_edge() {
        let mut h = Harness::new();
        let old: String = (1..=30).map(|i| format!("l{i}\n")).collect();
        let new = old
            .replace("l2\n", "two\n")
            .replace("l25\n", "twenty five\n");
        h.diff = diff::build("f.txt", Some(old.as_bytes()), Some(new.as_bytes()));
        h.body = DiffBody::new(&h.diff);
        h.frame(vec![], Modifiers::NONE);
        h.drag(line_center(0), &[pos2(20.0, 900.0)], Modifiers::NONE);
        assert_eq!(composer_range(&h), Some((Side::New, 1, 5)));
    }

    #[test]
    fn a_selection_stays_on_the_side_it_started_on() {
        let mut h = Harness::new();
        h.diff = diff::build("f.txt", Some(b"a\nold\nb\n"), Some(b"a\nnew\nb\n"));
        h.body = DiffBody::new(&h.diff);
        h.frame(vec![], Modifiers::NONE);
        h.drag(
            line_center(1),
            &[line_center(2), line_center(3)],
            Modifiers::NONE,
        );
        assert_eq!(composer_range(&h), Some((Side::Old, 2, 3)));
    }

    #[test]
    fn with_a_select_hint_the_number_column_keeps_staging_and_the_plus_starts_a_range() {
        let mut h = Harness::new();
        h.hint = Some("Select");
        h.frame(vec![], Modifiers::NONE);
        h.click(line_center(2), Modifiers::NONE);
        assert!(h.body.composer.is_none());
        assert_eq!(h.body.selection().len(), 1);
        let plus = |index: usize| pos2(NUM_W * 2.0 + GUTTER_W / 2.0, line_center(index).y);
        h.drag(plus(1), &[plus(3)], Modifiers::NONE);
        assert_eq!(composer_range(&h), Some((Side::New, 2, 4)));
    }

    #[test]
    fn saving_a_range_asks_the_host_for_a_thread_covering_it() {
        let mut h = Harness::new();
        h.body.open_composer(Side::New, 2, 4);
        h.body.draft = "  why these?  ".into();
        h.frame(vec![], Modifiers::NONE);
        let actions = h.cmd_enter();
        let [DiffAction::AddThread { anchor, .. }] = actions.as_slice() else {
            panic!("expected one AddThread, got {actions:?}")
        };
        assert_eq!((anchor.line_hint, anchor.span), (2, 2));
        assert_eq!(anchor.text, "x");
        assert_eq!(anchor.block, ["y", "z"]);
        assert!(h.body.composer.is_none() && h.body.range.is_none());
    }

    #[test]
    fn a_multi_line_thread_sits_under_its_last_line_and_lights_its_lines_while_hovered() {
        let mut h = Harness::new();
        let lines: Vec<&str> = vec!["a", "x", "y", "z", "b"];
        h.threads = vec![thread(5, Anchor::over(&lines, 1, 2, Side::New), false)];
        assert_eq!(
            kinds(&h.body, &input(&h)),
            ["hunk", "line", "line", "line", "line", "thread", "line"]
        );
        h.frame(vec![], Modifiers::NONE);
        assert!(!h.fills.contains(&tint(0x12)));
        let on_card = pos2(THREAD_INDENT + 40.0, 28.0 + 22.0 * 4.0 + 30.0);
        h.frame(vec![Event::PointerMoved(on_card)], Modifiers::NONE);
        h.frame(vec![], Modifiers::NONE);
        h.frame(vec![], Modifiers::NONE);
        assert!(h.fills.contains(&tint(0x12)));
        h.frame(vec![Event::PointerMoved(pos2(900.0, 5.0))], Modifiers::NONE);
        h.frame(vec![], Modifiers::NONE);
        h.frame(vec![], Modifiers::NONE);
        assert!(!h.fills.contains(&tint(0x12)));
    }

    #[test]
    fn a_one_line_thread_never_lights_other_lines() {
        let mut h = Harness::new();
        let lines: Vec<&str> = vec!["a", "x", "y", "z", "b"];
        h.threads = vec![thread(5, Anchor::at(&lines, 1, Side::New), false)];
        h.frame(vec![], Modifiers::NONE);
        let on_card = pos2(THREAD_INDENT + 40.0, 28.0 + 22.0 * 2.0 + 30.0);
        h.frame(vec![Event::PointerMoved(on_card)], Modifiers::NONE);
        h.frame(vec![], Modifiers::NONE);
        h.frame(vec![], Modifiers::NONE);
        assert!(!h.fills.contains(&tint(0x12)));
    }

    #[test]
    fn sending_a_reply_asks_the_host_to_append_it() {
        let mut h = Harness::new();
        let lines: Vec<&str> = vec!["a", "x", "y", "z", "b"];
        h.threads = vec![thread(7, Anchor::at(&lines, 1, Side::New), false)];
        h.frame(vec![], Modifiers::NONE);
        h.body.replies.insert(7, " sure ".into());
        let actions = h.cmd_enter();
        let [DiffAction::Reply { thread, comment }] = actions.as_slice() else {
            panic!("expected one Reply, got {actions:?}")
        };
        assert_eq!(*thread, 7);
        assert_eq!(comment.body, "sure");
        assert!(!h.body.replies.contains_key(&7));
    }

    #[test]
    fn threads_follow_their_lines_and_unplaceable_ones_collect_at_the_top() {
        let mut h = Harness::new();
        let new: Vec<&str> = vec!["a", "x", "y", "z", "b"];
        let gone: Vec<&str> = vec!["nowhere"];
        h.threads = vec![
            thread(1, Anchor::at(&new, 2, Side::New), false),
            thread(2, Anchor::at(&gone, 0, Side::New), false),
        ];
        let items = kinds(&h.body, &input(&h));
        assert_eq!(
            items,
            [
                "outside", "thread", "hunk", "line", "line", "line", "thread", "line", "line"
            ]
        );
        h.threads.clear();
        assert!(!kinds(&h.body, &input(&h)).contains(&"thread"));
    }

    #[test]
    fn threads_of_other_files_are_ignored() {
        let mut h = Harness::new();
        let new: Vec<&str> = vec!["a", "x", "y", "z", "b"];
        let mut other = thread(1, Anchor::at(&new, 2, Side::New), false);
        other.path = "elsewhere.txt".into();
        h.threads = vec![other];
        assert!(!kinds(&h.body, &input(&h)).contains(&"thread"));
    }

    #[test]
    fn split_layout_pairs_removed_and_added_lines_side_by_side() {
        let mut h = Harness::new();
        h.diff = diff::build("f.txt", Some(b"a\nold1\nold2\nb\n"), Some(b"a\nnew1\nb\n"));
        h.body = DiffBody::new(&h.diff);
        h.body.set_layout(DiffLayout::Split);
        assert_eq!(
            kinds(&h.body, &input(&h)),
            ["hunk", "split", "split", "split", "split"]
        );
        h.body.set_layout(DiffLayout::Unified);
        assert_eq!(
            kinds(&h.body, &input(&h)),
            ["hunk", "line", "line", "line", "line", "line"]
        );
    }

    #[test]
    fn embedded_tints_color_the_changed_rows_in_both_layouts() {
        let mut h = Harness::new();
        h.diff = diff::build("f.txt", Some(b"a\nold\nb\n"), Some(b"a\nnew\nb\n"));
        h.body = DiffBody::new(&h.diff);
        h.style = DiffStyle::embedded(Color32::from_rgb(0x2d, 0xd4, 0xbf));
        let added = h.style.look(LineKind::Added).fill;
        let removed = h.style.look(LineKind::Removed).fill;
        for layout in [DiffLayout::Unified, DiffLayout::Split] {
            h.body.set_layout(layout);
            h.frame(vec![], Modifiers::NONE);
            assert!(h.fills.contains(&added), "{layout:?} added");
            assert!(h.fills.contains(&removed), "{layout:?} removed");
            assert!(!h.fills.contains(&theme::added_bg()), "{layout:?}");
            assert!(!h.fills.contains(&theme::removed_bg()), "{layout:?}");
            assert!(!h.fills.contains(&theme::hunk_bg()), "{layout:?}");
        }
        h.style = DiffStyle::default();
        for layout in [DiffLayout::Unified, DiffLayout::Split] {
            h.body.set_layout(layout);
            h.frame(vec![], Modifiers::NONE);
            assert!(h.fills.contains(&theme::added_bg()), "{layout:?}");
            assert!(h.fills.contains(&theme::removed_bg()), "{layout:?}");
            assert!(h.fills.contains(&theme::hunk_bg()), "{layout:?}");
        }
    }

    #[test]
    fn file_mode_lists_every_line_of_the_new_text() {
        let mut h = Harness::new();
        h.mode = DiffMode::File;
        assert_eq!(kinds(&h.body, &input(&h)), ["file-line"; 5]);
    }

    #[test]
    fn reloading_keeps_the_selection_only_while_the_lines_are_the_same() {
        let mut h = Harness::new();
        h.body.toggle_line(2, false);
        h.body.reload(&sample());
        assert_eq!(h.body.selection().len(), 1);
        h.body
            .reload(&diff::build("f.txt", Some(b"a\n"), Some(b"a\nq\n")));
        assert_eq!(h.body.selection().len(), 0);
    }

    #[test]
    fn jumping_needs_a_hunk_in_diff_mode_and_a_changed_line_in_file_mode() {
        let body = DiffBody::new(&sample());
        assert!(body.has_changes_to_jump_to(DiffMode::Diff));
        assert!(body.has_changes_to_jump_to(DiffMode::File));
        let same = diff::build("f.txt", Some(b"a\n"), Some(b"a\n"));
        let body = DiffBody::new(&same);
        assert!(!body.has_changes_to_jump_to(DiffMode::Diff));
    }

    #[test]
    fn alt_down_and_up_scroll_between_hunks() {
        let mut h = Harness::new();
        let long: String = (1..=120).map(|i| format!("line {i}\n")).collect();
        let edited = long
            .replace("line 2\n", "line two\n")
            .replace("line 110\n", "line one hundred ten\n");
        h.diff = diff::build("f.txt", Some(long.as_bytes()), Some(edited.as_bytes()));
        h.body = DiffBody::new(&h.diff);
        let alt = |key| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::ALT,
        };
        h.frame(vec![], Modifiers::NONE);
        assert_eq!(h.body.scroll_y(), 0.0);
        h.body.jump_to_change(true);
        h.frame(vec![alt(egui::Key::ArrowDown)], Modifiers::ALT);
        h.frame(vec![], Modifiers::NONE);

        assert!(
            h.body.scroll_y() > 0.0,
            "jumping did not reach the second hunk"
        );
        h.body.jump_to_change(false);
        h.frame(vec![], Modifiers::NONE);
        h.frame(vec![], Modifiers::NONE);
        assert_eq!(h.body.scroll_y(), 0.0);
    }

    #[test]
    fn the_thread_card_is_640_wide_when_there_is_room_and_shrinks_with_the_pane() {
        assert_eq!(thread_card_width(1200.0), THREAD_W);
        assert_eq!(
            thread_card_width(THREAD_W + THREAD_INDENT + THREAD_MARGIN),
            THREAD_W
        );
        assert_eq!(
            thread_card_width(500.0),
            500.0 - THREAD_INDENT - THREAD_MARGIN
        );
        assert_eq!(thread_card_width(120.0), THREAD_MIN_W);
    }

    fn card_in(width: f32, h: &Harness) -> (f32, f32) {
        let mut seen = (0.0, 0.0);
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 400.0))),
            ..Default::default()
        };
        let _ = h.ctx.run_ui(raw, |ui| {
            let rows = h.body.rows(ui, &input(h));
            let thread = rows
                .items
                .iter()
                .position(|i| matches!(i, Item::Thread(_)))
                .expect("a thread row");
            seen = (rows.card_w, rows.heights[thread]);
        });
        seen
    }

    #[test]
    fn narrow_panes_get_a_narrower_card_whose_comments_wrap_taller() {
        let mut h = Harness::new();
        let lines: Vec<&str> = vec!["a", "x", "y", "z", "b"];
        let mut long = thread(1, Anchor::at(&lines, 1, Side::New), false);
        long.comments[0].body = "word ".repeat(60);
        h.threads = vec![long];
        let (wide_w, wide_h) = card_in(1200.0, &h);
        let (narrow_w, narrow_h) = card_in(420.0, &h);
        assert_eq!(wide_w, THREAD_W);
        assert!(narrow_w < 300.0, "card is {narrow_w} wide in a 420 pane");
        assert!(narrow_h > wide_h);
    }

    #[test]
    fn the_default_texts_are_the_wording_kelp_always_used() {
        let texts = DiffTexts::default();
        assert_eq!(texts.saved_hint, "Saved in this repo only · not pushed");
        let reply = if cfg!(target_os = "macos") {
            "Reply…  (⌘ Enter to send)"
        } else {
            "Reply…  (Ctrl+Enter to send)"
        };
        assert_eq!(texts.reply_hint, reply);
        assert_eq!(texts.comment_hint, "Leave a comment");
    }
}
