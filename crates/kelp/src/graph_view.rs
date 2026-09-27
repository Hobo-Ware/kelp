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
use kelp_core::refs::{RefKind, RefLabel};

use crate::avatars::AvatarStore;
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

pub struct GraphView {
    summaries: HashMap<usize, Summary>,
    pub scroll_to: Option<Selection>,
    graph_w: Option<f32>,
    lane_offset: f32,
    context: Option<Selection>,
}

pub struct GraphInput<'a> {
    pub repo: &'a gix::Repository,
    pub history: &'a History,
    pub selected: Option<Selection>,
    pub wip: Option<Wip<'a>>,
    pub lit: Option<&'a [bool]>,
    pub descriptions: bool,
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

pub enum Action {
    Select(Selection),
}

#[derive(Clone, Copy)]
struct RowMap {
    wip_at: Option<usize>,
    commits: usize,
}

impl RowMap {
    fn total(&self) -> usize {
        self.commits + self.wip_at.is_some() as usize
    }

    fn resolve(&self, display: usize) -> Selection {
        match self.wip_at {
            Some(w) if display == w => Selection::Wip,
            Some(w) if display > w => Selection::Commit(display - 1),
            _ => Selection::Commit(display),
        }
    }

    fn display(&self, selection: Selection) -> usize {
        match (selection, self.wip_at) {
            (Selection::Wip, Some(w)) => w,
            (Selection::Wip, None) => 0,
            (Selection::Commit(r), Some(w)) if r >= w => r + 1,
            (Selection::Commit(r), _) => r,
        }
    }
}

impl GraphView {
    pub fn new() -> Self {
        Self {
            summaries: HashMap::new(),
            scroll_to: None,
            graph_w: None,
            lane_offset: 0.0,
            context: None,
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
        mut menu: impl FnMut(&mut Ui, Selection, &str),
    ) -> Option<Action> {
        let GraphInput {
            repo,
            history,
            selected,
            wip,
            lit,
            descriptions,
        } = input;
        let content_w = history.layout.lane_count() as f32 * LANE_W + GRAPH_PAD * 2.0;
        let auto_w = content_w.clamp(120.0, DEFAULT_MAX_LANES * LANE_W + GRAPH_PAD * 2.0);
        let max_w = (ui.available_width() * 0.6).max(MIN_GRAPH_W);
        let graph_w = self.graph_w.unwrap_or(auto_w).clamp(MIN_GRAPH_W, max_w);
        self.lane_offset = self.lane_offset.clamp(0.0, (content_w - graph_w).max(0.0));
        let msg_x = LABELS_W + graph_w;
        self.header(ui, msg_x, auto_w, content_w > graph_w);

        let map = RowMap {
            wip_at: wip.as_ref().map(|w| w.head_row),
            commits: history.len(),
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let mut action = None;
        let mut scroll = egui::ScrollArea::vertical().auto_shrink(false);
        if let Some(selection) = self.scroll_to.take() {
            let visible = ui.available_height();
            let target = (map.display(selection) as f32 * ROW_H - visible / 3.0).max(0.0);
            scroll = scroll.vertical_scroll_offset(target);
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        let lane_offset = &mut self.lane_offset;
        let summaries = &mut self.summaries;
        let context = &mut self.context;
        scroll.show_rows(ui, ROW_H, map.total(), |ui, rows| {
            for display in rows.clone() {
                if let Selection::Commit(row) = map.resolve(display) {
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
            let painter = ui.painter_at(rect);
            for (i, display) in rows.clone().enumerate() {
                let geo = RowGeo {
                    left: rect.left(),
                    right: rect.right(),
                    top: rect.top() + i as f32 * ROW_H,
                    msg_x,
                    lane_offset: *lane_offset,
                };
                let selection = map.resolve(display);
                let is_selected = selected == Some(selection);
                match (selection, &wip) {
                    (Selection::Wip, Some(wip)) => {
                        paint_wip_row(&painter, &geo, history, wip, is_selected)
                    }
                    (Selection::Commit(row), _) => {
                        let dashed_top = map.wip_at == Some(row);
                        let avatar = avatars.texture(&summaries[&row].email, history.id(row));
                        let style = RowStyle {
                            selected: is_selected,
                            dashed_top,
                            faded: lit.is_some_and(|l| !l.get(row).copied().unwrap_or(true)),
                            descriptions,
                        };
                        paint_row(
                            &painter,
                            &geo,
                            history,
                            row,
                            &summaries[&row],
                            avatar,
                            style,
                            now,
                        );
                    }
                    (Selection::Wip, None) => {}
                }
            }
            if (response.clicked() || response.secondary_clicked())
                && let Some(pos) = response.interact_pointer_pos()
            {
                let display = rows.start + ((pos.y - rect.top()) / ROW_H) as usize;
                let selection = map.resolve(display.min(map.total().saturating_sub(1)));
                action = Some(Action::Select(selection));
                if response.secondary_clicked() {
                    *context = Some(selection);
                }
            }
            if let Some(selection) = *context {
                let title = match selection {
                    Selection::Commit(row) => summaries
                        .get(&row)
                        .map(|s| s.title.clone())
                        .unwrap_or_default(),
                    Selection::Wip => String::new(),
                };
                response.context_menu(|ui| menu(ui, selection, &title));
            }
        });
        action
    }

    fn header(&mut self, ui: &mut Ui, msg_x: f32, auto_w: f32, scrollable: bool) {
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

#[allow(clippy::too_many_arguments)]
fn paint_row(
    painter: &egui::Painter,
    geo: &RowGeo,
    history: &History,
    row: usize,
    summary: &Summary,
    avatar: Option<egui::TextureId>,
    style: RowStyle,
    now: i64,
) {
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
        let stroke = Stroke::new(LINE_W, theme::lane(edge.color));
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

    if node.x >= geo.graph_left() {
        paint_labels(&soft, geo, history.refs.at_row(row), node, color);
    }
    paint_avatar(&graph_soft, node, summary, avatar, color, selected);
    paint_message(&soft, geo, summary, selected, descriptions, now);
}

fn paint_wip_row(
    painter: &egui::Painter,
    geo: &RowGeo,
    history: &History,
    wip: &Wip<'_>,
    selected: bool,
) {
    let layout = &history.layout;
    let head_lane = layout.node_lane(wip.head_row);
    let head_color = theme::lane(layout.node_color(wip.head_row));
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
    for edge in layout.edges(wip.head_row) {
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
    graph.text(
        node,
        Align2::CENTER_CENTER,
        "+",
        FontId::proportional(13.0),
        head_color,
    );

    let count = |k: ChangeKind| wip.changes.iter().filter(|c| c.kind == k).count();
    let parts: Vec<String> = [
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
    .collect();
    let mut job = LayoutJob::default();
    let italic = TextFormat {
        italics: true,
        ..TextFormat::simple(FontId::proportional(13.0), theme::TEXT_MUTED)
    };
    job.append("Uncommitted changes", 0.0, italic);
    job.append(
        &parts.join(" · "),
        10.0,
        TextFormat::simple(FontId::proportional(13.0), theme::TEXT_FAINT),
    );
    let galley = painter.layout_job(job);
    painter.galley(
        pos2(geo.msg_left() + 15.0, geo.mid() - galley.size().y / 2.0),
        galley,
        theme::TEXT,
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
    selected: bool,
) {
    draw_avatar(
        painter,
        node,
        AVATAR_R,
        &summary.author,
        &summary.email,
        texture,
        lane,
        selected,
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
        painter.circle_stroke(
            center,
            radius + 3.5,
            Stroke::new(3.0, theme::with_alpha(ring, 0x40)),
        );
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
            painter.text(
                center,
                Align2::CENTER_CENTER,
                avatar::initials(name),
                FontId::new((radius * 0.8).max(8.0), theme::semibold()),
                ink,
            );
        }
    }
}

fn paint_labels<'a>(
    painter: &egui::Painter,
    geo: &RowGeo,
    labels: impl Iterator<Item = &'a RefLabel>,
    node: Pos2,
    lane: Color32,
) {
    let font = FontId::proportional(12.0);
    let mut right = geo.left + LABELS_END;
    let labels: Vec<&RefLabel> = labels.collect();
    for (i, label) in labels.iter().enumerate() {
        let text = match label.kind {
            RefKind::Local if label.is_head => format!("✔ {}", label.name),
            _ => label.name.clone(),
        };
        let galley = truncated(painter, text, font.clone(), Color32::PLACEHOLDER, 114.0);
        let w = galley.size().x + 16.0;
        let left_edge = geo.left + 8.0;
        let remaining = labels.len() - i;
        if right - w < left_edge || (i > 0 && right - w - 30.0 < left_edge) {
            let more = format!("+{remaining}");
            let g = painter.layout_no_wrap(more, font.clone(), theme::TEXT_MUTED);
            let r = Rect::from_min_size(
                pos2(right - g.size().x - 12.0, geo.mid() - 11.0),
                vec2(g.size().x + 12.0, 22.0),
            );
            painter.rect_filled(r, CornerRadius::same(5), theme::with_alpha(lane, 0x26));
            painter.galley(
                pos2(r.left() + 6.0, geo.mid() - g.size().y / 2.0),
                g,
                theme::TEXT_MUTED,
            );
            break;
        }
        let rect = Rect::from_min_size(pos2(right - w, geo.mid() - 11.0), vec2(w, 22.0));
        let (fill, stroke, ink) = label_style(label, lane);
        painter.rect(
            rect,
            CornerRadius::same(5),
            fill,
            stroke,
            egui::StrokeKind::Inside,
        );
        painter.galley(
            pos2(rect.left() + 8.0, geo.mid() - galley.size().y / 2.0),
            galley,
            ink,
        );
        right -= w + 4.0;
    }
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

fn label_style(label: &RefLabel, lane: Color32) -> (Color32, Stroke, Color32) {
    match label.kind {
        RefKind::Local if label.is_head => (
            theme::with_alpha(lane, 0x40),
            Stroke::new(1.0, theme::with_alpha(lane, 0xcc)),
            lane,
        ),
        RefKind::Local => (
            theme::with_alpha(lane, 0x26),
            Stroke::new(1.0, theme::with_alpha(lane, 0x99)),
            theme::TEXT_STRONG,
        ),
        RefKind::Remote => (
            Color32::TRANSPARENT,
            Stroke::new(1.0, theme::with_alpha(lane, 0x99)),
            theme::TEXT_STRONG,
        ),
        RefKind::Tag => (
            Color32::from_rgb(0x1f, 0x23, 0x2b),
            Stroke::new(1.0, Color32::from_rgb(0x4a, 0x51, 0x60)),
            Color32::from_rgb(0xc9, 0xcc, 0xd2),
        ),
    }
}

fn paint_message(
    painter: &egui::Painter,
    geo: &RowGeo,
    summary: &Summary,
    selected: bool,
    descriptions: bool,
    now: i64,
) {
    let when = commit::relative_time(summary.time, now);
    let time_font = FontId::proportional(11.0);
    let time_galley = painter.layout_no_wrap(when, time_font, theme::TEXT_FAINT);
    let time_w = time_galley.size().x;
    painter.galley(
        pos2(
            geo.right - 14.0 - time_w,
            geo.mid() - time_galley.size().y / 2.0,
        ),
        time_galley,
        theme::TEXT_FAINT,
    );

    let x = geo.msg_left() + 15.0;
    let max_width = (geo.right - 14.0 - time_w - 12.0 - x).max(0.0);
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
