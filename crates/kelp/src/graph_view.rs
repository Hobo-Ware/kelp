use std::collections::HashMap;
use std::f32::consts::PI;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2,
    text::{LayoutJob, TextFormat, TextWrapping},
    vec2,
};
use kelp_core::avatar;
use kelp_core::commit::{self, Summary};
use kelp_core::graph::EdgeKind;
use kelp_core::history::History;
use kelp_core::refs::{RefKind, RefLabel};

use crate::theme;

pub const ROW_H: f32 = 30.0;
const LANE_W: f32 = 24.0;
const ARC_R: f32 = ROW_H / 2.0;
const AVATAR_R: f32 = 10.0;
const LINE_W: f32 = 2.25;
const LABELS_W: f32 = 170.0;
const LABELS_END: f32 = 158.0;
const GRAPH_PAD: f32 = 16.0;
const MIN_GRAPH_W: f32 = 120.0;
const MAX_VISIBLE_LANES: f32 = 14.0;
const HEADER_H: f32 = 28.0;

pub struct GraphView {
    summaries: HashMap<usize, Summary>,
    pub scroll_to: Option<usize>,
}

pub enum Action {
    Select(usize),
}

impl GraphView {
    pub fn new() -> Self {
        Self {
            summaries: HashMap::new(),
            scroll_to: None,
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        repo: &gix::Repository,
        history: &History,
        selected: Option<usize>,
    ) -> Option<Action> {
        let graph_w = (history.layout.lane_count() as f32 * LANE_W + GRAPH_PAD * 2.0)
            .clamp(MIN_GRAPH_W, MAX_VISIBLE_LANES * LANE_W + GRAPH_PAD * 2.0);
        let msg_x = LABELS_W + graph_w;
        header(ui, msg_x);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let mut action = None;
        let mut scroll = egui::ScrollArea::vertical().auto_shrink(false);
        if let Some(row) = self.scroll_to.take() {
            let visible = ui.available_height();
            let target = (row as f32 * ROW_H - visible / 3.0).max(0.0);
            scroll = scroll.vertical_scroll_offset(target);
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        scroll.show_rows(ui, ROW_H, history.len(), |ui, rows| {
            for row in rows.clone() {
                self.summaries
                    .entry(row)
                    .or_insert_with(|| load_summary(repo, history, row));
            }
            let size = vec2(ui.available_width(), ROW_H * rows.len() as f32);
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let painter = ui.painter_at(rect);
            for (i, row) in rows.clone().enumerate() {
                let top = rect.top() + i as f32 * ROW_H;
                let geo = RowGeo {
                    left: rect.left(),
                    right: rect.right(),
                    top,
                    msg_x,
                };
                let summary = &self.summaries[&row];
                paint_row(
                    &painter,
                    &geo,
                    history,
                    row,
                    summary,
                    selected == Some(row),
                    now,
                );
            }
            if response.clicked()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let row = rows.start + ((pos.y - rect.top()) / ROW_H) as usize;
                action = Some(Action::Select(row.min(history.len().saturating_sub(1))));
            }
        });
        action
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

fn header(ui: &mut Ui, msg_x: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), HEADER_H), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::HEADER);
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, theme::BORDER),
    );
    let font = FontId::monospace(10.0);
    let y = rect.center().y;
    for (x, label) in [
        (12.0, "BRANCH / TAG"),
        (LABELS_W + 8.0, "GRAPH"),
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
}

struct RowGeo {
    left: f32,
    right: f32,
    top: f32,
    msg_x: f32,
}

impl RowGeo {
    fn lane_x(&self, lane: u16) -> f32 {
        self.left + LABELS_W + GRAPH_PAD + lane as f32 * LANE_W
    }
    fn mid(&self) -> f32 {
        self.top + ROW_H / 2.0
    }
    fn bottom(&self) -> f32 {
        self.top + ROW_H
    }
    fn msg_left(&self) -> f32 {
        self.left + self.msg_x
    }
}

fn paint_row(
    painter: &egui::Painter,
    geo: &RowGeo,
    history: &History,
    row: usize,
    summary: &Summary,
    selected: bool,
    now: i64,
) {
    let layout = &history.layout;
    let node_lane = layout.node_lane(row);
    let color = theme::lane(layout.node_color(row));
    let node = pos2(geo.lane_x(node_lane), geo.mid());
    let graph_clip = Rect::from_x_y_ranges(geo.left..=geo.msg_left(), geo.top..=geo.bottom());
    let graph = painter.with_clip_rect(graph_clip.intersect(painter.clip_rect()));

    let band = Rect::from_x_y_ranges(node.x..=geo.msg_left(), geo.top + 4.0..=geo.bottom() - 4.0);
    graph.rect_filled(
        band,
        0.0,
        theme::with_alpha(color, if selected { 0x4d } else { 0x17 }),
    );
    let strip = Rect::from_x_y_ranges(
        geo.msg_left()..=geo.msg_left() + 3.0,
        geo.top..=geo.bottom(),
    );
    painter.rect_filled(strip, 0.0, color);
    if selected {
        let bg = Rect::from_x_y_ranges(geo.msg_left() + 3.0..=geo.right, geo.top..=geo.bottom());
        painter.rect_filled(bg, 0.0, theme::SELECTED_ROW);
    }

    let visible_lanes = ((geo.msg_x - LABELS_W - GRAPH_PAD) / LANE_W).ceil() as u16 + 1;
    for edge in layout.edges(row) {
        if edge.lane > visible_lanes && edge.kind == EdgeKind::Pass {
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

    paint_labels(painter, geo, history.refs.at_row(row), node, color);
    paint_avatar(&graph, node, summary, color, selected);
    paint_message(painter, geo, summary, selected, now);
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

fn paint_avatar(
    painter: &egui::Painter,
    node: Pos2,
    summary: &Summary,
    lane: Color32,
    selected: bool,
) {
    if selected {
        painter.circle_stroke(
            node,
            AVATAR_R + 3.5,
            Stroke::new(3.0, theme::with_alpha(lane, 0x40)),
        );
    }
    let fill = theme::AVATARS[avatar::color_index(&summary.email, theme::AVATARS.len())];
    painter.circle(node, AVATAR_R, fill, Stroke::new(2.0, lane));
    painter.text(
        node,
        Align2::CENTER_CENTER,
        avatar::initials(&summary.author),
        FontId::proportional(8.0),
        theme::AVATAR_INK,
    );
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
        RefKind::Local if label.is_head => {
            (lane, Stroke::NONE, Color32::from_rgb(0x0f, 0x1a, 0x1a))
        }
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
    job.append(
        &summary.title,
        0.0,
        TextFormat::simple(FontId::proportional(13.0), title_color),
    );
    if !summary.body_preview.is_empty() {
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
