use egui::{self, FontId};

use crate::theme;

pub const MARK_A: &str = "A";
pub const MARK_B: &str = "B";

/// The commits picked for a comparison, by row.
#[derive(Clone, Copy, Default)]
pub struct Marks {
    pub base: Option<usize>,
    pub target: Option<usize>,
    /// The right-hand side is the working tree, not a commit.
    pub work_tree: bool,
}

impl Marks {
    pub fn mark(&self, row: usize) -> Option<&'static str> {
        if self.base == Some(row) {
            Some(MARK_A)
        } else if self.target == Some(row) {
            Some(MARK_B)
        } else {
            None
        }
    }
}

pub fn paint_mark(painter: &egui::Painter, center: egui::Pos2, mark: &str) {
    painter.circle_filled(center, 7.0, theme::with_alpha(theme::accent(), 0x40));
    painter.circle_stroke(
        center,
        7.0,
        egui::Stroke::new(1.0, theme::with_alpha(theme::accent(), 0xc0)),
    );
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        mark,
        FontId::new(9.5, theme::semibold()),
        theme::accent(),
    );
}
