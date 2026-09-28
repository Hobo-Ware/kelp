use serde::{Deserialize, Serialize};

pub const ZOOM_STEPS: [f32; 8] = [0.8, 0.9, 1.0, 1.1, 1.25, 1.4, 1.5, 1.6];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Sidebar,
    Details,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Panels {
    pub sidebar_open: bool,
    pub details_open: bool,
    pub sidebar_width: f32,
    pub details_width: f32,
}

impl Default for Panels {
    fn default() -> Self {
        Self {
            sidebar_open: true,
            details_open: true,
            sidebar_width: 250.0,
            details_width: 370.0,
        }
    }
}

impl Panels {
    pub fn toggle(&mut self, side: Side) {
        match side {
            Side::Sidebar => self.sidebar_open = !self.sidebar_open,
            Side::Details => self.details_open = !self.details_open,
        }
    }

    pub fn collapsed_by_env(mut self) -> Self {
        if let Ok(names) = std::env::var("KELP_COLLAPSE") {
            for name in names.split(',') {
                match name.trim() {
                    "sidebar" => self.sidebar_open = false,
                    "details" => self.details_open = false,
                    _ => {}
                }
            }
        }
        self
    }
}

pub fn zoom_step(current: f32, direction: i32) -> f32 {
    let index = ZOOM_STEPS
        .iter()
        .position(|&z| z >= current - 0.001)
        .unwrap_or(ZOOM_STEPS.len() - 1);
    let exact = (ZOOM_STEPS[index] - current).abs() < 0.001;
    let next = match direction {
        d if d > 0 && exact => index + 1,
        d if d > 0 => index,
        d if d < 0 => index.saturating_sub(1),
        _ => return 1.0,
    };
    ZOOM_STEPS[next.min(ZOOM_STEPS.len() - 1)]
}

pub fn clamp_zoom(zoom: f32) -> f32 {
    if zoom.is_finite() {
        zoom.clamp(ZOOM_STEPS[0], ZOOM_STEPS[ZOOM_STEPS.len() - 1])
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_steps_up_and_down_and_stops_at_the_ends() {
        assert_eq!(zoom_step(1.0, 1), 1.1);
        assert_eq!(zoom_step(1.1, 1), 1.25);
        assert_eq!(zoom_step(1.0, -1), 0.9);
        assert_eq!(zoom_step(0.8, -1), 0.8);
        assert_eq!(zoom_step(1.6, 1), 1.6);
        assert_eq!(zoom_step(1.33, 0), 1.0);
    }

    #[test]
    fn off_step_zoom_snaps_to_the_next_step() {
        assert_eq!(zoom_step(1.2, 1), 1.25);
        assert_eq!(zoom_step(1.2, -1), 1.1);
    }

    #[test]
    fn saved_zoom_is_kept_in_range() {
        assert_eq!(clamp_zoom(3.0), 1.6);
        assert_eq!(clamp_zoom(0.1), 0.8);
        assert_eq!(clamp_zoom(f32::NAN), 1.0);
    }

    #[test]
    fn toggling_flips_one_side() {
        let mut p = Panels::default();
        p.toggle(Side::Details);
        assert!(p.sidebar_open && !p.details_open);
    }
}
