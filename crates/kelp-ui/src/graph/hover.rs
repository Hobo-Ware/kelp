use std::ops::Range;

use kelp_core::graph::{Edge, EdgeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Segment {
    child: usize,
    parent: usize,
    lane: u16,
}

const EXTENT_ROWS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    None,
    Remote,
    Local,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct HoverPath {
    rows: Vec<usize>,
    segments: Vec<Segment>,
}

impl HoverPath {
    pub fn toward_tip(
        start: usize,
        first_parent: impl Fn(usize) -> Option<u32>,
        label: impl Fn(usize) -> Label,
        node_lane: impl Fn(usize) -> u16,
    ) -> Self {
        let mut rows = vec![start];
        let mut segments = Vec::new();
        let mut current = start;
        loop {
            let kind = label(current);
            if kind == Label::Local {
                break;
            }
            let lane = node_lane(current);
            let children = || {
                (0..current)
                    .rev()
                    .filter(|&c| first_parent(c) == Some(current as u32))
            };
            let same_lane = children().find(|&c| node_lane(c) == lane);
            if kind == Label::Remote && same_lane.is_none() {
                break;
            }
            let Some(child) = same_lane.or_else(|| children().next()) else {
                break;
            };
            segments.push(Segment {
                child,
                parent: current,
                lane: node_lane(child),
            });
            rows.push(child);
            current = child;
        }
        if segments.is_empty() {
            current = start;
            for _ in 0..EXTENT_ROWS {
                let Some(parent) = first_parent(current).map(|p| p as usize) else {
                    break;
                };
                segments.push(Segment {
                    child: current,
                    parent,
                    lane: node_lane(current),
                });
                rows.push(parent);
                if node_lane(parent) != node_lane(current) || label(parent) != Label::None {
                    break;
                }
                current = parent;
            }
        }
        rows.sort_unstable();
        Self { rows, segments }
    }

    pub fn contains(&self, row: usize) -> bool {
        self.rows.binary_search(&row).is_ok()
    }

    pub fn visible(&self, rows: Range<usize>) -> VisiblePath<'_> {
        VisiblePath {
            path: self,
            segments: self
                .segments
                .iter()
                .copied()
                .filter(|s| s.child < rows.end && s.parent >= rows.start)
                .collect(),
        }
    }
}

pub struct VisiblePath<'a> {
    path: &'a HoverPath,
    segments: Vec<Segment>,
}

impl VisiblePath<'_> {
    pub fn contains(&self, row: usize) -> bool {
        self.path.contains(row)
    }

    pub fn carries(&self, row: usize, edge: &Edge) -> bool {
        self.segments.iter().any(|s| {
            edge.lane == s.lane
                && match edge.kind {
                    EdgeKind::Bottom => row == s.child,
                    EdgeKind::Pass => s.child < row && row < s.parent,
                    EdgeKind::Top | EdgeKind::JoinIn => row == s.parent,
                    EdgeKind::MergeOut => false,
                }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace(parents: &[&[u32]], refs: &[usize], lanes: &[u16], start: usize) -> HoverPath {
        trace_labels(parents, refs, &[], lanes, start)
    }

    fn trace_labels(
        parents: &[&[u32]],
        local: &[usize],
        remote: &[usize],
        lanes: &[u16],
        start: usize,
    ) -> HoverPath {
        HoverPath::toward_tip(
            start,
            |r| parents[r].first().copied(),
            |r| match (local.contains(&r), remote.contains(&r)) {
                (true, _) => Label::Local,
                (_, true) => Label::Remote,
                _ => Label::None,
            },
            |r| lanes[r],
        )
    }

    #[test]
    fn walks_up_the_first_parent_chain_to_the_tip() {
        let path = trace(&[&[1], &[2], &[]], &[], &[0, 0, 0], 2);
        assert_eq!(path.rows, [0, 1, 2]);
        assert_eq!(path.segments.len(), 2);
    }

    #[test]
    fn stops_at_the_nearest_ref() {
        let path = trace(&[&[1], &[2], &[]], &[1], &[0, 0, 0], 2);
        assert_eq!(path.rows, [1, 2]);
    }

    #[test]
    fn a_fork_continues_on_the_commits_own_lane_not_into_the_nearest_branch() {
        let parents: &[&[u32]] = &[&[2], &[2], &[3], &[]];
        let lanes = [0, 1, 0, 0];
        let path = trace(parents, &[0, 1], &lanes, 3);
        assert_eq!(path.rows, [0, 2, 3]);
        assert!(path.segments.iter().all(|s| s.lane == 0));
    }

    #[test]
    fn a_side_branch_commit_walks_up_its_own_branch() {
        let parents: &[&[u32]] = &[&[3], &[3], &[3], &[]];
        let lanes = [0, 2, 1, 0];
        let path = trace(parents, &[0, 1, 2], &lanes, 3);
        assert_eq!(path.rows, [0, 3]);
        let side = trace(parents, &[0, 1, 2], &lanes, 2);
        assert_eq!(
            side.rows,
            [2, 3],
            "a branch tip shows its line down to the fork"
        );
    }

    #[test]
    fn a_merge_on_the_same_lane_is_the_continuation() {
        let parents: &[&[u32]] = &[&[2, 1], &[3], &[3], &[]];
        let lanes = [0, 1, 0, 0];
        let path = trace(parents, &[0], &lanes, 3);
        assert_eq!(path.rows, [0, 2, 3]);
    }

    #[test]
    fn falls_back_to_the_nearest_child_when_none_shares_the_lane() {
        let parents: &[&[u32]] = &[&[2], &[2], &[]];
        let lanes = [1, 2, 0];
        let path = trace(parents, &[0, 1], &lanes, 2);
        assert_eq!(path.rows, [1, 2]);
    }

    #[test]
    fn a_merge_does_not_follow_its_second_parent() {
        let parents: &[&[u32]] = &[&[1, 2], &[3], &[3], &[]];
        let path = trace(parents, &[], &[0, 0, 1, 0], 2);
        assert_eq!(path.rows, [2, 3]);
        let path = trace(parents, &[], &[0, 0, 1, 0], 1);
        assert_eq!(path.rows, [0, 1]);
    }

    #[test]
    fn a_remote_marker_does_not_end_a_line_that_goes_on_above_it() {
        let parents: &[&[u32]] = &[&[1], &[2], &[]];
        let path = trace_labels(parents, &[], &[1], &[0, 0, 0], 2);
        assert_eq!(path.rows, [0, 1, 2]);
    }

    #[test]
    fn a_remote_marker_ends_the_line_when_nothing_continues_above_it() {
        let parents: &[&[u32]] = &[&[1], &[2], &[]];
        let path = trace_labels(parents, &[], &[1], &[1, 0, 0], 2);
        assert_eq!(path.rows, [1, 2]);
    }

    #[test]
    fn a_local_branch_still_ends_the_line_even_with_a_remote_marker_above() {
        let parents: &[&[u32]] = &[&[1], &[2], &[]];
        let path = trace_labels(parents, &[1], &[0], &[0, 0, 0], 2);
        assert_eq!(path.rows, [1, 2]);
    }

    #[test]
    fn hovering_a_branch_tip_shows_its_line_down_to_the_next_label() {
        let parents: &[&[u32]] = &[&[1], &[2], &[3], &[4], &[]];
        let lanes = [0; 5];
        let down = trace_labels(parents, &[0], &[], &lanes, 0);
        assert_eq!(down.rows, [0, 1, 2, 3, 4]);
        assert_eq!(down.segments.len(), 4);
        let stopped = trace_labels(parents, &[0], &[2], &lanes, 0);
        assert_eq!(stopped.rows, [0, 1, 2]);
    }

    #[test]
    fn a_feature_branch_tip_stops_at_the_commit_it_forks_from() {
        let parents: &[&[u32]] = &[&[1], &[3], &[3], &[]];
        let lanes = [1, 1, 0, 0];
        let path = trace_labels(parents, &[0], &[], &lanes, 0);
        assert_eq!(path.rows, [0, 1, 3]);
        let visible = path.visible(0..4);
        let edge = |kind, lane| Edge {
            kind,
            lane,
            color: 0,
            from: 0,
        };
        assert!(visible.carries(0, &edge(EdgeKind::Bottom, 1)));
        assert!(visible.carries(2, &edge(EdgeKind::Pass, 1)));
        assert!(visible.carries(3, &edge(EdgeKind::JoinIn, 1)));
    }

    #[test]
    fn the_downward_line_is_capped() {
        let n = EXTENT_ROWS + 100;
        let parents: Vec<Vec<u32>> = (0..n)
            .map(|r| {
                if r + 1 < n {
                    vec![r as u32 + 1]
                } else {
                    vec![]
                }
            })
            .collect();
        let refs: Vec<&[u32]> = parents.iter().map(Vec::as_slice).collect();
        let lanes = vec![0; n];
        let path = trace_labels(&refs, &[0], &[], &lanes, 0);
        assert_eq!(path.rows.len(), EXTENT_ROWS + 1);
    }

    #[test]
    fn edges_of_the_chain_are_carried_on_the_child_lane() {
        let parents: &[&[u32]] = &[&[3], &[2], &[], &[]];
        let path = trace(parents, &[], &[1, 0, 0, 0], 3);
        assert_eq!(path.rows, [0, 3]);
        let visible = path.visible(0..4);
        let edge = |kind, lane| Edge {
            kind,
            lane,
            color: 0,
            from: 0,
        };
        assert!(visible.carries(0, &edge(EdgeKind::Bottom, 1)));
        assert!(visible.carries(1, &edge(EdgeKind::Pass, 1)));
        assert!(visible.carries(3, &edge(EdgeKind::JoinIn, 1)));
        assert!(!visible.carries(1, &edge(EdgeKind::Pass, 0)));
        assert!(!visible.carries(3, &edge(EdgeKind::Top, 0)));
    }
}
