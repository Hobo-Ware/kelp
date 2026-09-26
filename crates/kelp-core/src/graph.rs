#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    /// Full-height vertical line through the row on `lane`.
    Pass,
    /// Vertical line from the top of the row down to the dot.
    Top,
    /// Vertical line from the dot down to the bottom of the row.
    Bottom,
    /// A branch coming down `lane` that ends here: it turns with a
    /// rounded corner and runs flat into the dot.
    JoinIn,
    /// A merge: runs flat out of the dot, turns with a rounded corner
    /// and continues down `lane`.
    MergeOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub kind: EdgeKind,
    pub lane: u16,
    pub color: u8,
}

#[derive(Debug, Default)]
pub struct Layout {
    node_lane: Vec<u16>,
    node_color: Vec<u8>,
    row_edges: Vec<u32>,
    edges: Vec<Edge>,
    lane_count: u16,
}

impl Layout {
    pub fn rows(&self) -> usize {
        self.node_lane.len()
    }

    pub fn node_lane(&self, row: usize) -> u16 {
        self.node_lane[row]
    }

    pub fn node_color(&self, row: usize) -> u8 {
        self.node_color[row]
    }

    pub fn edges(&self, row: usize) -> &[Edge] {
        let start = self.row_edges[row] as usize;
        let end = self.row_edges[row + 1] as usize;
        &self.edges[start..end]
    }

    pub fn lane_count(&self) -> u16 {
        self.lane_count
    }
}

#[derive(Clone, Copy)]
struct Slot {
    expects: u32,
    color: u8,
}

pub fn layout<'a>(rows: usize, parents: impl Fn(usize) -> &'a [u32], colors: u8) -> Layout {
    let mut out = Layout {
        node_lane: Vec::with_capacity(rows),
        node_color: Vec::with_capacity(rows),
        row_edges: Vec::with_capacity(rows + 1),
        edges: Vec::with_capacity(rows * 3),
        lane_count: 0,
    };
    let mut active: Vec<Option<Slot>> = Vec::new();
    let mut next_color: u8 = 0;
    let mut take_color = || {
        let c = next_color;
        next_color = (next_color + 1) % colors;
        c
    };

    for row in 0..rows {
        out.row_edges.push(out.edges.len() as u32);
        let row_id = row as u32;

        let mut node: Option<(usize, u8)> = None;
        for (lane, slot) in active.iter_mut().enumerate() {
            let Some(s) = *slot else { continue };
            if s.expects != row_id {
                continue;
            }
            match node {
                None => {
                    node = Some((lane, s.color));
                    out.edges.push(Edge {
                        kind: EdgeKind::Top,
                        lane: lane as u16,
                        color: s.color,
                    });
                }
                Some(_) => {
                    out.edges.push(Edge {
                        kind: EdgeKind::JoinIn,
                        lane: lane as u16,
                        color: s.color,
                    });
                    *slot = None;
                }
            }
        }
        let (node_lane, node_color) =
            node.unwrap_or_else(|| (free_lane(&mut active), take_color()));

        for (lane, slot) in active.iter().enumerate() {
            if let Some(s) = slot
                && lane != node_lane
            {
                out.edges.push(Edge {
                    kind: EdgeKind::Pass,
                    lane: lane as u16,
                    color: s.color,
                });
            }
        }

        let ps = parents(row);
        debug_assert!(
            ps.iter().all(|&p| p as usize > row),
            "parents must come after children"
        );
        match ps.split_first() {
            None => active[node_lane] = None,
            Some((&first, rest)) => {
                active[node_lane] = Some(Slot {
                    expects: first,
                    color: node_color,
                });
                out.edges.push(Edge {
                    kind: EdgeKind::Bottom,
                    lane: node_lane as u16,
                    color: node_color,
                });
                for &p in rest {
                    if p == first {
                        continue;
                    }
                    let existing = active
                        .iter()
                        .enumerate()
                        .find(|(lane, s)| *lane != node_lane && s.is_some_and(|s| s.expects == p));
                    let (lane, color) = match existing {
                        Some((lane, s)) => (lane, s.unwrap().color),
                        None => {
                            let lane = free_lane(&mut active);
                            let color = take_color();
                            active[lane] = Some(Slot { expects: p, color });
                            (lane, color)
                        }
                    };
                    out.edges.push(Edge {
                        kind: EdgeKind::MergeOut,
                        lane: lane as u16,
                        color,
                    });
                }
            }
        }

        while active.last().is_some_and(Option::is_none) {
            active.pop();
        }
        out.lane_count = out
            .lane_count
            .max(active.len() as u16)
            .max(node_lane as u16 + 1);
        out.node_lane.push(node_lane as u16);
        out.node_color.push(node_color);
    }
    out.row_edges.push(out.edges.len() as u32);
    out
}

fn free_lane(active: &mut Vec<Option<Slot>>) -> usize {
    match active.iter().position(Option::is_none) {
        Some(lane) => lane,
        None => {
            active.push(None);
            active.len() - 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EdgeKind::*;
    use super::*;

    fn run(parents: &[&[u32]]) -> Layout {
        let owned: Vec<Vec<u32>> = parents.iter().map(|p| p.to_vec()).collect();
        layout(owned.len(), |r| &owned[r], 5)
    }

    fn kinds(l: &Layout, row: usize) -> Vec<(EdgeKind, u16)> {
        let mut k: Vec<_> = l.edges(row).iter().map(|e| (e.kind, e.lane)).collect();
        k.sort_by_key(|(kind, lane)| (*lane, *kind as u8));
        k
    }

    #[test]
    fn linear_history_stays_in_one_lane() {
        let l = run(&[&[1], &[2], &[]]);
        assert_eq!(
            (0..3).map(|r| l.node_lane(r)).collect::<Vec<_>>(),
            [0, 0, 0]
        );
        assert_eq!(kinds(&l, 0), [(Bottom, 0)]);
        assert_eq!(kinds(&l, 1), [(Top, 0), (Bottom, 0)]);
        assert_eq!(kinds(&l, 2), [(Top, 0)]);
        assert_eq!(l.lane_count(), 1);
    }

    #[test]
    fn merge_branches_out_and_comes_back() {
        let l = run(&[&[1, 2], &[3], &[3], &[]]);
        assert_eq!(kinds(&l, 0), [(Bottom, 0), (MergeOut, 1)]);
        assert_eq!(kinds(&l, 1), [(Top, 0), (Bottom, 0), (Pass, 1)]);
        assert_eq!(l.node_lane(2), 1);
        assert_eq!(kinds(&l, 2), [(Pass, 0), (Top, 1), (Bottom, 1)]);
        assert_eq!(kinds(&l, 3), [(Top, 0), (JoinIn, 1)]);
        assert_eq!(
            l.edges(3).iter().find(|e| e.kind == JoinIn).unwrap().color,
            l.node_color(2)
        );
    }

    #[test]
    fn two_tips_fork_from_one_parent() {
        let l = run(&[&[2], &[2], &[]]);
        assert_eq!((l.node_lane(0), l.node_lane(1)), (0, 1));
        assert_eq!(kinds(&l, 1), [(Pass, 0), (Bottom, 1)]);
        assert_eq!(kinds(&l, 2), [(Top, 0), (JoinIn, 1)]);
        assert_ne!(l.node_color(0), l.node_color(1));
    }

    #[test]
    fn freed_lanes_are_reused() {
        let l = run(&[&[2], &[2], &[4], &[4], &[]]);
        assert_eq!(l.node_lane(3), 1);
        assert_eq!(l.lane_count(), 2);
    }

    #[test]
    fn merge_into_an_existing_lane_does_not_open_a_new_one() {
        let l = run(&[&[2], &[3, 2], &[3], &[]]);
        assert_eq!(l.node_lane(1), 1);
        assert_eq!(kinds(&l, 1), [(Pass, 0), (MergeOut, 0), (Bottom, 1)]);
        assert_eq!(l.lane_count(), 2);
    }

    #[test]
    fn octopus_merge_opens_one_lane_per_extra_parent() {
        let l = run(&[&[1, 2, 3], &[4], &[4], &[4], &[]]);
        assert_eq!(kinds(&l, 0), [(Bottom, 0), (MergeOut, 1), (MergeOut, 2)]);
        assert_eq!(kinds(&l, 4), [(Top, 0), (JoinIn, 1), (JoinIn, 2)]);
    }
}
