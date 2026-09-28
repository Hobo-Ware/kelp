use std::cell::Cell;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use eframe::egui::{
    self, Align2, Color32, CursorIcon, FontId, Key, Margin, Modifiers, Rect, RichText, Sense,
    Stroke, Ui, pos2, vec2,
};
use gix::ObjectId;
use kelp_core::diff::{self, Body, FileDiff, Line, LineKind};
use kelp_core::ops::Op;
use kelp_core::review::{Anchor, Comment, Review, Side, Thread};

use crate::avatars::AvatarStore;
use crate::blame_view::{self, BlameView};
use crate::dialogs::Dialog;
use crate::preview_view::Preview;
use crate::{theme, widgets};

thread_local! {
    static ASKED_BEFORE_DISCARD: Cell<bool> = const { Cell::new(false) };
}

const LINE_H: f32 = 22.0;
const HUNK_H: f32 = 28.0;
const DISCARD_W: f32 = 76.0;
const NUM_W: f32 = 46.0;
const GUTTER_W: f32 = 24.0;
const HEADER_H: f32 = 48.0;
const TITLE_STATS_W: f32 = 150.0;
const TITLE_TOO_NARROW: f32 = 120.0;
const TITLE_ROOMY: f32 = 320.0;
const THREAD_INDENT: f32 = NUM_W * 2.0 + GUTTER_W + 10.0;
const THREAD_W: f32 = 640.0;
const RESOLVED_H: f32 = 34.0;
const COMPOSER_H: f32 = 132.0;
const ADDED_BG: Color32 = Color32::from_rgb(0x16, 0x30, 0x2a);
const REMOVED_BG: Color32 = Color32::from_rgb(0x3a, 0x1f, 0x22);
const ADDED_EMPHASIS: Color32 = Color32::from_rgb(0x22, 0x55, 0x44);
const REMOVED_EMPHASIS: Color32 = Color32::from_rgb(0x66, 0x2d, 0x33);
const NUM_COLOR: Color32 = Color32::from_rgb(0x5e, 0x65, 0x73);
const CONTEXT_TEXT: Color32 = Color32::from_rgb(0xb4, 0xb9, 0xc2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSource {
    Commit(ObjectId),
    Unstaged,
    Staged,
    File(ObjectId),
    Range(ObjectId, Option<ObjectId>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Diff,
    File,
    Preview,
    Blame,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    #[default]
    Unified,
    Split,
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

pub struct DiffView {
    pub source: DiffSource,
    pub diff: FileDiff,
    old_path: Option<String>,
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
    header_squeeze: u8,
    embedded: bool,
    ask: Option<Dialog>,
    preview: Option<Preview>,
    file_change_starts: Vec<u32>,
    selected_lines: BTreeSet<usize>,
    select_anchor: Option<usize>,
    scroll_y: f32,
    jump: Option<Jump>,
    blame_from: Option<(PathBuf, Option<ObjectId>)>,
    blame: Option<BlameView>,
}

pub enum Event {
    None,
    Close,
    Changed,
    Run(Op),
    Ask(Dialog),
    OpenInEditor,
    FileHistory,
    Reveal(ObjectId),
}

impl DiffView {
    pub fn load(
        repo: &gix::Repository,
        workdir: Option<&Path>,
        source: DiffSource,
        path: &str,
    ) -> anyhow::Result<Self> {
        Self::load_moved(repo, workdir, source, path, None)
    }

    pub fn load_moved(
        repo: &gix::Repository,
        workdir: Option<&Path>,
        source: DiffSource,
        path: &str,
        old_path: Option<&str>,
    ) -> anyhow::Result<Self> {
        let diff = match (source, workdir) {
            (DiffSource::Range(base, target), _) => {
                diff::range_file(repo, workdir, base, target, path, old_path)?
            }
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
        let read = |repo_path: &str| -> Option<Vec<u8>> {
            match source {
                DiffSource::Commit(id) | DiffSource::File(id) => {
                    diff::file_at(repo, id, repo_path).ok().flatten()
                }
                DiffSource::Range(_, Some(id)) => diff::file_at(repo, id, repo_path).ok().flatten(),
                DiffSource::Staged => diff::index_blob(repo, repo_path).ok().flatten(),
                DiffSource::Unstaged | DiffSource::Range(_, None) => {
                    workdir.and_then(|w| std::fs::read(w.join(repo_path)).ok())
                }
            }
        };
        let preview = diff
            .preview
            .as_ref()
            .map(|sides| Preview::new(sides, path, &read));
        let file_change_starts = change_starts(&lines);
        let has_lines = diff.new_text.as_deref().is_some_and(|t| !t.is_empty());
        let blame_from = match (source, workdir) {
            (DiffSource::Commit(id) | DiffSource::File(id), _) if has_lines => Some((
                workdir.map_or_else(|| repo.path().to_path_buf(), Path::to_path_buf),
                Some(id),
            )),
            (DiffSource::Unstaged, Some(workdir)) if has_lines => {
                Some((workdir.to_path_buf(), None))
            }
            _ => None,
        };
        let text_body = matches!(diff.body, Body::Text(_));
        let mode = match (preview.is_some(), source) {
            (true, _) if !text_body => Mode::Preview,
            (true, DiffSource::File(_)) => Mode::Preview,
            (_, DiffSource::File(_)) => Mode::File,
            _ => Mode::Diff,
        };
        Ok(Self {
            source,
            old_path: old_path.map(str::to_string),
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
            header_squeeze: 0,
            embedded: false,
            ask: None,
            preview,
            file_change_starts,
            selected_lines: BTreeSet::new(),
            select_anchor: None,
            scroll_y: 0.0,
            jump: None,
            blame_from,
            blame: None,
        })
    }

    pub fn path(&self) -> &str {
        &self.diff.path
    }

    pub fn is_staged(&self) -> bool {
        self.source == DiffSource::Staged
    }

    pub fn is_working(&self) -> bool {
        matches!(
            self.source,
            DiffSource::Unstaged | DiffSource::Staged | DiffSource::Range(_, None)
        )
    }

    pub fn reload(&mut self, repo: &gix::Repository, workdir: Option<&Path>) -> anyhow::Result<()> {
        let fresh = Self::load_moved(
            repo,
            workdir,
            self.source,
            &self.diff.path.clone(),
            self.old_path.as_deref(),
        )?;
        let kept = std::mem::replace(self, fresh);
        self.mode = kept.mode;
        self.layout = kept.layout;
        self.composer = kept.composer;
        self.draft = kept.draft;
        self.replies = kept.replies;
        self.expanded = kept.expanded;
        self.header_squeeze = kept.header_squeeze;
        self.embedded = kept.embedded;
        self.scroll_y = kept.scroll_y;
        self.blame = kept.blame;
        if self.is_working()
            && let Some(blame) = &mut self.blame
        {
            blame.restart();
        }
        if same_lines(&kept.lines, &self.lines) {
            self.selected_lines = kept.selected_lines;
            self.select_anchor = kept.select_anchor;
        }
        if let (Some(old), Some(sides)) = (kept.preview, &self.diff.preview)
            && old.same_content(sides)
        {
            self.preview = Some(old);
        }
        if self.mode == Mode::Preview && self.preview.is_none() {
            self.mode = Mode::Diff;
        }
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

    fn lines_selectable(&self) -> bool {
        self.mode == Mode::Diff && self.hunk_action().is_some()
    }

    fn jump_targets_exist(&self) -> bool {
        match self.mode {
            Mode::Diff => !self.hunks.is_empty(),
            Mode::File => !self.file_change_starts.is_empty(),
            Mode::Preview | Mode::Blame => false,
        }
    }

    fn stage_selected_lines(&mut self) {
        let staged = self.source == DiffSource::Staged;
        if let Some(patch) = diff::lines_patch(&self.diff, &self.selected_lines, staged) {
            self.pending = Some(Op::ApplyToIndex {
                patch,
                reverse: staged,
            });
        }
        self.selected_lines.clear();
        self.select_anchor = None;
    }

    fn can_discard(&self) -> bool {
        self.source == DiffSource::Unstaged && self.lines_selectable()
    }

    fn discard_selected_lines(&mut self, skip_confirm: bool) {
        let count = self.selected_lines.len();
        if let Some(patch) = diff::discard_lines_patch(&self.diff, &self.selected_lines) {
            let what = format!("{count} line{}", if count == 1 { "" } else { "s" });
            self.discard(patch, &what, skip_confirm);
        }
        self.selected_lines.clear();
        self.select_anchor = None;
    }

    fn discard_hunk(&mut self, hunk: usize, skip_confirm: bool) {
        if let Some(patch) = diff::hunk_patch(&self.diff, hunk) {
            self.discard(patch, "this hunk", skip_confirm);
        }
    }

    fn discard(&mut self, patch: String, what: &str, skip_confirm: bool) {
        let op = Op::DiscardPatch(patch);
        if skip_confirm || ASKED_BEFORE_DISCARD.replace(true) {
            self.pending = Some(op);
            return;
        }
        self.ask = Some(Dialog::Confirm {
            title: format!("Discard {what}?"),
            body: format!(
                "The change goes away from {} on disk. Cmd+Z brings it back. \
                 Kelp won't ask again until it restarts, and Shift-click skips this.",
                self.diff.path
            ),
            op,
            danger: true,
        });
    }

    fn toggle_line(&mut self, index: usize, extend: bool) {
        match self.select_anchor.filter(|_| extend) {
            Some(anchor) => {
                let (from, to) = (anchor.min(index), anchor.max(index));
                let changed = (from..=to).filter(|&i| self.lines[i].1.kind != LineKind::Context);
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

    pub fn layout(&self) -> Layout {
        self.layout
    }

    pub fn embedded(mut self) -> Self {
        self.embedded = true;
        self
    }

    pub fn show_preview(&mut self) {
        if self.preview.is_some() {
            self.mode = Mode::Preview;
        }
    }

    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
    }

    fn commit(&self) -> Option<String> {
        match self.source {
            DiffSource::Commit(id) | DiffSource::File(id) | DiffSource::Range(_, Some(id)) => {
                Some(id.to_string())
            }
            DiffSource::Unstaged | DiffSource::Staged | DiffSource::Range(_, None) => None,
        }
    }

    pub fn show_blame(&mut self) {
        if self.blame_from.is_some() {
            self.mode = Mode::Blame;
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        review: &mut Review,
        author: &str,
        avatars: &mut AvatarStore,
    ) -> Event {
        if let Some(text) = &self.diff.new_text {
            crate::fonts::ensure_fallback(ui.ctx(), text);
        }
        if !ui.ctx().egui_wants_keyboard_input() {
            if ui.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowUp)) {
                self.jump = Some(Jump::Previous);
            }
            if ui.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowDown)) {
                self.jump = Some(Jump::Next);
            }
        }
        let mut event = self.header(ui, review);
        if self.mode == Mode::Preview
            && let Some(preview) = &mut self.preview
        {
            preview.ui(ui);
            return event;
        }
        if self.mode == Mode::Blame
            && let Some((dir, rev)) = &self.blame_from
        {
            let path = self.diff.path.clone();
            let blame = self
                .blame
                .get_or_insert_with(|| BlameView::new(ui.ctx(), dir.clone(), *rev, &path));
            if let blame_view::Event::Reveal(id) = blame.ui(ui, avatars) {
                event = Event::Reveal(id);
            }
            return event;
        }
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
                if let Some(dialog) = self.ask.take() {
                    event = Event::Ask(dialog);
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
                let row = ui.available_rect_before_wrap();
                let controls = ui
                    .scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(row)
                            .layout(egui::Layout::right_to_left(egui::Align::Center)),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 12.0;
                            if !self.embedded && widgets::close_button(ui, "Close (Esc)") {
                                event = Event::Close;
                            }
                            if widgets::icon_button(
                                ui,
                                crate::icons::Icon::Pencil,
                                "Open in editor",
                            ) {
                                event = Event::OpenInEditor;
                            }
                            if !self.embedded
                                && widgets::icon_button(
                                    ui,
                                    crate::icons::Icon::Clock,
                                    "File history",
                                )
                            {
                                event = Event::FileHistory;
                            }
                            ui.add_space(4.0);
                            if self.mode == Mode::Diff && self.header_squeeze < 2 {
                                widgets::segmented(
                                    ui,
                                    &mut self.layout,
                                    &[(Layout::Split, "Split"), (Layout::Unified, "Unified")],
                                );
                            }
                            let mut modes = Vec::new();
                            if self.preview.is_some() {
                                modes.push((Mode::Preview, "Preview"));
                            }
                            if matches!(self.diff.body, Body::Text(_)) {
                                modes.push((Mode::File, "File"));
                                if !matches!(self.source, DiffSource::File(_)) {
                                    modes.push((Mode::Diff, "Diff"));
                                }
                                if self.blame_from.is_some() {
                                    modes.push((Mode::Blame, "Blame"));
                                }
                            }
                            if modes.len() > 1 {
                                widgets::segmented(ui, &mut self.mode, &modes);
                            }
                            if self.jump_targets_exist() && self.header_squeeze < 1 {
                                ui.add_space(4.0);
                                if arrow_button(ui, false, "Next change (⌥↓)") {
                                    self.jump = Some(Jump::Next);
                                }
                                if arrow_button(ui, true, "Previous change (⌥↑)") {
                                    self.jump = Some(Jump::Previous);
                                }
                            }
                            if self.lines_selectable() && !self.selected_lines.is_empty() {
                                ui.add_space(4.0);
                                let count = self.selected_lines.len();
                                let verb = if self.is_staged() { "Unstage" } else { "Stage" };
                                let label = format!(
                                    "{verb} {count} line{}",
                                    if count == 1 { "" } else { "s" }
                                );
                                let button = egui::Button::new(
                                    RichText::new(label)
                                        .size(12.0)
                                        .family(theme::semibold())
                                        .color(Color32::from_rgb(0x10, 0x13, 0x1a)),
                                )
                                .fill(theme::ACCENT)
                                .corner_radius(5);
                                if ui.add(button).clicked() {
                                    self.stage_selected_lines();
                                }
                                if self.can_discard() {
                                    let discard = egui::Button::new(
                                        RichText::new(format!(
                                            "Discard {count} line{}",
                                            if count == 1 { "" } else { "s" }
                                        ))
                                        .size(12.0)
                                        .color(theme::DELETED),
                                    )
                                    .fill(theme::with_alpha(theme::DELETED, 0x1c))
                                    .stroke(Stroke::new(
                                        1.0,
                                        theme::with_alpha(theme::DELETED, 0x66),
                                    ))
                                    .corner_radius(5);
                                    let response = ui.add(discard).on_hover_text(
                                    "Undo these lines in the file. Shift-click skips the question.",
                                );
                                    if response.clicked() {
                                        let shift = ui.input(|i| i.modifiers.shift);
                                        self.discard_selected_lines(shift);
                                    }
                                }
                                let clear = egui::Button::new(
                                    RichText::new("Clear").size(12.0).color(theme::TEXT_MUTED),
                                )
                                .frame(false);
                                if ui.add(clear).clicked() {
                                    self.selected_lines.clear();
                                    self.select_anchor = None;
                                }
                            }
                        },
                    )
                    .response
                    .rect;
                let title = Rect::from_min_max(row.min, pos2(controls.left() - 12.0, row.max.y));
                let squeeze = next_squeeze(self.header_squeeze, title.width());
                if squeeze != self.header_squeeze {
                    self.header_squeeze = squeeze;
                    ui.ctx().request_repaint();
                }
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(title)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| {
                        ui.set_clip_rect(title.intersect(ui.clip_rect()));
                        ui.spacing_mut().item_spacing.x = 12.0;
                        let path = self.path().to_string();
                        let (dir, name) = path
                            .rsplit_once('/')
                            .map_or(("", path.as_str()), |(d, n)| (d, n));
                        let width_of = |text: String, family| {
                            ui.painter()
                                .layout_no_wrap(text, FontId::new(13.0, family), Color32::WHITE)
                                .size()
                                .x
                        };
                        let dir_fits = width_of(format!("{dir}/"), egui::FontFamily::Monospace)
                            + width_of(name.to_string(), theme::semibold())
                            + TITLE_STATS_W
                            <= ui.available_width();
                        if !dir.is_empty() && dir_fits {
                            ui.label(
                                RichText::new(format!("{dir}/"))
                                    .monospace()
                                    .color(theme::TEXT_FAINT),
                            );
                            ui.add_space(-12.0);
                        }
                        ui.add(
                            egui::Label::new(
                                RichText::new(name)
                                    .monospace()
                                    .family(theme::semibold())
                                    .color(theme::TEXT_STRONG),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&path);
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
                            DiffSource::Range(base, target) => format!(
                                "{}..{}",
                                base.to_hex_with_len(7),
                                target.map_or("working tree".to_string(), |t| t
                                    .to_hex_with_len(7)
                                    .to_string())
                            ),
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
                    },
                );
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
            Mode::Preview | Mode::Blame => {}
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
        let mut scroll = egui::ScrollArea::both().auto_shrink(false);
        if let Some(jump) = self.jump.take()
            && let Some(target) = self.jump_target(&items, &offsets, jump)
        {
            scroll = scroll.vertical_scroll_offset(target);
        }
        scroll.show_viewport(ui, |ui, viewport| {
            self.scroll_y = viewport.min.y;
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
                let rect = Rect::from_min_size(origin + vec2(0.0, top), vec2(width, heights[k]));
                match *item {
                    Item::Hunk(h) => {
                        ui.painter()
                            .rect_filled(rect, 0.0, Color32::from_rgb(0x1a, 0x22, 0x30));
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
                            if self.can_discard() {
                                let discard_rect = Rect::from_min_size(
                                    button_rect.min - vec2(DISCARD_W + 8.0, 0.0),
                                    vec2(DISCARD_W, button_rect.height()),
                                );
                                let discard = egui::Button::new(
                                    RichText::new("Discard").size(12.0).color(theme::DELETED),
                                )
                                .corner_radius(5)
                                .fill(theme::with_alpha(theme::DELETED, 0x1c));
                                let response = ui.put(discard_rect, discard).on_hover_text(
                                    "Undo this hunk in the file. Shift-click skips the question.",
                                );
                                if response.clicked() {
                                    let shift = ui.input(|i| i.modifiers.shift);
                                    self.discard_hunk(h, shift);
                                }
                            }
                        }
                    }
                    Item::OutsideHeader => {
                        ui.painter()
                            .rect_filled(rect, 0.0, Color32::from_rgb(0x1f, 0x23, 0x2b));
                        ui.painter().text(
                            pos2(rect.left() + 16.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            "Comments on lines not shown here",
                            FontId::proportional(12.0),
                            theme::TEXT_MUTED,
                        );
                    }
                    Item::Line(i) => self.unified_line(ui, rect, i, &font),
                    Item::Split(l, r) => {
                        let half = rect.width() / 2.0;
                        let left = Rect::from_min_size(rect.min, vec2(half, rect.height()));
                        let right = Rect::from_min_size(
                            rect.min + vec2(half, 0.0),
                            vec2(half, rect.height()),
                        );
                        self.split_side(ui, left, l, Side::Old, &font);
                        self.split_side(ui, right, r, Side::New, &font);
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
                            Row {
                                numbers: Numbers::One(Some(number)),
                                mark: "",
                                text: &text,
                                emphasis: &[],
                                bg: Color32::TRANSPARENT,
                                mark_color: theme::TEXT_FAINT,
                                text_color: CONTEXT_TEXT,
                                side: Side::New,
                                selectable: None,
                            },
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

    fn jump_target(&self, items: &[Item], offsets: &[f32], jump: Jump) -> Option<f32> {
        let targets = items.iter().zip(offsets).filter_map(|(item, &top)| {
            let is_start = match *item {
                Item::Hunk(_) => true,
                Item::FileLine(i) => self.file_change_starts.contains(&(i as u32 + 1)),
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

    fn unified_line(&mut self, ui: &mut Ui, rect: Rect, index: usize, font: &FontId) {
        let line = self.lines[index].1.clone();
        let (bg, mark, mark_color) = style(line.kind);
        let side = if line.kind == LineKind::Removed {
            Side::Old
        } else {
            Side::New
        };
        let row = Row {
            numbers: Numbers::Both(line.old, line.new),
            mark,
            text: &line.text,
            emphasis: &line.emphasis,
            bg,
            mark_color,
            text_color: text_color(line.kind),
            side,
            selectable: self.selectable(index),
        };
        self.code_row(ui, rect, row, font);
    }

    fn split_side(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        index: Option<usize>,
        side: Side,
        font: &FontId,
    ) {
        let Some(index) = index else {
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_rgb(0x18, 0x1b, 0x21));
            return;
        };
        let line = self.lines[index].1.clone();
        let (bg, mark, mark_color) = style(line.kind);
        let number = if side == Side::Old {
            line.old
        } else {
            line.new
        };
        let row = Row {
            numbers: Numbers::One(number),
            mark,
            text: &line.text,
            emphasis: &line.emphasis,
            bg,
            mark_color,
            text_color: text_color(line.kind),
            side,
            selectable: self.selectable(index),
        };
        self.code_row(ui, rect, row, font);
    }

    fn selectable(&self, index: usize) -> Option<usize> {
        (self.lines_selectable() && self.lines[index].1.kind != LineKind::Context).then_some(index)
    }

    fn code_row(&mut self, ui: &mut Ui, rect: Rect, row: Row, font: &FontId) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, row.bg);
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
                painter.rect_filled(numbers_rect, 0.0, theme::with_alpha(theme::ACCENT, 0x38));
                painter.rect_filled(
                    Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
                    0.0,
                    theme::ACCENT,
                );
            } else if response.hovered() {
                painter.rect_filled(numbers_rect, 0.0, theme::with_alpha(Color32::WHITE, 0x0c));
            }
            let hint = if self.is_staged() {
                "Select to unstage (Shift-click for a range)"
            } else {
                "Select to stage (Shift-click for a range)"
            };
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
                    NUM_COLOR,
                );
                painter.text(
                    pos2(rect.left() + NUM_W * 2.0 - 8.0, mid),
                    Align2::RIGHT_CENTER,
                    num(new),
                    font.clone(),
                    NUM_COLOR,
                );
                if row.side == Side::Old { old } else { new }
            }
            Numbers::One(n) => {
                painter.text(
                    pos2(rect.left() + NUM_W - 8.0, mid),
                    Align2::RIGHT_CENTER,
                    num(n),
                    font.clone(),
                    NUM_COLOR,
                );
                n
            }
        };
        let side = row.side;
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
            row.mark,
            font.clone(),
            row.mark_color,
        );
        let galley = painter.layout_no_wrap(row.text.to_string(), font.clone(), row.text_color);
        let origin = pos2(gutter.right() + 20.0, mid - galley.size().y / 2.0);
        let emphasis_fill = if row.bg == REMOVED_BG {
            REMOVED_EMPHASIS
        } else {
            ADDED_EMPHASIS
        };
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
            painter.rect_filled(span, 3.0, emphasis_fill);
        }
        painter.galley(origin, galley, row.text_color);
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

struct Row<'a> {
    numbers: Numbers,
    mark: &'static str,
    text: &'a str,
    emphasis: &'a [(usize, usize)],
    bg: Color32,
    mark_color: Color32,
    text_color: Color32,
    side: Side,
    selectable: Option<usize>,
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

fn text_color(kind: LineKind) -> Color32 {
    if kind == LineKind::Context {
        CONTEXT_TEXT
    } else {
        theme::TEXT
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

fn arrow_button(ui: &mut Ui, up: bool, hint: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    let response = response
        .on_hover_text(hint)
        .on_hover_cursor(CursorIcon::PointingHand);
    let hovered = response.hovered();
    if hovered {
        ui.painter().rect_filled(rect, 5.0, theme::CONTROL_HOVER);
    }
    let color = if hovered {
        theme::TEXT_STRONG
    } else {
        theme::TEXT_MUTED
    };
    let c = rect.center();
    let dy = if up { -2.5 } else { 2.5 };
    let stroke = Stroke::new(1.5, color);
    ui.painter()
        .line_segment([c + vec2(-4.5, -dy), c + vec2(0.0, dy)], stroke);
    ui.painter()
        .line_segment([c + vec2(0.0, dy), c + vec2(4.5, -dy)], stroke);
    response.clicked()
}

fn style(kind: LineKind) -> (Color32, &'static str, Color32) {
    match kind {
        LineKind::Added => (ADDED_BG, "+", theme::ADDED),
        LineKind::Removed => (REMOVED_BG, "-", theme::DELETED),
        LineKind::Context => (Color32::TRANSPARENT, "", theme::TEXT_FAINT),
    }
}

fn next_squeeze(level: u8, title_w: f32) -> u8 {
    if title_w < TITLE_TOO_NARROW {
        (level + 1).min(2)
    } else if title_w > TITLE_ROOMY {
        level.saturating_sub(1)
    } else {
        level
    }
}

fn notice(ui: &mut Ui, text: &str) {
    ui.centered_and_justified(|ui| ui.label(RichText::new(text).color(theme::TEXT_MUTED)));
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    #[test]
    fn header_controls_step_aside_one_at_a_time_and_come_back() {
        assert_eq!(super::next_squeeze(0, 80.0), 1);
        assert_eq!(super::next_squeeze(1, 80.0), 2);
        assert_eq!(super::next_squeeze(2, 80.0), 2);
        assert_eq!(super::next_squeeze(2, 200.0), 2);
        assert_eq!(super::next_squeeze(2, 400.0), 1);
        assert_eq!(super::next_squeeze(0, 400.0), 0);
    }

    use std::path::PathBuf;

    use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
    use kelp_core::git_cli::run;
    use kelp_core::ops::Op;
    use kelp_core::review::Review;

    use crate::dialogs::Dialog;

    use super::{DiffSource, DiffView, HEADER_H, HUNK_H, LINE_H};

    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("kelp-ui-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.email", "t@example.com"],
            &["config", "user.name", "T"],
        ] {
            run(&dir, args).unwrap();
        }
        std::fs::write(dir.join("f.txt"), "a\nb\n").unwrap();
        run(&dir, &["add", "."]).unwrap();
        run(&dir, &["commit", "-q", "-m", "base"]).unwrap();
        std::fs::write(dir.join("f.txt"), "a\nx\ny\nz\nb\n").unwrap();
        Scratch(dir)
    }

    fn frame(ctx: &egui::Context, view: &mut DiffView, events: Vec<Event>, modifiers: Modifiers) {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 300.0))),
            events,
            modifiers,
            ..Default::default()
        };
        let mut review = Review::default();
        let mut avatars = crate::avatars::AvatarStore::new(ctx.clone(), None);
        avatars.enabled = false;
        let _ = ctx.run_ui(input, |ui| {
            view.ui(ui, &mut review, "T", &mut avatars);
        });
    }

    fn click(ctx: &egui::Context, view: &mut DiffView, at: Pos2, modifiers: Modifiers) {
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        };
        frame(ctx, view, vec![Event::PointerMoved(at)], modifiers);
        frame(ctx, view, vec![press(true)], modifiers);
        frame(ctx, view, vec![press(false)], modifiers);
        frame(ctx, view, vec![], Modifiers::NONE);
    }

    fn line_center(index: usize) -> Pos2 {
        let spacing = egui::Style::default().spacing.item_spacing.y;
        let top = HEADER_H + spacing + HUNK_H + index as f32 * LINE_H;
        pos2(20.0, top + LINE_H / 2.0)
    }

    #[test]
    fn discarding_asks_once_then_runs_and_shift_skips_the_question() {
        let repo = scratch("discard");
        let git = gix::open(&repo.0).unwrap();
        let mut view = DiffView::load(&git, Some(&repo.0), DiffSource::Unstaged, "f.txt").unwrap();
        assert!(view.can_discard());

        view.selected_lines.insert(2);
        view.discard_selected_lines(false);
        let Some(Dialog::Confirm {
            op, danger: true, ..
        }) = view.ask.take()
        else {
            panic!("the first discard should ask")
        };
        assert!(view.pending.is_none());
        op.run(&repo.0).unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.0.join("f.txt")).unwrap(),
            "a\nx\nz\nb\n"
        );

        let git = gix::open(&repo.0).unwrap();
        let mut view = DiffView::load(&git, Some(&repo.0), DiffSource::Unstaged, "f.txt").unwrap();
        view.discard_hunk(0, false);
        assert!(view.ask.is_none(), "asked only once per session");
        let Some(op @ Op::DiscardPatch(_)) = view.pending.take() else {
            panic!("expected a discard patch")
        };
        op.run(&repo.0).unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.0.join("f.txt")).unwrap(),
            "a\nb\n"
        );
    }

    #[test]
    fn shift_click_discards_without_asking_and_staged_diffs_cannot_discard() {
        let repo = scratch("discard-shift");
        let git = gix::open(&repo.0).unwrap();
        let mut view = DiffView::load(&git, Some(&repo.0), DiffSource::Unstaged, "f.txt").unwrap();
        view.discard_hunk(0, true);
        assert!(view.ask.is_none());
        assert!(matches!(view.pending, Some(Op::DiscardPatch(_))));

        run(&repo.0, &["add", "f.txt"]).unwrap();
        let git = gix::open(&repo.0).unwrap();
        let staged = DiffView::load(&git, Some(&repo.0), DiffSource::Staged, "f.txt").unwrap();
        assert!(!staged.can_discard());
    }

    #[test]
    fn click_and_shift_click_select_lines_then_stage_them() {
        let repo = scratch("select");
        let git = gix::open(&repo.0).unwrap();
        let mut view = DiffView::load(&git, Some(&repo.0), DiffSource::Unstaged, "f.txt").unwrap();
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        frame(&ctx, &mut view, vec![], Modifiers::NONE);

        click(&ctx, &mut view, line_center(2), Modifiers::NONE);
        assert_eq!(view.selected_lines.iter().copied().collect::<Vec<_>>(), [2]);
        click(&ctx, &mut view, line_center(3), Modifiers::SHIFT);
        assert_eq!(
            view.selected_lines.iter().copied().collect::<Vec<_>>(),
            [2, 3]
        );
        click(&ctx, &mut view, line_center(0), Modifiers::NONE);
        assert_eq!(
            view.selected_lines.len(),
            2,
            "context lines are not selectable"
        );

        view.stage_selected_lines();
        let Some(op @ Op::ApplyToIndex { reverse: false, .. }) = view.pending.take() else {
            panic!("expected a forward index patch")
        };
        op.run(&repo.0).unwrap();
        assert_eq!(run(&repo.0, &["show", ":f.txt"]).unwrap(), "a\ny\nz\nb\n");
        assert!(view.selected_lines.is_empty());
    }

    #[test]
    fn alt_down_and_up_jump_between_hunks() {
        let repo = scratch("jump");
        let long: String = (1..=120).map(|i| format!("line {i}\n")).collect();
        std::fs::write(repo.0.join("f.txt"), &long).unwrap();
        run(&repo.0, &["commit", "-qam", "long"]).unwrap();
        let edited = long
            .replace("line 2\n", "line two\n")
            .replace("line 110\n", "line one hundred ten\n");
        std::fs::write(repo.0.join("f.txt"), edited).unwrap();
        let git = gix::open(&repo.0).unwrap();
        let mut view = DiffView::load(&git, Some(&repo.0), DiffSource::Unstaged, "f.txt").unwrap();
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let key = |key| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::ALT,
        };
        frame(&ctx, &mut view, vec![], Modifiers::NONE);
        assert_eq!(view.scroll_y, 0.0);
        frame(
            &ctx,
            &mut view,
            vec![key(egui::Key::ArrowDown)],
            Modifiers::ALT,
        );
        frame(&ctx, &mut view, vec![], Modifiers::NONE);
        assert!(
            view.scroll_y > 0.0,
            "Alt+Down did not move to the second hunk"
        );
        frame(
            &ctx,
            &mut view,
            vec![key(egui::Key::ArrowUp)],
            Modifiers::ALT,
        );
        frame(&ctx, &mut view, vec![], Modifiers::NONE);
        assert_eq!(view.scroll_y, 0.0);
    }
}
