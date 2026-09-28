use eframe::egui::{
    self, Color32, CornerRadius, CursorIcon, FontId, Rect, RichText, Sense, Stroke, Ui, pos2, vec2,
};
use kelp_core::avatar::GitHubRepo;
use kelp_core::pulls::{self, Checks, Pull, State};

use crate::theme;

pub const PILL_H: f32 = 16.0;
const DOT_R: f32 = 2.5;
const PAD: f32 = 5.0;

pub fn state_color(state: State) -> Color32 {
    match state {
        State::Open => theme::accent(),
        State::Draft => theme::text_muted(),
        State::Merged => theme::lanes()[2],
        State::Closed => theme::deleted(),
    }
}

fn checks_color(checks: Checks) -> Color32 {
    match checks {
        Checks::Passing => theme::added(),
        Checks::Failing => theme::deleted(),
        Checks::Pending => theme::modified(),
    }
}

fn text(pull: &Pull) -> String {
    format!("#{}", pull.number)
}

pub fn pill_width(painter: &egui::Painter, pull: &Pull) -> f32 {
    let galley = painter.layout_no_wrap(text(pull), FontId::proportional(10.5), Color32::WHITE);
    let dot = if pull.checks.is_some() {
        DOT_R * 2.0 + 4.0
    } else {
        0.0
    };
    galley.size().x + PAD * 2.0 + dot
}

pub fn paint_pill(painter: &egui::Painter, rect: Rect, pull: &Pull, hovered: bool) {
    let color = state_color(pull.state);
    painter.rect(
        rect,
        CornerRadius::same(8),
        theme::with_alpha(color, if hovered { 0x44 } else { 0x26 }),
        Stroke::new(1.0, theme::with_alpha(color, 0x99)),
        egui::StrokeKind::Inside,
    );
    let galley = painter.layout_no_wrap(text(pull), FontId::proportional(10.5), color);
    let text_pos = pos2(rect.left() + PAD, rect.center().y - galley.size().y / 2.0);
    let text_right = text_pos.x + galley.size().x;
    painter.galley(text_pos, galley, color);
    if let Some(checks) = pull.checks {
        painter.circle_filled(
            pos2(text_right + 3.0 + DOT_R, rect.center().y),
            DOT_R,
            checks_color(checks),
        );
    }
}

pub fn tooltip(ui: &mut Ui, pull: &Pull) {
    ui.set_max_width(320.0);
    ui.label(
        RichText::new(format!("#{} {}", pull.number, pull.title))
            .family(theme::semibold())
            .color(theme::text_strong()),
    );
    let mut facts = vec![pull.state.label()];
    facts.extend(pull.review_text());
    facts.extend(pull.checks_text());
    ui.label(
        RichText::new(facts.join("  ·  "))
            .size(12.0)
            .color(state_color(pull.state)),
    );
    ui.label(
        RichText::new("Click to open on GitHub")
            .size(11.0)
            .color(theme::text_faint()),
    );
}

pub fn clicked_pill(ui: &mut Ui, pull: &Pull) -> bool {
    let width = pill_width(ui.painter(), pull);
    let (rect, response) = ui.allocate_exact_size(vec2(width, PILL_H), Sense::click());
    let response = response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_ui(|ui| tooltip(ui, pull));
    paint_pill(ui.painter(), rect, pull, response.hovered());
    response.clicked()
}

pub fn load(github: &GitHubRepo, force: bool) -> Vec<Pull> {
    if let Some(path) = std::env::var_os("KELP_FAKE_PULLS") {
        return std::fs::read_to_string(path)
            .ok()
            .and_then(|json| pulls::parse_gh(&json).ok())
            .unwrap_or_default();
    }
    let cache = pulls::cache_file(github);
    let cached = pulls::load_cached(&cache);
    if let Some((list, true)) = &cached
        && !force
    {
        return list.clone();
    }
    match pulls::fetch(github) {
        Ok(list) => {
            pulls::save_cached(&cache, &list);
            list
        }
        Err(_) => cached.map(|(list, _)| list).unwrap_or_default(),
    }
}

pub fn open_url(url: &str) -> std::io::Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(url)
        .spawn()
        .map(|_| ())
}
