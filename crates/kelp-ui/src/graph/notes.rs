use egui::{Color32, FontId, Painter, Rect, pos2, vec2};

use crate::icons::{self, Icon};

const ICON: f32 = 12.0;
const ICON_GAP: f32 = 3.0;
const SIZE: f32 = 11.5;
const MOST: usize = 99;

/// Space on each side of a notes badge.
pub const GAP: f32 = 6.0;

/// The count as drawn: the number, or `99+`.
pub fn label(count: usize) -> String {
    if count > MOST {
        format!("{MOST}+")
    } else {
        count.to_string()
    }
}

pub fn width(painter: &Painter, count: usize) -> f32 {
    let text = painter.layout_no_wrap(label(count), FontId::proportional(SIZE), Color32::WHITE);
    ICON + ICON_GAP + text.size().x
}

/// Draws a speech bubble and `count` with the badge's right edge at `right`, centered on `mid`.
pub fn paint(painter: &Painter, right: f32, mid: f32, count: usize, color: Color32) {
    let galley = painter.layout_no_wrap(label(count), FontId::proportional(SIZE), color);
    let left = right - ICON - ICON_GAP - galley.size().x;
    let spot = Rect::from_center_size(pos2(left + ICON / 2.0, mid), vec2(ICON, ICON));
    icons::paint(painter, spot, Icon::Comment, color);
    painter.galley(
        pos2(left + ICON + ICON_GAP, mid - galley.size().y / 2.0),
        galley,
        color,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_counts_are_capped() {
        assert_eq!(label(1), "1");
        assert_eq!(label(99), "99");
        assert_eq!(label(100), "99+");
    }
}
