use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{
    self, Color32, CursorIcon, FontId, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2,
};
use gix::ObjectId;
use kelp_core::diff::{self, Body, FileDiff};
use kelp_core::ops::Op;
use kelp_core::review::Review;
use kelp_ui::diff::{DiffAction, DiffBody, DiffInput, DiffMode, DiffTexts, HunkButton};

use crate::avatars::AvatarStore;
use crate::blame_view::{self, BlameView};
use crate::dialogs::Dialog;
use crate::preview_view::Preview;
use crate::{theme, widgets};

thread_local! {
    static ASKED_BEFORE_DISCARD: Cell<bool> = const { Cell::new(false) };
}

const STAGE_HUNK_W: f32 = 118.0;
const DISCARD_W: f32 = 76.0;
const STAGE_BUTTON: usize = 0;
const HEADER_H: f32 = 48.0;
const TITLE_STATS_W: f32 = 150.0;
const TITLE_TOO_NARROW: f32 = 120.0;
const TITLE_ROOMY: f32 = 320.0;
const STACKED_PANE_H: f32 = 480.0;

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

pub use kelp_ui::diff::DiffLayout as Layout;

pub struct DiffView {
    pub source: DiffSource,
    pub diff: FileDiff,
    old_path: Option<String>,
    body: DiffBody,
    mode: Mode,
    pending: Option<Op>,
    header_squeeze: u8,
    lfs_hint: bool,
    embedded: bool,
    stacked: bool,
    ask: Option<Dialog>,
    preview: Option<Preview>,
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
        let lfs_hint = lfs_hint_due(&diff);
        Ok(Self {
            source,
            old_path: old_path.map(str::to_string),
            body: DiffBody::new(&diff),
            diff,
            mode,
            pending: None,
            header_squeeze: 0,
            embedded: false,
            stacked: false,
            lfs_hint,
            ask: None,
            preview,
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
        self.body = kept.body;
        self.body.reload(&self.diff);
        self.header_squeeze = kept.header_squeeze;
        self.lfs_hint = kept.lfs_hint;
        self.embedded = kept.embedded;
        self.stacked = kept.stacked;
        self.blame = kept.blame;
        if self.is_working()
            && let Some(blame) = &mut self.blame
        {
            blame.restart();
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
        if self.stacked {
            return false;
        }
        match self.mode {
            Mode::Diff => self.body.has_changes_to_jump_to(DiffMode::Diff),
            Mode::File => self.body.has_changes_to_jump_to(DiffMode::File),
            Mode::Preview | Mode::Blame => false,
        }
    }

    fn hunk_buttons(&self) -> Vec<HunkButton> {
        let Some(label) = self.hunk_action() else {
            return Vec::new();
        };
        let mut buttons = vec![HunkButton {
            label: label.to_string(),
            hint: None,
            danger: false,
            width: STAGE_HUNK_W,
        }];
        if self.can_discard() {
            buttons.push(HunkButton {
                label: "Discard".to_string(),
                hint: Some("Undo this hunk in the file. Shift-click skips the question.".into()),
                danger: true,
                width: DISCARD_W,
            });
        }
        buttons
    }

    fn line_select_hint(&self) -> Option<&'static str> {
        self.lines_selectable().then(|| {
            if self.is_staged() {
                "Select to unstage (Shift-click for a range)"
            } else {
                "Select to stage (Shift-click for a range)"
            }
        })
    }

    fn apply(&mut self, actions: Vec<DiffAction>, review: &mut Review) -> bool {
        let mut changed = false;
        for action in actions {
            match action {
                DiffAction::AddThread { anchor, comment } => {
                    review.add(self.path(), self.commit(), anchor, comment);
                    changed = true;
                }
                DiffAction::Reply { thread, comment } => {
                    review.reply(thread, comment);
                    changed = true;
                }
                DiffAction::SetResolved { thread, resolved } => {
                    review.set_resolved(thread, resolved);
                    changed = true;
                }
                DiffAction::DeleteThread(thread) => {
                    review.delete(thread);
                    changed = true;
                }
                DiffAction::HunkButton {
                    hunk,
                    button: STAGE_BUTTON,
                    ..
                } => {
                    if let Some(patch) = diff::hunk_patch(&self.diff, hunk) {
                        let reverse = self.source == DiffSource::Staged;
                        self.pending = Some(Op::ApplyToIndex { patch, reverse });
                    }
                }
                DiffAction::HunkButton { hunk, shift, .. } => self.discard_hunk(hunk, shift),
            }
        }
        changed
    }

    fn stage_selected_lines(&mut self) {
        let staged = self.source == DiffSource::Staged;
        if let Some(patch) = diff::lines_patch(&self.diff, self.body.selection(), staged) {
            self.pending = Some(Op::ApplyToIndex {
                patch,
                reverse: staged,
            });
        }
        self.body.clear_selection();
    }

    fn can_discard(&self) -> bool {
        self.source == DiffSource::Unstaged && self.lines_selectable()
    }

    fn discard_selected_lines(&mut self, skip_confirm: bool) {
        let count = self.body.selection().len();
        if let Some(patch) = diff::discard_lines_patch(&self.diff, self.body.selection()) {
            let what = format!("{count} line{}", if count == 1 { "" } else { "s" });
            self.discard(patch, &what, skip_confirm);
        }
        self.body.clear_selection();
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

    pub fn layout(&self) -> Layout {
        self.body.layout()
    }

    pub fn jump_to_change(&mut self, next: bool) {
        self.body.jump_to_change(next);
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
        self.body.set_layout(layout);
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
        self.stacked = false;
        self.show(ui, review, author, avatars)
    }

    pub fn stacked_ui(
        &mut self,
        ui: &mut Ui,
        review: &mut Review,
        author: &str,
        avatars: &mut AvatarStore,
    ) -> Event {
        self.stacked = true;
        self.show(ui, review, author, avatars)
    }

    fn show(
        &mut self,
        ui: &mut Ui,
        review: &mut Review,
        author: &str,
        avatars: &mut AvatarStore,
    ) -> Event {
        if !self.stacked {
            self.body.jump_with_keys(ui);
        }
        let mut event = self.header(ui, review);
        self.lfs_strip(ui);
        if self.mode == Mode::Preview
            && let Some(preview) = &mut self.preview
        {
            if self.stacked {
                ui.allocate_ui(vec2(ui.available_width(), STACKED_PANE_H), |ui| {
                    preview.ui(ui)
                });
            } else {
                preview.ui(ui);
            }
            return event;
        }
        if self.mode == Mode::Blame
            && let Some((dir, rev)) = &self.blame_from
        {
            let path = self.diff.path.clone();
            let blame = self
                .blame
                .get_or_insert_with(|| BlameView::new(ui.ctx(), dir.clone(), *rev, &path));
            let stacked = self.stacked;
            let mut reveal = None;
            let mut show_blame = |ui: &mut Ui| {
                if let blame_view::Event::Reveal(id) = blame.ui(ui, avatars) {
                    reveal = Some(id);
                }
            };
            if stacked {
                ui.allocate_ui(vec2(ui.available_width(), STACKED_PANE_H), show_blame);
            } else {
                show_blame(ui);
            }
            if let Some(id) = reveal {
                event = Event::Reveal(id);
            }
            return event;
        }
        let hunk_buttons = self.hunk_buttons();
        let line_select_hint = self.line_select_hint();
        let actions = self.body.ui(
            ui,
            DiffInput {
                diff: &self.diff,
                threads: &review.threads,
                author,
                mode: match self.mode {
                    Mode::File => DiffMode::File,
                    _ => DiffMode::Diff,
                },
                stacked: self.stacked,
                hunk_buttons: &hunk_buttons,
                line_select_hint,
                texts: &DiffTexts::default(),
                style: crate::graph_glue::diff_style(),
            },
        );
        if matches!(self.diff.body, Body::Text(_)) {
            if self.apply(actions, review) {
                event = Event::Changed;
            }
            if let Some(op) = self.pending.take() {
                event = Event::Run(op);
            }
            if let Some(dialog) = self.ask.take() {
                event = Event::Ask(dialog);
            }
        }
        event
    }

    fn header(&mut self, ui: &mut Ui, review: &Review) -> Event {
        let mut event = Event::None;
        egui::Frame::new()
            .fill(theme::panel())
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
                            if self.mode == Mode::Diff
                                && self.header_squeeze < 2
                                && matches!(self.diff.body, Body::Text(_))
                            {
                                let mut layout = self.body.layout();
                                widgets::segmented(
                                    ui,
                                    &mut layout,
                                    &[(Layout::Split, "Split"), (Layout::Unified, "Unified")],
                                );
                                self.body.set_layout(layout);
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
                                    self.body.jump_to_change(true);
                                }
                                if arrow_button(ui, true, "Previous change (⌥↑)") {
                                    self.body.jump_to_change(false);
                                }
                            }
                            if self.lines_selectable() && !self.body.selection().is_empty() {
                                ui.add_space(4.0);
                                let count = self.body.selection().len();
                                let verb = if self.is_staged() { "Unstage" } else { "Stage" };
                                let label = format!(
                                    "{verb} {count} line{}",
                                    if count == 1 { "" } else { "s" }
                                );
                                let button = egui::Button::new(
                                    RichText::new(label)
                                        .size(12.0)
                                        .family(theme::semibold())
                                        .color(theme::on_accent()),
                                )
                                .fill(theme::accent())
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
                                        .color(theme::deleted()),
                                    )
                                    .fill(theme::with_alpha(theme::deleted(), 0x1c))
                                    .stroke(Stroke::new(
                                        1.0,
                                        theme::with_alpha(theme::deleted(), 0x66),
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
                                let clear =
                                    RichText::new("Clear").size(12.0).color(theme::text_muted());
                                if crate::widgets::text_button(ui, clear).clicked() {
                                    self.body.clear_selection();
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
                                    .color(theme::text_faint()),
                            );
                            ui.add_space(-12.0);
                        }
                        ui.add(
                            egui::Label::new(
                                RichText::new(name)
                                    .monospace()
                                    .family(theme::semibold())
                                    .color(theme::text_strong()),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&path);
                        if self.mode == Mode::Diff {
                            ui.label(
                                RichText::new(format!("+{}", self.diff.added))
                                    .size(12.0)
                                    .color(theme::added()),
                            );
                            ui.label(
                                RichText::new(format!("−{}", self.diff.removed))
                                    .size(12.0)
                                    .color(theme::deleted()),
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
                        ui.label(RichText::new(origin).size(12.0).color(theme::text_faint()));
                        let comments = review.count_for(self.path());
                        if comments > 0 {
                            ui.label(
                                RichText::new(format!(
                                    "{comments} comment{}",
                                    if comments == 1 { "" } else { "s" }
                                ))
                                .size(12.0)
                                .color(theme::accent()),
                            );
                        }
                    },
                );
            });
        let rect = ui.min_rect();
        ui.painter().hline(
            rect.x_range(),
            rect.bottom(),
            Stroke::new(1.0, theme::border()),
        );
        event
    }
}

fn arrow_button(ui: &mut Ui, up: bool, hint: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    let response = response
        .on_hover_text(hint)
        .on_hover_cursor(CursorIcon::PointingHand);
    let hovered = response.hovered();
    if hovered {
        ui.painter().rect_filled(rect, 5.0, theme::control_hover());
    }
    let color = if hovered {
        theme::text_strong()
    } else {
        theme::text_muted()
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

fn next_squeeze(level: u8, title_w: f32) -> u8 {
    if title_w < TITLE_TOO_NARROW {
        (level + 1).min(2)
    } else if title_w > TITLE_ROOMY {
        level.saturating_sub(1)
    } else {
        level
    }
}

static LFS_HINT_SHOWN: AtomicBool = AtomicBool::new(false);

fn lfs_hint_due(diff: &FileDiff) -> bool {
    !diff.lfs.is_empty()
        && !kelp_core::lfs::installed()
        && !LFS_HINT_SHOWN.swap(true, Ordering::Relaxed)
}

impl DiffView {
    fn lfs_strip(&self, ui: &mut Ui) {
        let Some(pointer) = self.diff.lfs.shown() else {
            return;
        };
        let mut text = format!(
            "LFS file · {} · oid {}",
            human_bytes(pointer.size),
            &pointer.oid[..12]
        );
        if self.lfs_hint {
            text.push_str(" · Git LFS isn't installed, so only the pointer can be shown");
        }
        egui::Frame::new()
            .fill(theme::with_alpha(theme::accent(), 0x14))
            .inner_margin(Margin::symmetric(16, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(text).size(12.0).color(theme::text_muted()));
            });
    }
}

fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    match bytes as f64 {
        b if b < KB => format!("{bytes} B"),
        b if b < KB * KB => format!("{:.1} KB", b / KB),
        b if b < KB * KB * KB => format!("{:.1} MB", b / (KB * KB)),
        b => format!("{:.1} GB", b / (KB * KB * KB)),
    }
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

    use kelp_ui::diff::{HUNK_H, LINE_H};

    use super::{DiffSource, DiffView, HEADER_H};

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

        view.body.toggle_line(2, false);
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
        assert_eq!(
            view.body.selection().iter().copied().collect::<Vec<_>>(),
            [2]
        );
        click(&ctx, &mut view, line_center(3), Modifiers::SHIFT);
        assert_eq!(
            view.body.selection().iter().copied().collect::<Vec<_>>(),
            [2, 3]
        );
        click(&ctx, &mut view, line_center(0), Modifiers::NONE);
        assert_eq!(
            view.body.selection().len(),
            2,
            "context lines are not selectable"
        );

        view.stage_selected_lines();
        let Some(op @ Op::ApplyToIndex { reverse: false, .. }) = view.pending.take() else {
            panic!("expected a forward index patch")
        };
        op.run(&repo.0).unwrap();
        assert_eq!(run(&repo.0, &["show", ":f.txt"]).unwrap(), "a\ny\nz\nb\n");
        assert_eq!(view.body.selection().len(), 0);
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
        assert_eq!(view.body.scroll_y(), 0.0);
        frame(
            &ctx,
            &mut view,
            vec![key(egui::Key::ArrowDown)],
            Modifiers::ALT,
        );
        frame(&ctx, &mut view, vec![], Modifiers::NONE);
        assert!(
            view.body.scroll_y() > 0.0,
            "Alt+Down did not move to the second hunk"
        );
        frame(
            &ctx,
            &mut view,
            vec![key(egui::Key::ArrowUp)],
            Modifiers::ALT,
        );
        frame(&ctx, &mut view, vec![], Modifiers::NONE);
        assert_eq!(view.body.scroll_y(), 0.0);
    }
}
