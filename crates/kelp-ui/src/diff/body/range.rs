use kelp_core::review::{Anchor, Side};

/// A block of lines picked in one hunk and on one side of the diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LineRange {
    pub side: Side,
    pub hunk: usize,
    pub anchor: u32,
    pub head: u32,
}

impl LineRange {
    pub fn new(side: Side, hunk: usize, line: u32) -> Self {
        Self {
            side,
            hunk,
            anchor: line,
            head: line,
        }
    }

    pub fn first(&self) -> u32 {
        self.anchor.min(self.head)
    }

    pub fn last(&self) -> u32 {
        self.anchor.max(self.head)
    }

    pub fn is_multi(&self) -> bool {
        self.first() != self.last()
    }

    pub fn contains(&self, line: u32) -> bool {
        (self.first()..=self.last()).contains(&line)
    }

    pub fn extends_to(&self, side: Side, hunk: usize) -> bool {
        self.side == side && self.hunk == hunk
    }
}

/// A row that can end a selection: where it is on screen and which line it holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Candidate {
    pub top: f32,
    pub number: u32,
}

/// The line the pointer is on, given rows in top-to-bottom order. Above the first row it is the
/// first, below the last row it is the last, and over something that is not a row (a note card,
/// a row of the other side) it is the row just above.
pub(super) fn head_at(rows: &[Candidate], y: f32) -> Option<u32> {
    let below = rows.partition_point(|row| row.top <= y);
    rows.get(below.saturating_sub(1)).map(|row| row.number)
}

/// "Line 6" or "Lines 6-12".
pub(super) fn anchor_label(anchor: &Anchor) -> String {
    if anchor.span == 0 {
        format!("Line {}", anchor.line_hint)
    } else {
        format!("Lines {}-{}", anchor.line_hint, anchor.last_line_hint())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<Candidate> {
        (0..4)
            .map(|i| Candidate {
                top: 10.0 + i as f32 * 22.0,
                number: 5 + i,
            })
            .collect()
    }

    #[test]
    fn a_range_runs_from_the_first_to_the_last_line_whichever_way_it_was_dragged() {
        let mut range = LineRange::new(Side::New, 0, 8);
        assert!(!range.is_multi());
        range.head = 5;
        assert_eq!((range.first(), range.last()), (5, 8));
        assert!(range.is_multi());
        assert!(range.contains(6) && range.contains(8) && !range.contains(9));
        range.head = 12;
        assert_eq!((range.first(), range.last()), (8, 12));
    }

    #[test]
    fn only_the_same_side_and_hunk_extends_a_range() {
        let range = LineRange::new(Side::New, 1, 8);
        assert!(range.extends_to(Side::New, 1));
        assert!(!range.extends_to(Side::Old, 1));
        assert!(!range.extends_to(Side::New, 2));
    }

    #[test]
    fn the_pointer_picks_the_row_under_it_and_stops_at_the_edges() {
        let rows = rows();
        let cases = [
            (-50.0, Some(5)),
            (10.0, Some(5)),
            (31.9, Some(5)),
            (32.0, Some(6)),
            (60.0, Some(7)),
            (76.0, Some(8)),
            (900.0, Some(8)),
        ];
        for (y, want) in cases {
            assert_eq!(head_at(&rows, y), want, "y = {y}");
        }
        assert_eq!(head_at(&[], 5.0), None);
    }

    #[test]
    fn a_label_names_the_line_or_the_range() {
        let lines = ["a", "b", "c", "d"];
        assert_eq!(anchor_label(&Anchor::at(&lines, 1, Side::New)), "Line 2");
        assert_eq!(
            anchor_label(&Anchor::over(&lines, 1, 2, Side::New)),
            "Lines 2-4"
        );
    }
}
