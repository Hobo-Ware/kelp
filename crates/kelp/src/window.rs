use std::time::{Duration, Instant};

use eframe::egui::{self, Rect};

pub const MIN_SIZE: [f32; 2] = [900.0, 560.0];
pub const DEFAULT_SIZE: [f32; 2] = [1440.0, 900.0];
const MAX_SIDE: f32 = 16_384.0;
const MAX_OFFSET: f32 = 20_000.0;
const SAVE_AFTER: Duration = Duration::from_millis(600);

pub struct Placement {
    pub size: [f32; 2],
    pub position: Option<[f32; 2]>,
}

pub fn restore(saved: Option<[f32; 4]>) -> Placement {
    let Some([x, y, w, h]) = saved.filter(|g| g.iter().all(|v| v.is_finite())) else {
        return Placement {
            size: DEFAULT_SIZE,
            position: None,
        };
    };
    let size = [
        w.clamp(MIN_SIZE[0], MAX_SIDE),
        h.clamp(MIN_SIZE[1], MAX_SIDE),
    ];
    let on_some_screen = y >= 0.0 && x > -MAX_OFFSET && x < MAX_OFFSET && y < MAX_OFFSET;
    Placement {
        size,
        position: on_some_screen.then_some([x, y]),
    }
}

pub fn capture(outer: Rect, inner: Rect, zoom: f32) -> [f32; 4] {
    [
        (outer.min.x * zoom).round(),
        (outer.min.y * zoom).round(),
        (inner.width() * zoom).round(),
        (inner.height() * zoom).round(),
    ]
}

#[derive(Default)]
pub struct Tracker {
    pending: Option<([f32; 4], Instant)>,
}

impl Tracker {
    pub fn settled_change(
        &mut self,
        ctx: &egui::Context,
        saved: Option<[f32; 4]>,
    ) -> Option<[f32; 4]> {
        let geometry = current(ctx)?;
        if Some(geometry) == saved {
            self.pending = None;
            return None;
        }
        match self.pending {
            Some((pending, since)) if pending == geometry => {
                let left = SAVE_AFTER.saturating_sub(since.elapsed());
                if left.is_zero() {
                    self.pending = None;
                    return Some(geometry);
                }
                ctx.request_repaint_after(left);
            }
            _ => {
                self.pending = Some((geometry, Instant::now()));
                ctx.request_repaint_after(SAVE_AFTER);
            }
        }
        None
    }
}

pub fn current(ctx: &egui::Context) -> Option<[f32; 4]> {
    let zoom = ctx.zoom_factor();
    ctx.input(|i| {
        let viewport = i.viewport();
        if viewport.fullscreen.unwrap_or(false) || viewport.minimized.unwrap_or(false) {
            return None;
        }
        Some(capture(viewport.outer_rect?, viewport.inner_rect?, zoom))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoomed_geometry_is_saved_in_screen_points() {
        let outer = Rect::from_min_size(egui::pos2(80.0, 40.0), egui::vec2(1152.0, 736.0));
        let inner = Rect::from_min_size(egui::pos2(80.0, 40.0), egui::vec2(1152.0, 720.0));
        assert_eq!(capture(outer, inner, 1.25), [100.0, 50.0, 1440.0, 900.0]);
    }

    #[test]
    fn nothing_saved_uses_the_default_size() {
        let p = restore(None);
        assert_eq!(p.size, DEFAULT_SIZE);
        assert_eq!(p.position, None);
    }

    #[test]
    fn tiny_or_huge_sizes_are_clamped() {
        assert_eq!(restore(Some([10.0, 40.0, 200.0, 100.0])).size, MIN_SIZE);
        assert_eq!(
            restore(Some([10.0, 40.0, 90_000.0, 90_000.0])).size,
            [MAX_SIDE, MAX_SIDE]
        );
    }

    #[test]
    fn a_position_off_every_screen_is_dropped() {
        assert_eq!(
            restore(Some([120.0, 60.0, 1200.0, 800.0])).position,
            Some([120.0, 60.0])
        );
        assert_eq!(
            restore(Some([-1600.0, 60.0, 1200.0, 800.0])).position,
            Some([-1600.0, 60.0])
        );
        assert_eq!(restore(Some([120.0, -300.0, 1200.0, 800.0])).position, None);
        assert_eq!(
            restore(Some([90_000.0, 60.0, 1200.0, 800.0])).position,
            None
        );
        assert_eq!(
            restore(Some([f32::NAN, 60.0, 1200.0, 800.0])).position,
            None
        );
    }
}
