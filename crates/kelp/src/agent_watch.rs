use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Rect, TextureHandle, pos2};
use kelp_core::agents::{self, Agent, Session};

const SCAN_EVERY: Duration = Duration::from_secs(10);

struct Watch {
    sessions: Mutex<Vec<Session>>,
    focused: AtomicBool,
    wanted: AtomicBool,
    scanner: Option<std::thread::Thread>,
}

static WATCH: OnceLock<Watch> = OnceLock::new();

pub fn init(ctx: &egui::Context) {
    WATCH.get_or_init(|| {
        if let Ok(spec) = std::env::var("KELP_FAKE_AGENTS") {
            return Watch {
                sessions: Mutex::new(agents::fake_sessions(&spec)),
                focused: AtomicBool::new(false),
                wanted: AtomicBool::new(false),
                scanner: None,
            };
        }
        let ctx = ctx.clone();
        let scanner = std::thread::spawn(move || scan_forever(ctx));
        Watch {
            sessions: Mutex::new(Vec::new()),
            focused: AtomicBool::new(true),
            wanted: AtomicBool::new(false),
            scanner: Some(scanner.thread().clone()),
        }
    });
}

fn scan_forever(ctx: egui::Context) {
    let mut cache = HashMap::new();
    loop {
        if let Some(watch) = WATCH.get()
            && watch.focused.load(Ordering::Relaxed)
            && watch.wanted.load(Ordering::Relaxed)
        {
            let found = agents::sessions_cached(&mut cache);
            if let Ok(mut current) = watch.sessions.lock()
                && *current != found
            {
                *current = found;
                ctx.request_repaint();
            }
        }
        std::thread::park_timeout(SCAN_EVERY);
    }
}

pub fn set_focused(focused: bool) {
    if let Some(watch) = WATCH.get()
        && watch.focused.swap(focused, Ordering::Relaxed) != focused
        && focused
        && let Some(scanner) = &watch.scanner
    {
        scanner.unpark();
    }
}

pub fn set_wanted(wanted: bool) {
    if let Some(watch) = WATCH.get()
        && watch.wanted.swap(wanted, Ordering::Relaxed) != wanted
        && wanted
        && let Some(scanner) = &watch.scanner
    {
        scanner.unpark();
    }
}

pub fn in_worktrees(worktrees: &[PathBuf]) -> HashMap<PathBuf, Vec<Agent>> {
    let Some(watch) = WATCH.get() else {
        return HashMap::new();
    };
    match watch.sessions.lock() {
        Ok(sessions) => agents::by_worktree(worktrees, &sessions),
        Err(_) => HashMap::new(),
    }
}

fn svg_of(agent: Agent) -> Option<&'static [u8]> {
    Some(match agent {
        Agent::Claude => include_bytes!("../assets/agents/anthropic.svg"),
        Agent::Gemini => include_bytes!("../assets/agents/googlegemini.svg"),
        Agent::Copilot => include_bytes!("../assets/agents/githubcopilot.svg"),
        Agent::Cursor => include_bytes!("../assets/agents/cursor.svg"),
        Agent::Codex | Agent::Aider => return None,
    })
}

const GLYPH_PX: u32 = 64;

fn white_glyph(svg: &[u8]) -> Option<egui::ColorImage> {
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(GLYPH_PX, GLYPH_PX)?;
    let scale = GLYPH_PX as f32 / tree.size().width().max(tree.size().height());
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let rgba: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|p| [255, 255, 255, p.alpha()])
        .collect();
    let side = GLYPH_PX as usize;
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [side, side],
        &rgba,
    ))
}

fn glyph(ctx: &egui::Context, agent: Agent) -> Option<TextureHandle> {
    let id = egui::Id::new(("agent-glyph", agent.label()));
    if let Some(cached) = ctx.data(|d| d.get_temp::<Option<TextureHandle>>(id)) {
        return cached;
    }
    let texture = svg_of(agent).and_then(white_glyph).map(|image| {
        ctx.load_texture(
            format!("agent-glyph-{}", agent.label()),
            image,
            egui::TextureOptions::LINEAR,
        )
    });
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    texture
}

pub fn paint_badge(painter: &egui::Painter, ctx: &egui::Context, rect: Rect, agent: Agent) {
    let brand = crate::theme::agent_brand(agent);
    match glyph(ctx, agent) {
        Some(texture) => {
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.image(texture.id(), rect, uv, brand);
        }
        None => {
            let luminance = 0.299 * f32::from(brand.r())
                + 0.587 * f32::from(brand.g())
                + 0.114 * f32::from(brand.b());
            let ink = if luminance > 150.0 {
                Color32::BLACK
            } else {
                Color32::WHITE
            };
            let initial = agent.label().chars().next().unwrap_or('?');
            painter.rect_filled(rect, CornerRadius::same(4), brand);
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                initial,
                FontId::proportional(rect.height() * 0.72),
                ink,
            );
        }
    }
}

pub fn summary(agents: &[Agent]) -> String {
    let mut counts: Vec<(Agent, usize)> = Vec::new();
    for agent in agents {
        match counts.iter_mut().find(|(a, _)| a == agent) {
            Some((_, n)) => *n += 1,
            None => counts.push((*agent, 1)),
        }
    }
    let parts: Vec<String> = counts
        .iter()
        .map(|(agent, n)| match n {
            1 => agent.label().to_string(),
            n => format!("{} x{n}", agent.label()),
        })
        .collect();
    format!("{} running in this worktree", parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_groups_sessions_by_tool() {
        assert_eq!(summary(&[Agent::Claude]), "Claude running in this worktree");
        assert_eq!(
            summary(&[Agent::Claude, Agent::Claude, Agent::Codex]),
            "Claude x2, Codex running in this worktree"
        );
    }
}
