use egui::Color32;

use crate::theme;

const LANES: usize = 8;

/// Where ref chips (branches, tags) are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefPlacement {
    /// Right-aligned in their own column left of the graph.
    Column,
    /// Small outlined chips on the commit's row, before its title. No column.
    Inline,
}

/// How the graph looks. `Default` is the Kelp app's look; [`GraphStyle::embedded`] is a flat
/// list for a narrow side panel in a host app.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphStyle {
    /// The column header row with the funnel and the resize handle.
    pub header: bool,
    pub refs: RefPlacement,
    /// Avatars on the nodes. Off draws plain dots (a hollow one for HEAD) on thin, translucent
    /// lane lines.
    pub avatars: bool,
    /// The tinted band behind each graph cell, the strip and divider before the message, and the
    /// full-width selection fill. Off leaves rows bare and selects with a rounded rect.
    pub row_bands: bool,
    /// Height of one row, in points.
    pub row_h: f32,
    /// Replaces the first lane color, and colors the selection when `row_bands` is off.
    pub accent: Option<Color32>,
    /// Distance between neighboring lanes. At least half of `row_h`, so the corners stay round.
    pub lane_w: f32,
    /// Space before the first lane.
    pub graph_lead: f32,
    /// Space after the last lane.
    pub graph_tail: f32,
    /// A dimmed conventional-commit prefix, a right-aligned relative date of fixed width and a
    /// message that takes all the remaining width. The host's `GraphColumns` are ignored.
    pub list_text: bool,
}

impl Default for GraphStyle {
    fn default() -> Self {
        Self {
            header: true,
            refs: RefPlacement::Column,
            avatars: true,
            row_bands: true,
            row_h: 30.0,
            accent: None,
            lane_w: 24.0,
            graph_lead: 16.0,
            graph_tail: 40.0,
            list_text: false,
        }
    }
}

impl GraphStyle {
    /// A clean list for a narrow panel: no header, no ref column, no avatars, no row bands,
    /// 36 px rows, with `accent` on the first lane, the dots and the selection.
    pub fn embedded(accent: Color32) -> Self {
        Self {
            header: false,
            refs: RefPlacement::Inline,
            avatars: false,
            row_bands: false,
            row_h: 36.0,
            accent: Some(accent),
            lane_w: 18.0,
            graph_lead: 17.0,
            graph_tail: 17.0,
            list_text: true,
        }
    }

    pub(super) fn lane(&self, color: u8) -> Color32 {
        match self.accent {
            Some(accent) if (color as usize).is_multiple_of(LANES) => accent,
            _ => theme::lane(color),
        }
    }

    pub(super) fn accent(&self) -> Color32 {
        self.accent.unwrap_or_else(theme::accent)
    }

    pub(super) fn arc_r(&self) -> f32 {
        self.row_h / 2.0
    }

    pub(super) fn lane_w(&self) -> f32 {
        self.lane_w.max(self.arc_r())
    }

    pub(super) fn labels_w(&self) -> f32 {
        match self.refs {
            RefPlacement::Column => super::view::LABELS_W,
            RefPlacement::Inline => 0.0,
        }
    }

    pub(super) fn content_w(&self, lanes: u16, gutter: f32) -> f32 {
        self.graph_lead + self.graph_tail + (lanes as f32 - 1.0) * self.lane_w() + gutter
    }

    pub(super) fn line_w(&self) -> f32 {
        if self.avatars { 2.25 } else { 2.0 }
    }

    pub(super) fn line_alpha(&self) -> f32 {
        if self.avatars { 1.0 } else { 0.5 }
    }

    pub(super) fn text_inset(&self) -> f32 {
        if self.row_bands { 15.0 } else { 0.0 }
    }

    pub(super) fn title_size(&self) -> f32 {
        if self.list_text { 13.5 } else { 13.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_style_is_the_kelp_look() {
        let style = GraphStyle::default();
        assert!(style.header);
        assert_eq!(style.refs, RefPlacement::Column);
        assert!(style.avatars);
        assert!(style.row_bands);
        assert!(!style.list_text);
        assert_eq!(style.accent, None);
        assert_eq!(style.row_h, super::super::ROW_H);
        assert_eq!(style.arc_r(), 15.0);
        assert_eq!(style.lane_w(), 24.0);
        assert_eq!(style.labels_w(), 170.0);
        assert_eq!(style.line_w(), 2.25);
        assert_eq!(style.line_alpha(), 1.0);
        for lanes in [0, 1, 5, 14] {
            assert_eq!(style.content_w(lanes, 0.0), lanes as f32 * 24.0 + 32.0);
        }
        assert_eq!(style.content_w(3, 24.0), 3.0 * 24.0 + 24.0 + 32.0);
        for color in 0..10 {
            assert_eq!(style.lane(color), theme::lane(color));
        }
    }

    #[test]
    fn the_accent_replaces_only_the_first_lane_color() {
        let accent = Color32::from_rgb(1, 2, 3);
        let style = GraphStyle::embedded(accent);
        assert_eq!(style.lane(0), accent);
        assert_eq!(style.lane(8), accent);
        for color in 1..8 {
            assert_eq!(style.lane(color), theme::lane(color));
        }
    }

    #[test]
    fn corners_stay_round_when_lanes_are_closer_than_half_a_row() {
        let style = GraphStyle {
            lane_w: 4.0,
            ..GraphStyle::default()
        };
        assert_eq!(style.lane_w(), style.arc_r());
    }

    #[test]
    fn the_embedded_look_is_a_flat_list() {
        let style = GraphStyle::embedded(Color32::WHITE);
        assert!(!style.header);
        assert_eq!(style.refs, RefPlacement::Inline);
        assert!(!style.avatars);
        assert!(!style.row_bands);
        assert!(style.list_text);
        assert_eq!(style.row_h, 36.0);
        assert_eq!(style.labels_w(), 0.0);
        assert_eq!(style.content_w(1, 0.0), 34.0);
    }
}
