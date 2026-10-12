use egui::{
    self, Color32, FontId, Pos2, Rect, Stroke, Ui, pos2,
    text::{LayoutJob, TextWrapping},
    vec2,
};
use kelp_core::ObjectId;
use kelp_core::avatar;
use kelp_core::commit::{self, Summary};

use crate::theme;

pub const AVATAR_R: f32 = 10.0;

pub struct TooltipCommit<'a> {
    pub title: &'a str,
    pub body: &'a str,
    pub author: &'a str,
    pub email: &'a str,
    pub time: i64,
    pub id: ObjectId,
}

pub fn commit_tooltip(ui: &mut Ui, commit: &TooltipCommit<'_>, now: i64) {
    ui.set_max_width(420.0);
    ui.spacing_mut().item_spacing.y = 4.0;
    ui.label(
        egui::RichText::new(commit.title)
            .family(theme::semibold())
            .color(theme::text_strong()),
    );
    let body: Vec<&str> = commit
        .body
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(3)
        .collect();
    if !body.is_empty() {
        ui.label(egui::RichText::new(body.join("\n")).color(theme::text_muted()));
    }
    ui.label(
        egui::RichText::new(format!("{} <{}>", commit.author, commit.email))
            .size(12.0)
            .color(theme::text()),
    );
    ui.label(
        egui::RichText::new(format!(
            "{} · {}",
            commit::calendar_time(commit.time),
            commit::relative_time(commit.time, now)
        ))
        .size(12.0)
        .color(theme::text_faint()),
    );
    ui.label(
        egui::RichText::new(commit.id.to_string())
            .monospace()
            .size(11.0)
            .color(theme::text_faint()),
    );
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
