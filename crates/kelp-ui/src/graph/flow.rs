use std::ops::Range;

#[derive(Debug, Clone, Copy)]
struct Gap {
    after: usize,
    height: f32,
}

/// Vertical positions of the display rows: uniform rows with an optional gap under one of them.
#[derive(Debug, Clone, Copy)]
pub struct Flow {
    row_h: f32,
    total: usize,
    gap: Option<Gap>,
}

impl Flow {
    pub fn new(row_h: f32, total: usize, gap: Option<(usize, f32)>) -> Self {
        let gap = gap
            .filter(|&(after, height)| after < total && height > 0.0)
            .map(|(after, height)| Gap { after, height });
        Self { row_h, total, gap }
    }

    /// Offset of the top of a display row from the top of the content.
    pub fn top(&self, display: usize) -> f32 {
        let pushed = self
            .gap
            .filter(|g| display > g.after)
            .map_or(0.0, |g| g.height);
        display as f32 * self.row_h + pushed
    }

    pub fn height(&self) -> f32 {
        self.top(self.total)
    }

    /// The display row the gap sits under, and the gap's offset range.
    pub fn gap(&self) -> Option<(usize, Range<f32>)> {
        self.gap.map(|g| {
            let start = (g.after + 1) as f32 * self.row_h;
            (g.after, start..start + g.height)
        })
    }

    /// The row under an offset; `None` inside the gap or past the last row.
    pub fn row_at(&self, y: f32) -> Option<usize> {
        if y < 0.0 {
            return None;
        }
        let row = match self.gap() {
            Some((_, span)) if y >= span.end => {
                ((y - (span.end - span.start)) / self.row_h) as usize
            }
            Some((_, span)) if y >= span.start => return None,
            _ => (y / self.row_h) as usize,
        };
        (row < self.total).then_some(row)
    }

    /// The rows to draw for a scrolled viewport, with a row of slack below. A visible gap pulls
    /// in the row above it and the row below it, so lanes can run through.
    pub fn visible(&self, view: Range<f32>) -> Range<usize> {
        let squash = |y: f32| match self.gap() {
            Some((_, span)) if y >= span.end => y - (span.end - span.start),
            Some((_, span)) if y > span.start => span.start,
            _ => y,
        };
        let mut min_row = (squash(view.start) / self.row_h).floor() as usize;
        let mut max_row = (squash(view.end) / self.row_h).ceil() as usize + 1;
        if max_row > self.total {
            let diff = max_row.saturating_sub(min_row);
            max_row = self.total;
            min_row = self.total.saturating_sub(diff);
        }
        if let Some((after, span)) = self.gap()
            && span.end > view.start
            && span.start < view.end
        {
            min_row = min_row.min(after);
            max_row = max_row.max((after + 2).min(self.total));
        }
        min_row..max_row
    }

    /// The scroll offset that brings a row near the top third of a viewport `visible` tall. When
    /// the gap sits under that row, the row and the whole gap stay in view.
    pub fn scroll_target(&self, display: usize, visible: f32) -> f32 {
        let top = self.top(display);
        let target = (top - visible / 3.0).max(0.0);
        match self.gap() {
            Some((after, span)) if after == display => target.max(span.end - visible).min(top),
            _ => target,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Flow;

    #[test]
    fn without_a_gap_rows_are_uniform() {
        let flow = Flow::new(30.0, 100, None);
        assert_eq!(flow.top(0), 0.0);
        assert_eq!(flow.top(7), 210.0);
        assert_eq!(flow.height(), 3000.0);
        assert_eq!(flow.gap(), None);
        assert_eq!(flow.row_at(59.9), Some(1));
        assert_eq!(flow.row_at(3000.0), None);
        assert_eq!(flow.scroll_target(10, 300.0), 200.0);
        assert_eq!(flow.scroll_target(1, 300.0), 0.0);
    }

    #[test]
    fn without_a_gap_the_visible_rows_are_what_show_rows_would_pick() {
        let flow = Flow::new(30.0, 100, None);
        assert_eq!(flow.visible(0.0..300.0), 0..11);
        assert_eq!(flow.visible(95.0..395.0), 3..15);
        assert_eq!(flow.visible(2800.0..3100.0), 88..100);
    }

    #[test]
    fn a_gap_pushes_the_rows_below_it_down() {
        let flow = Flow::new(30.0, 10, Some((2, 100.0)));
        assert_eq!(flow.top(2), 60.0);
        assert_eq!(flow.top(3), 90.0 + 100.0);
        assert_eq!(flow.top(9), 270.0 + 100.0);
        assert_eq!(flow.height(), 300.0 + 100.0);
        assert_eq!(flow.gap(), Some((2, 90.0..190.0)));
    }

    #[test]
    fn a_click_in_the_gap_hits_no_row() {
        let flow = Flow::new(30.0, 10, Some((2, 100.0)));
        assert_eq!(flow.row_at(89.0), Some(2));
        assert_eq!(flow.row_at(90.0), None);
        assert_eq!(flow.row_at(189.0), None);
        assert_eq!(flow.row_at(190.0), Some(3));
        assert_eq!(flow.row_at(399.0), Some(9));
        assert_eq!(flow.row_at(400.0), None);
    }

    #[test]
    fn a_gap_off_the_end_or_empty_is_ignored() {
        assert_eq!(Flow::new(30.0, 3, Some((3, 50.0))).height(), 90.0);
        assert_eq!(Flow::new(30.0, 3, Some((1, 0.0))).height(), 90.0);
    }

    #[test]
    fn a_visible_gap_brings_its_neighbours_along() {
        let flow = Flow::new(30.0, 100, Some((40, 200.0)));
        let gap = flow.gap().unwrap().1;
        let seen = flow.visible(gap.start + 20.0..gap.start + 60.0);
        assert!(seen.contains(&40) && seen.contains(&41), "{seen:?}");
        let above = flow.visible(0.0..300.0);
        assert_eq!(above, 0..11);
        let below = flow.visible(gap.end + 600.0..gap.end + 900.0);
        assert!(!below.contains(&40), "{below:?}");
    }

    #[test]
    fn scrolling_to_a_row_keeps_its_gap_in_view() {
        let flow = Flow::new(30.0, 100, Some((50, 200.0)));
        let visible = 300.0;
        let plain = Flow::new(30.0, 100, None).scroll_target(50, visible);
        let target = flow.scroll_target(50, visible);
        let top = flow.top(50);
        let bottom = flow.gap().unwrap().1.end;
        assert!(target > plain - 1.0);
        assert!(target <= top, "the row stays in view");
        assert!(target + visible >= bottom, "the gap's bottom is in view");
    }

    #[test]
    fn a_gap_taller_than_the_viewport_keeps_the_row_in_view() {
        let flow = Flow::new(30.0, 100, Some((50, 900.0)));
        let target = flow.scroll_target(50, 300.0);
        assert_eq!(target, flow.top(50));
    }

    #[test]
    fn a_gap_under_another_row_does_not_move_the_target_row() {
        let flow = Flow::new(30.0, 100, Some((50, 200.0)));
        assert_eq!(flow.scroll_target(10, 300.0), 200.0);
        assert_eq!(flow.scroll_target(60, 300.0), flow.top(60) - 100.0);
    }
}
