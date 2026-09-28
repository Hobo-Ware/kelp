use std::collections::HashMap;
use std::f32::consts::PI;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui,
    pos2,
    text::{LayoutJob, TextFormat, TextWrapping},
    vec2,
};
use kelp_core::avatar;
use kelp_core::commit::{self, ChangeKind, FileChange, Summary};
use kelp_core::graph::EdgeKind;
use kelp_core::history::History;
use kelp_core::refs::RefLabel;
use kelp_core::workspace::Worktree;

use crate::avatars::AvatarStore;
use crate::columns::GraphColumns;
use crate::commands::Command;
use crate::graph_hover::{HoverPath, VisiblePath};
use crate::graph_rows::{Row, RowMap};
use crate::ref_labels::{self, DropPlan, DropTarget, LabelEvent, MenuFor};
use crate::repo_view::Selection;
use crate::theme;

pub const ROW_H: f32 = 30.0;
const LANE_W: f32 = 24.0;
const ARC_R: f32 = ROW_H / 2.0;
const AVATAR_R: f32 = 10.0;
const LINE_W: f32 = 2.25;
const LABELS_W: f32 = 170.0;
const LABELS_END: f32 = 158.0;
const GRAPH_PAD: f32 = 16.0;
const MIN_GRAPH_W: f32 = 80.0;
const DEFAULT_MAX_LANES: f32 = 14.0;
const HEADER_H: f32 = 28.0;
const WIP_GREY: Color32 = Color32::from_rgb(0x3a, 0x41, 0x50);
const HOVER_LINE_W: f32 = 3.5;
const OFF_PATH_OPACITY: f32 = 0.45;
const OPEN_BUTTON_W: f32 = 56.0;

pub struct GraphView {
    summaries: HashMap<usize, Summary>,
    pub scroll_to: Option<Selection>,
    graph_w: Option<f32>,
    lane_offset: f32,
    context: Option<Row>,
    hover: Option<(usize, HoverPath)>,
    drag: Option<ref_labels::Drag>,
    drop: Option<DropPlan>,
    pub columns_changed: Option<GraphColumns>,
}

pub struct GraphInput<'a> {
    pub repo: &'a gix::Repository,
    pub history: &'a History,
    pub selected: Option<Selection>,
    pub head_row: Option<usize>,
    pub wip: Option<Wip<'a>>,
    pub lit: Option<&'a [bool]>,
    pub descriptions: bool,
    pub other_wips: &'a [OtherWip<'a>],
    pub columns: GraphColumns,
}

#[derive(Clone, Copy)]
struct RowStyle {
    selected: bool,
    dashed_top: bool,
    faded: bool,
    descriptions: bool,
}

pub struct Wip<'a> {
    pub head_row: usize,
    pub changes: &'a [FileChange],
}

pub struct OtherWip<'a> {
    pub head_row: usize,
    pub tree: &'a Worktree,
    pub changes: usize,
}

pub enum Action {
    Select(Selection),
    Command(Command),
}

#[derive(Clone, Copy, PartialEq)]
enum OnPath {
    NoHover,
    Yes,
    No,
}

impl GraphView {
    pub fn new() -> Self {
        Self {
            summaries: HashMap::new(),
            scroll_to: None,
            graph_w: None,
            lane_offset: 0.0,
            context: None,
            drag: None,
            drop: None,
            hover: None,
            columns_changed: None,
        }
    }

    pub fn clear_cache(&mut self) {
        self.summaries.clear();
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        input: GraphInput<'_>,
        avatars: &mut AvatarStore,
        mut menu: impl FnMut(&mut Ui, MenuFor<'_>),
    ) -> Option<Action> {
        let GraphInput {
            repo,
            history,
            selected,
            head_row,
            wip,
            lit,
            descriptions,
            other_wips,
            mut columns,
        } = input;
        let content_w = history.layout.lane_count() as f32 * LANE_W + GRAPH_PAD * 2.0;
        let auto_w = content_w.clamp(120.0, DEFAULT_MAX_LANES * LANE_W + GRAPH_PAD * 2.0);
        let max_w = (ui.available_width() * 0.6).max(MIN_GRAPH_W);
        let graph_w = self.graph_w.unwrap_or(auto_w).clamp(MIN_GRAPH_W, max_w);
        self.lane_offset = self.lane_offset.clamp(0.0, (content_w - graph_w).max(0.0));
        let msg_x = LABELS_W + graph_w;
        if self.header(ui, msg_x, auto_w, content_w > graph_w, &mut columns) {
            self.columns_changed = Some(columns);
        }

        let other_heads: Vec<usize> = other_wips.iter().map(|w| w.head_row).collect();
        let map = RowMap::new(
            history.len(),
            wip.as_ref().map(|w| w.head_row),
            &other_heads,
        );
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let mut action = None;
        let mut scroll = egui::ScrollArea::vertical().auto_shrink(false);
        if let Some(selection) = self.scroll_to.take() {
            let visible = ui.available_height();
            let target = (map.display_of(selection) as f32 * ROW_H - visible / 3.0).max(0.0);
            scroll = scroll.vertical_scroll_offset(target);
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        let lane_offset = &mut self.lane_offset;
        let summaries = &mut self.summaries;
        let context = &mut self.context;
        let drag = &mut self.drag;
        let drop = &mut self.drop;
        let open_refs = std::env::var("KELP_OPEN_REFS").ok();
        let open_drop = std::env::var("KELP_OPEN_DROP").ok();
        let hover = &mut self.hover;
        scroll.show_rows(ui, ROW_H, map.total(), |ui, rows| {
            for display in rows.clone() {
                if let Row::Commit(row) = map.resolve(display) {
                    summaries.entry(row).or_insert_with(|| {
                        let summary = load_summary(repo, history, row);
                        crate::fonts::ensure_fallback(ui.ctx(), &summary.title);
                        crate::fonts::ensure_fallback(ui.ctx(), &summary.author);
                        summary
                    });
                }
            }
            let size = vec2(ui.available_width(), ROW_H * rows.len() as f32);
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let graph_area =
                Rect::from_x_y_ranges(rect.left() + LABELS_W..=rect.left() + msg_x, rect.y_range());
            if ui.rect_contains_pointer(graph_area) {
                let dx = ui.input(|i| i.smooth_scroll_delta.x);
                if dx != 0.0 {
                    *lane_offset = (*lane_offset - dx).clamp(0.0, (content_w - graph_w).max(0.0));
                }
            }
            let row_at = |y: f32| {
                let display = rows.start + ((y - rect.top()) / ROW_H) as usize;
                map.resolve(display.min(map.total().saturating_sub(1)))
            };
            let hovered_row = forced_hover_row().or_else(|| {
                response
                    .hover_pos()
                    .filter(|_| !response.context_menu_opened())
                    .and_then(|pos| match row_at(pos.y) {
                        Row::Commit(row) => Some(row),
                        _ => None,
                    })
            });
            match hovered_row {
                Some(row) if hover.as_ref().is_none_or(|(r, _)| *r != row) => {
                    *hover = Some((row, trace_hover(history, row)));
                }
                None => *hover = None,
                _ => {}
            }
            let first_commit = (rows.start..rows.end)
                .find_map(|d| match map.resolve(d) {
                    Row::Commit(row) => Some(row),
                    _ => None,
                })
                .unwrap_or(0);
            let visible_path = hover
                .as_ref()
                .map(|(_, path)| path.visible(first_commit..first_commit + rows.len() + 1));
            let painter = ui.painter_at(rect);
            let mut label_events = Vec::new();
            let mut label_hits: Vec<(Rect, usize, RefLabel)> = Vec::new();
            let mut open_buttons = Vec::new();
            let mut halo = None;
            for (i, display) in rows.clone().enumerate() {
                let geo = RowGeo {
                    left: rect.left(),
                    right: rect.right(),
                    top: rect.top() + i as f32 * ROW_H,
                    msg_x,
                    lane_offset: *lane_offset,
                    columns,
                };
                let row_kind = map.resolve(display);
                let is_selected =
                    row_kind.selection().is_some() && row_kind.selection() == selected;
                if let Row::Commit(row) = row_kind
                    && hover.as_ref().is_some_and(|(h, _)| *h == row)
                    && !is_selected
                {
                    let band =
                        Rect::from_x_y_ranges(geo.msg_left()..=geo.right, geo.top..=geo.bottom());
                    painter.rect_filled(band, 0.0, theme::with_alpha(Color32::WHITE, 0x06));
                }
                let on_path = |row: usize| match &visible_path {
                    None => OnPath::NoHover,
                    Some(path) if path.contains(row) => OnPath::Yes,
                    Some(_) => OnPath::No,
                };
                match (row_kind, &wip) {
                    (Row::CurrentWip, Some(wip)) => {
                        let label = WipLabel {
                            owner: "this worktree".into(),
                            counts: counts_label(wip.changes),
                            has_button: false,
                        };
                        paint_wip_row(&painter, &geo, history, wip.head_row, &label, is_selected);
                    }
                    (Row::OtherWip(i), _) => {
                        let other = &other_wips[i];
                        let label = WipLabel {
                            owner: other.tree.name(),
                            counts: format!("{} changed", other.changes),
                            has_button: true,
                        };
                        paint_wip_row(&painter, &geo, history, other.head_row, &label, false);
                        let button = open_button_rect(&geo);
                        let hot = response.hover_pos().is_some_and(|p| button.contains(p));
                        paint_open_button(&painter, button, hot);
                        open_buttons.push((button, other.tree.path.clone()));
                    }
                    (Row::Commit(row), _) => {
                        let avatar = avatars.texture(&summaries[&row].email, history.id(row));
                        let style = RowStyle {
                            selected: is_selected,
                            dashed_top: map.has_wip_above(row),
                            faded: lit.is_some_and(|l| !l.get(row).copied().unwrap_or(true)),
                            descriptions,
                        };
                        let placed = paint_row(
                            &painter,
                            &geo,
                            history,
                            RowPaint {
                                row,
                                summary: &summaries[&row],
                                avatar,
                                style,
                                path: visible_path.as_ref(),
                                on_path: on_path(row),
                            },
                            now,
                        );
                        let labels: Vec<&RefLabel> = history.refs.at_row(row).collect();
                        let force_open = open_refs
                            .as_deref()
                            .is_some_and(|rev| history.id(row).to_string().starts_with(rev));
                        for spot in &placed {
                            if let ref_labels::Slot::Label(i) = spot.slot {
                                label_hits.push((spot.rect, row, labels[i].clone()));
                            }
                        }
                        let lane = theme::lane(history.layout.node_color(row));
                        if is_selected {
                            let center = pos2(geo.lane_x(history.layout.node_lane(row)), geo.mid());
                            let graph_x = geo.graph_left()..=geo.msg_left();
                            halo = Some((center, lane, graph_x));
                        }
                        let events = ref_labels::interact(
                            ui, row, &labels, &placed, lane, force_open, &mut menu,
                        );
                        label_events.extend(events.into_iter().map(|e| (row, e)));
                    }
                    (Row::CurrentWip, None) => {}
                }
            }
            if let Some((center, lane, graph_x)) = halo {
                let clip = Rect::from_x_y_ranges(graph_x, rect.y_range());
                selection_halo(&painter.with_clip_rect(clip), center, AVATAR_R, lane);
            }
            let commit_at = |pos: Pos2| -> Option<usize> {
                if !rect.contains(pos) {
                    return None;
                }
                let display = rows.start + ((pos.y - rect.top()) / ROW_H) as usize;
                match map.resolve(display.min(map.total().saturating_sub(1))) {
                    Row::Commit(row) => Some(row),
                    _ => None,
                }
            };
            let target_at = |pos: Pos2| -> Option<DropTarget> {
                let row = commit_at(pos)?;
                let label = label_hits
                    .iter()
                    .find(|(r, hit_row, _)| *hit_row == row && r.contains(pos))
                    .map(|(_, _, label)| label.clone());
                Some(DropTarget {
                    row,
                    commit: history.id(row).to_string(),
                    label,
                    is_head: head_row == Some(row),
                })
            };
            for (row, event) in label_events {
                match event {
                    LabelEvent::Select => action = Some(Action::Select(Selection::Commit(row))),
                    LabelEvent::Command(command) => action = Some(Action::Command(command)),
                    LabelEvent::DragStart(label) => {
                        *drag = Some(ref_labels::Drag {
                            label,
                            from_row: row,
                        })
                    }
                    LabelEvent::DragStop => {
                        let pos = ui.ctx().pointer_latest_pos();
                        if let (Some(dragged), Some(pos)) = (drag.take(), pos)
                            && let Some(target) = target_at(pos)
                            && (target.row != dragged.from_row || target.label.is_some())
                        {
                            *drop = Some(DropPlan {
                                source: dragged.label,
                                target,
                                pos,
                            });
                            egui::Popup::open_id(ui.ctx(), drop_menu_id());
                        }
                    }
                }
            }
            if let Some(dragged) = drag.as_ref() {
                if let Some(pos) = ui.ctx().pointer_latest_pos() {
                    if let Some(target) = commit_at(pos) {
                        let display = map.display(Row::Commit(target));
                        if rows.contains(&display) {
                            let top = rect.top() + (display - rows.start) as f32 * ROW_H;
                            let row_rect = Rect::from_x_y_ranges(
                                rect.left() + 2.0..=rect.right() - 2.0,
                                top + 1.0..=top + ROW_H - 1.0,
                            );
                            painter.rect_stroke(
                                row_rect,
                                CornerRadius::same(5),
                                Stroke::new(1.5, theme::with_alpha(theme::ACCENT, 0xb0)),
                                egui::StrokeKind::Inside,
                            );
                        }
                    }
                    let lane = theme::lane(history.layout.node_color(dragged.from_row));
                    ref_labels::paint_ghost(ui.ctx(), &dragged.label, pos, lane);
                    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                }
                if !ui.input(|i| i.pointer.primary_down()) {
                    *drag = None;
                }
            }
            if let Some(spec) = open_drop.as_deref()
                && drop.is_none()
                && let Some((source, rev)) = spec.split_once('@')
                && let Some((_, _, source_label)) =
                    label_hits.iter().find(|(_, _, l)| l.name == source)
                && let Some(row) = rows.clone().find_map(|display| match map.resolve(display) {
                    Row::Commit(row) if history.id(row).to_string().starts_with(rev) => Some(row),
                    _ => None,
                })
            {
                let top = rect.top() + (map.display(Row::Commit(row)) - rows.start) as f32 * ROW_H;
                let label = label_hits
                    .iter()
                    .find(|(_, hit_row, l)| *hit_row == row && l.name != source)
                    .map(|(_, _, l)| l.clone());
                *drop = Some(DropPlan {
                    source: source_label.clone(),
                    target: DropTarget {
                        row,
                        commit: history.id(row).to_string(),
                        label,
                        is_head: head_row == Some(row),
                    },
                    pos: pos2(rect.left() + msg_x + 60.0, top + ROW_H),
                });
                egui::Popup::open_id(ui.ctx(), drop_menu_id());
            }
            let clicked_at = response
                .interact_pointer_pos()
                .filter(|_| response.clicked() || response.double_clicked());
            if let Some((_, path)) =
                clicked_at.and_then(|pos| open_buttons.iter().find(|(r, _)| r.contains(pos)))
            {
                action = Some(Action::Command(Command::OpenRepo(path.clone())));
            } else if (response.clicked() || response.secondary_clicked())
                && let Some(pos) = response.interact_pointer_pos()
            {
                let row = row_at(pos.y);
                if let Some(selection) = row.selection() {
                    action = Some(Action::Select(selection));
                }
                if response.secondary_clicked() {
                    *context = Some(row);
                }
            } else if response.double_clicked()
                && let Some(pos) = response.interact_pointer_pos()
                && let Row::OtherWip(i) = row_at(pos.y)
            {
                action = Some(Action::Command(Command::OpenRepo(
                    other_wips[i].tree.path.clone(),
                )));
            }
            if let Some(row) = *context {
                response.context_menu(|ui| match row {
                    Row::Commit(commit) => {
                        let title = summaries
                            .get(&commit)
                            .map(|s| s.title.clone())
                            .unwrap_or_default();
                        menu(ui, MenuFor::Commit(Selection::Commit(commit), &title));
                    }
                    Row::CurrentWip => menu(ui, MenuFor::Commit(Selection::Wip, "")),
                    Row::OtherWip(i) => {
                        if let Some(other) = other_wips.get(i) {
                            menu(ui, MenuFor::Worktree(other.tree));
                        }
                    }
                });
            }
            if let Some((row, _)) = hover.as_ref()
                && let Some(summary) = summaries.get(row)
                && !response.context_menu_opened()
            {
                let id = history.id(*row);
                response
                    .clone()
                    .on_hover_ui_at_pointer(|ui| commit_tooltip(ui, summary, id, now));
            }
            if std::env::var("KELP_OPEN_MENU").as_deref() == Ok("commit")
                && let Some(selection) = selected
            {
                let top = rect.top() + (map.display_of(selection) - rows.start) as f32 * ROW_H;
                let title = match selection {
                    Selection::Commit(row) => summaries[&row].title.clone(),
                    Selection::Wip => String::new(),
                };
                egui::Popup::context_menu(&response)
                    .open(true)
                    .at_position(egui::pos2(rect.left() + msg_x + 120.0, top + ROW_H))
                    .show(|ui| menu(ui, MenuFor::Commit(selection, &title)));
            }
        });
        if let Some(plan) = self.drop.as_ref() {
            let id = drop_menu_id();
            egui::Popup::new(
                id,
                ui.ctx().clone(),
                plan.pos,
                egui::LayerId::new(egui::Order::Foreground, id),
            )
            .kind(egui::PopupKind::Menu)
            .open_memory(None)
            .show(|ui| menu(ui, MenuFor::Drop(plan)));
            if !egui::Popup::is_id_open(ui.ctx(), id) {
                self.drop = None;
            }
        }
        action
    }

    fn header(
        &mut self,
        ui: &mut Ui,
        msg_x: f32,
        auto_w: f32,
        scrollable: bool,
        columns: &mut GraphColumns,
    ) -> bool {
        let (rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), HEADER_H), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::HEADER);
        painter.hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0, theme::BORDER),
        );
        let font = FontId::monospace(10.0);
        let y = rect.center().y;
        let graph_label = if scrollable { "GRAPH ⇄" } else { "GRAPH" };
        for (x, label) in [
            (12.0, "BRANCH / TAG"),
            (LABELS_W + 8.0, graph_label),
            (msg_x + 14.0, "COMMIT MESSAGE"),
        ] {
            painter.text(
                pos2(rect.left() + x, y),
                Align2::LEFT_CENTER,
                label,
                font.clone(),
                theme::TEXT_FAINT,
            );
        }

        let handle = Rect::from_center_size(pos2(rect.left() + msg_x, y), vec2(9.0, HEADER_H));
        let response = ui.interact(
            handle,
            ui.id().with("graph-resize"),
            Sense::click_and_drag(),
        );
        let hot = response.hovered() || response.dragged();
        if hot {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
        painter.vline(
            rect.left() + msg_x,
            rect.top() + 6.0..=rect.bottom() - 6.0,
            Stroke::new(
                if hot { 2.0 } else { 1.0 },
                if hot { theme::ACCENT } else { theme::BORDER },
            ),
        );
        if response.dragged() {
            let current = self.graph_w.unwrap_or(auto_w);
            self.graph_w = Some((current + response.drag_delta().x).max(MIN_GRAPH_W));
        }
        if response.double_clicked() {
            self.graph_w = None;
        }
        response.on_hover_text(
            "Drag to resize the graph. Double-click to reset. Scroll sideways to see more lanes.",
        );
        crate::columns::header(ui, rect, columns)
    }
}

fn load_summary(repo: &gix::Repository, history: &History, row: usize) -> Summary {
    commit::summary(repo, history.id(row)).unwrap_or_else(|_| Summary {
        title: history.id(row).to_hex_with_len(7).to_string(),
        body_preview: String::new(),
        author: String::new(),
        email: String::new(),
        time: 0,
    })
}

struct RowGeo {
    left: f32,
    right: f32,
    top: f32,
    msg_x: f32,
    lane_offset: f32,
    columns: GraphColumns,
}

impl RowGeo {
    fn lane_x(&self, lane: u16) -> f32 {
        self.left + LABELS_W + GRAPH_PAD + lane as f32 * LANE_W - self.lane_offset
    }
    fn mid(&self) -> f32 {
        self.top + ROW_H / 2.0
    }
    fn bottom(&self) -> f32 {
        self.top + ROW_H
    }
    fn graph_left(&self) -> f32 {
        self.left + LABELS_W
    }
    fn msg_left(&self) -> f32 {
        self.left + self.msg_x
    }
    fn message_right(&self) -> f32 {
        crate::columns::message_right(self.right, &self.columns)
    }
    fn graph_clip(&self, painter: &egui::Painter) -> egui::Painter {
        let clip = Rect::from_x_y_ranges(
            self.graph_left()..=self.msg_left(),
            self.top..=self.bottom(),
        );
        painter.with_clip_rect(clip.intersect(painter.clip_rect()))
    }
    fn lane_visible(&self, lane: u16) -> bool {
        let x = self.lane_x(lane);
        x > self.graph_left() - LANE_W && x < self.msg_left() + LANE_W
    }
}

struct RowPaint<'a> {
    row: usize,
    summary: &'a Summary,
    avatar: Option<egui::TextureId>,
    style: RowStyle,
    path: Option<&'a VisiblePath<'a>>,
    on_path: OnPath,
}

fn paint_row(
    painter: &egui::Painter,
    geo: &RowGeo,
    history: &History,
    paint: RowPaint<'_>,
    now: i64,
) -> Vec<ref_labels::Placed> {
    let RowPaint {
        row,
        summary,
        avatar,
        style,
        path,
        on_path,
    } = paint;
    let RowStyle {
        selected,
        dashed_top,
        faded,
        descriptions,
    } = style;
    let mut soft = painter.clone();
    if faded {
        soft.multiply_opacity(0.35);
    }
    let layout = &history.layout;
    let node_lane = layout.node_lane(row);
    let color = theme::lane(layout.node_color(row));
    let node = pos2(geo.lane_x(node_lane), geo.mid());
    let graph = geo.graph_clip(painter);
    let mut graph_soft = graph.clone();
    if faded {
        graph_soft.multiply_opacity(0.35);
    }
    if on_path == OnPath::No {
        graph_soft.multiply_opacity(OFF_PATH_OPACITY + 0.3);
    }

    let band_left = node.x.max(geo.graph_left());
    let band = Rect::from_x_y_ranges(
        band_left..=geo.msg_left(),
        geo.top + 4.0..=geo.bottom() - 4.0,
    );
    graph_soft.rect_filled(
        band,
        0.0,
        theme::with_alpha(color, if selected { 0x33 } else { 0x0f }),
    );
    let strip = Rect::from_x_y_ranges(
        geo.msg_left()..=geo.msg_left() + 3.0,
        geo.top..=geo.bottom(),
    );
    soft.rect_filled(strip, 0.0, theme::with_alpha(color, 0xb0));
    if selected {
        let bg = Rect::from_x_y_ranges(geo.msg_left() + 3.0..=geo.right, geo.top..=geo.bottom());
        painter.rect_filled(bg, 0.0, theme::SELECTED_ROW);
    }

    let edges = layout.edges(row);
    let has_top = edges.iter().any(|e| e.kind == EdgeKind::Top);
    if dashed_top && !has_top {
        dashed(&graph, pos2(node.x, geo.top), node, color);
    }
    for edge in edges {
        if edge.kind == EdgeKind::Pass && !geo.lane_visible(edge.lane) {
            continue;
        }
        let lane_color = theme::lane(edge.color);
        let stroke = match path.map(|p| p.carries(row, edge)) {
            None => Stroke::new(LINE_W, lane_color),
            Some(true) => Stroke::new(HOVER_LINE_W, lane_color),
            Some(false) => Stroke::new(LINE_W, lane_color.gamma_multiply(OFF_PATH_OPACITY)),
        };
        let x = geo.lane_x(edge.lane);
        match edge.kind {
            EdgeKind::Pass => {
                graph.line_segment([pos2(x, geo.top), pos2(x, geo.bottom())], stroke);
            }
            EdgeKind::Top => {
                graph.line_segment([pos2(x, geo.top), pos2(x, geo.mid())], stroke);
            }
            EdgeKind::Bottom => {
                graph.line_segment([pos2(x, geo.mid()), pos2(x, geo.bottom())], stroke);
            }
            EdgeKind::JoinIn => {
                let dir = (node.x - x).signum();
                let corner = pos2(x + dir * ARC_R, geo.top);
                let mut points =
                    quarter_arc(corner, pos2(x, geo.top), pos2(x + dir * ARC_R, geo.mid()));
                points.push(node);
                graph.add(Shape::line(points, stroke));
            }
            EdgeKind::MergeOut => {
                let dir = (x - node.x).signum();
                let corner = pos2(x - dir * ARC_R, geo.bottom());
                let mut points = vec![node];
                points.extend(quarter_arc(
                    corner,
                    pos2(x - dir * ARC_R, geo.mid()),
                    pos2(x, geo.bottom()),
                ));
                graph.add(Shape::line(points, stroke));
            }
        }
    }

    let mut placed = Vec::new();
    if node.x >= geo.graph_left() {
        let labels: Vec<&RefLabel> = history.refs.at_row(row).collect();
        placed = paint_labels(&soft, geo, &labels, node, color);
    }
    paint_avatar(&graph_soft, node, summary, avatar, color);
    paint_message(
        &soft,
        geo,
        history.id(row),
        summary,
        selected,
        descriptions,
        now,
    );
    placed
}

struct WipLabel {
    owner: String,
    counts: String,
    has_button: bool,
}

fn counts_label(changes: &[FileChange]) -> String {
    let count = |k: ChangeKind| changes.iter().filter(|c| c.kind == k).count();
    [
        (
            count(ChangeKind::Modified) + count(ChangeKind::Renamed),
            "modified",
        ),
        (count(ChangeKind::Added), "added"),
        (count(ChangeKind::Deleted), "deleted"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, label)| format!("{n} {label}"))
    .collect::<Vec<_>>()
    .join(" · ")
}

fn paint_wip_row(
    painter: &egui::Painter,
    geo: &RowGeo,
    history: &History,
    head_row: usize,
    label: &WipLabel,
    selected: bool,
) {
    let layout = &history.layout;
    let head_lane = layout.node_lane(head_row);
    let head_color = theme::lane(layout.node_color(head_row));
    let node = pos2(geo.lane_x(head_lane), geo.mid());
    let graph = geo.graph_clip(painter);

    let strip = Rect::from_x_y_ranges(
        geo.msg_left()..=geo.msg_left() + 3.0,
        geo.top..=geo.bottom(),
    );
    painter.rect_filled(strip, 0.0, WIP_GREY);
    if selected {
        let bg = Rect::from_x_y_ranges(geo.msg_left() + 3.0..=geo.right, geo.top..=geo.bottom());
        painter.rect_filled(bg, 0.0, theme::SELECTED_ROW);
    }
    for edge in layout.edges(head_row) {
        let from_above = matches!(edge.kind, EdgeKind::Pass | EdgeKind::Top | EdgeKind::JoinIn);
        if from_above && geo.lane_visible(edge.lane) {
            let x = geo.lane_x(edge.lane);
            graph.line_segment(
                [pos2(x, geo.top), pos2(x, geo.bottom())],
                Stroke::new(LINE_W, theme::lane(edge.color)),
            );
        }
    }
    dashed(&graph, node, pos2(node.x, geo.bottom()), head_color);

    let ring: Vec<Pos2> = (0..=48)
        .map(|i| {
            let a = i as f32 / 48.0 * 2.0 * PI;
            node + vec2(a.cos(), a.sin()) * (AVATAR_R + 1.0)
        })
        .collect();
    graph.circle_filled(node, AVATAR_R + 1.0, theme::BG);
    graph.extend(Shape::dashed_line(
        &ring,
        Stroke::new(2.0, head_color),
        3.0,
        2.5,
    ));
    paint_plus(&graph, node, 4.0, head_color);

    let mut job = LayoutJob::default();
    let italic = TextFormat {
        italics: true,
        ..TextFormat::simple(FontId::proportional(13.0), theme::TEXT_MUTED)
    };
    job.append("Uncommitted changes", 0.0, italic);
    job.append(
        &format!(" · {}", label.owner),
        0.0,
        TextFormat::simple(FontId::proportional(13.0), theme::TEXT_MUTED),
    );
    job.append(
        &label.counts,
        10.0,
        TextFormat::simple(FontId::proportional(13.0), theme::TEXT_FAINT),
    );
    let reserved = if label.has_button {
        OPEN_BUTTON_W + 28.0
    } else {
        14.0
    };
    job.wrap =
        TextWrapping::truncate_at_width((geo.right - reserved - geo.msg_left() - 15.0).max(0.0));
    let galley = painter.layout_job(job);
    painter.galley(
        pos2(geo.msg_left() + 15.0, geo.mid() - galley.size().y / 2.0),
        galley,
        theme::TEXT,
    );
}

fn paint_plus(painter: &egui::Painter, center: Pos2, half: f32, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    painter.line_segment([center - vec2(half, 0.0), center + vec2(half, 0.0)], stroke);
    painter.line_segment([center - vec2(0.0, half), center + vec2(0.0, half)], stroke);
}

fn open_button_rect(geo: &RowGeo) -> Rect {
    Rect::from_min_size(
        pos2(geo.right - OPEN_BUTTON_W - 14.0, geo.top + 5.0),
        vec2(OPEN_BUTTON_W, ROW_H - 10.0),
    )
}

fn paint_open_button(painter: &egui::Painter, rect: Rect, hot: bool) {
    let (fill, text) = if hot {
        (theme::CONTROL_HOVER, theme::TEXT_STRONG)
    } else {
        (theme::CONTROL, theme::TEXT)
    };
    painter.rect(
        rect,
        CornerRadius::same(5),
        fill,
        Stroke::new(1.0, theme::BORDER),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "Open",
        FontId::proportional(12.0),
        text,
    );
}

fn forced_hover_row() -> Option<usize> {
    std::env::var("KELP_HOVER_ROW").ok()?.parse().ok()
}

fn trace_hover(history: &History, row: usize) -> HoverPath {
    HoverPath::toward_tip(
        row,
        |r| history.parents(r).first().copied(),
        |r| history.refs.at_row(r).next().is_some(),
        |r| history.layout.node_lane(r),
    )
}

fn commit_tooltip(ui: &mut Ui, summary: &Summary, id: gix::ObjectId, now: i64) {
    ui.set_max_width(420.0);
    ui.spacing_mut().item_spacing.y = 4.0;
    ui.label(
        egui::RichText::new(&summary.title)
            .family(theme::semibold())
            .color(theme::TEXT_STRONG),
    );
    let body: Vec<&str> = summary
        .body_preview
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(3)
        .collect();
    if !body.is_empty() {
        ui.label(egui::RichText::new(body.join("\n")).color(theme::TEXT_MUTED));
    }
    ui.label(
        egui::RichText::new(format!("{} <{}>", summary.author, summary.email))
            .size(12.0)
            .color(theme::TEXT),
    );
    ui.label(
        egui::RichText::new(format!(
            "{} · {}",
            commit::calendar_time(summary.time),
            commit::relative_time(summary.time, now)
        ))
        .size(12.0)
        .color(theme::TEXT_FAINT),
    );
    ui.label(
        egui::RichText::new(id.to_string())
            .monospace()
            .size(11.0)
            .color(theme::TEXT_FAINT),
    );
}

fn dashed(painter: &egui::Painter, from: Pos2, to: Pos2, color: Color32) {
    painter.extend(Shape::dashed_line(
        &[from, to],
        Stroke::new(2.0, color),
        3.0,
        3.0,
    ));
}

fn quarter_arc(center: Pos2, from: Pos2, to: Pos2) -> Vec<Pos2> {
    let a0 = (from - center).angle();
    let mut delta = (to - center).angle() - a0;
    if delta > PI {
        delta -= 2.0 * PI;
    } else if delta < -PI {
        delta += 2.0 * PI;
    }
    const STEPS: usize = 10;
    (0..=STEPS)
        .map(|i| {
            let a = a0 + delta * i as f32 / STEPS as f32;
            center + vec2(a.cos(), a.sin()) * ARC_R
        })
        .collect()
}

pub fn paint_avatar(
    painter: &egui::Painter,
    node: Pos2,
    summary: &Summary,
    texture: Option<egui::TextureId>,
    lane: Color32,
) {
    draw_avatar(
        painter,
        node,
        AVATAR_R,
        &summary.author,
        &summary.email,
        texture,
        lane,
        false,
    );
}

pub fn selection_halo(painter: &egui::Painter, center: Pos2, radius: f32, ring: Color32) {
    painter.circle_stroke(
        center,
        radius + 3.5,
        Stroke::new(3.0, theme::with_alpha(ring, 0x40)),
    );
}

#[allow(clippy::too_many_arguments)]
pub fn draw_avatar(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    name: &str,
    email: &str,
    texture: Option<egui::TextureId>,
    ring: Color32,
    selected: bool,
) {
    if selected {
        selection_halo(painter, center, radius, ring);
    }
    match texture {
        Some(id) => {
            let rect = Rect::from_center_size(center, vec2(radius * 2.0, radius * 2.0));
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.image(id, rect, uv, Color32::WHITE);
            painter.circle_stroke(center, radius, Stroke::new(1.5, ring));
        }
        None => {
            let (fill, ink) = theme::generated_avatar(email);
            painter.circle(center, radius, fill, Stroke::new(2.0, ring));
            let galley = painter.layout_no_wrap(
                avatar::initials(name),
                FontId::new((radius * 0.8).max(8.0), theme::semibold()),
                ink,
            );
            let ink_center = galley.mesh_bounds.center().to_vec2();
            painter.galley(center - ink_center, galley, ink);
        }
    }
}

fn paint_labels(
    painter: &egui::Painter,
    geo: &RowGeo,
    labels: &[&RefLabel],
    node: Pos2,
    lane: Color32,
) -> Vec<ref_labels::Placed> {
    let placed = ref_labels::paint(
        painter,
        geo.left + LABELS_END,
        geo.left + 8.0,
        geo.mid(),
        labels,
        lane,
    );
    if !labels.is_empty() {
        let stroke = Stroke::new(1.0, theme::with_alpha(lane, 0x66));
        painter.line_segment(
            [
                pos2(geo.left + LABELS_END, node.y),
                pos2(node.x - AVATAR_R, node.y),
            ],
            stroke,
        );
    }
    placed
}

fn drop_menu_id() -> egui::Id {
    egui::Id::new("ref-drop-menu")
}

pub fn truncated(
    painter: &egui::Painter,
    text: String,
    font: FontId,
    color: Color32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::simple_singleline(text, font, color);
    job.wrap = TextWrapping::truncate_at_width(max_width);
    painter.layout_job(job)
}

#[allow(clippy::too_many_arguments)]
fn paint_message(
    painter: &egui::Painter,
    geo: &RowGeo,
    id: gix::ObjectId,
    summary: &Summary,
    selected: bool,
    descriptions: bool,
    now: i64,
) {
    let right = geo.message_right();
    crate::columns::paint_cells(
        painter,
        &geo.columns,
        geo.right,
        geo.mid(),
        id,
        summary,
        now,
    );
    let time_w = if geo.columns.date {
        0.0
    } else {
        let when = commit::relative_time(summary.time, now);
        let time_galley =
            painter.layout_no_wrap(when, FontId::proportional(11.0), theme::TEXT_FAINT);
        let time_w = time_galley.size().x;
        painter.galley(
            pos2(
                right - 14.0 - time_w,
                geo.mid() - time_galley.size().y / 2.0,
            ),
            time_galley,
            theme::TEXT_FAINT,
        );
        time_w
    };

    let x = geo.msg_left() + 15.0;
    let max_width = (right - 14.0 - time_w - 12.0 - x).max(0.0);
    let mut job = LayoutJob::default();
    let title_color = if selected {
        theme::TEXT_STRONG
    } else {
        theme::TEXT
    };
    let title_font = if selected {
        FontId::new(13.0, theme::semibold())
    } else {
        FontId::proportional(13.0)
    };
    job.append(
        &summary.title,
        0.0,
        TextFormat::simple(title_font, title_color),
    );
    if descriptions && !summary.body_preview.is_empty() {
        job.append(
            &summary.body_preview,
            10.0,
            TextFormat::simple(FontId::proportional(13.0), theme::TEXT_FAINT),
        );
    }
    job.wrap = TextWrapping {
        max_width,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = painter.layout_job(job);
    painter.galley(
        pos2(x, geo.mid() - galley.size().y / 2.0),
        galley,
        theme::TEXT,
    );
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use eframe::egui::{self, Event, Pos2, RawInput, Rect, vec2};
    use kelp_core::history::History;

    use super::{GraphInput, GraphView, HEADER_H, ROW_H};
    use crate::avatars::AvatarStore;

    fn scratch_repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kelp-graph-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            let ok = Command::new("git")
                .current_dir(&dir)
                .args(args)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        run(&["init", "-q", "-b", "main"]);
        run(&["config", "user.email", "t@example.com"]);
        run(&["config", "user.name", "T"]);
        for n in 0..3 {
            std::fs::write(dir.join("a.txt"), n.to_string()).unwrap();
            run(&["add", "."]);
            run(&["commit", "-q", "-m", &format!("commit {n}")]);
        }
        dir
    }

    struct Harness {
        ctx: egui::Context,
        view: GraphView,
        repo: gix::Repository,
        history: History,
        avatars: AvatarStore,
    }

    impl Harness {
        fn frame(&mut self, events: Vec<Event>) {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 400.0))),
                events,
                ..Default::default()
            };
            let Self {
                ctx,
                view,
                repo,
                history,
                avatars,
            } = self;
            let _ = ctx.run_ui(input, |ui| {
                let graph = GraphInput {
                    repo,
                    history,
                    selected: None,
                    head_row: Some(0),
                    wip: None,
                    lit: None,
                    descriptions: false,
                    other_wips: &[],
                    columns: Default::default(),
                };
                view.ui(ui, graph, avatars, |_, _| {});
            });
        }
    }

    #[test]
    fn hovering_a_row_traces_its_path_to_the_branch_tip() {
        let dir = scratch_repo("hover");
        let (repo, history) = History::open(Path::new(&dir)).unwrap();
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut avatars = AvatarStore::new(ctx.clone(), None);
        avatars.enabled = false;
        let mut h = Harness {
            ctx,
            view: GraphView::new(),
            repo,
            history,
            avatars,
        };
        h.frame(vec![]);
        assert!(h.view.hover.is_none());

        let pointer = Pos2::new(600.0, HEADER_H + ROW_H * 2.5);
        h.frame(vec![Event::PointerMoved(pointer)]);
        h.frame(vec![]);
        let (row, path) = h.view.hover.as_ref().expect("a hovered row");
        assert_eq!(*row, 2);
        assert!((0..=2).all(|r| path.contains(r)));

        h.frame(vec![Event::PointerGone]);
        assert!(h.view.hover.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
