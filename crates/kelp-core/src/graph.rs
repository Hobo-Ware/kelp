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
    row_outgoing: Vec<u32>,
    edges: Vec<Edge>,
    spans: Vec<Span>,
    bucket_start: Vec<u32>,
    bucket_spans: Vec<u32>,
    lane_count: u16,
}

#[derive(Debug, Clone, Copy)]
struct Span {
    lane: u16,
    color: u8,
    from: u32,
    to: u32,
}

const BUCKET_ROWS: usize = 128;

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

    pub fn edges(&self, row: usize) -> Vec<Edge> {
        let start = self.row_edges[row] as usize;
        let split = self.row_outgoing[row] as usize;
        let end = self.row_edges[row + 1] as usize;
        let mut out = Vec::with_capacity(end - start + 8);
        out.extend_from_slice(&self.edges[start..split]);
        let row_id = row as u32;
        let bucket = row / BUCKET_ROWS;
        let ids = &self.bucket_spans
            [self.bucket_start[bucket] as usize..self.bucket_start[bucket + 1] as usize];
        let first_pass = out.len();
        out.extend(
            ids.iter()
                .map(|&id| self.spans[id as usize])
                .filter(|span| span.from < row_id && row_id < span.to)
                .map(|span| Edge {
                    kind: EdgeKind::Pass,
                    lane: span.lane,
                    color: span.color,
                }),
        );
        out[first_pass..].sort_unstable_by_key(|e| e.lane);
        out.extend_from_slice(&self.edges[split..end]);
        out
    }

    pub fn lane_count(&self) -> u16 {
        self.lane_count
    }

    fn index_spans(&mut self) {
        let buckets = self.rows().div_ceil(BUCKET_ROWS) + 1;
        let mut counts = vec![0u32; buckets + 1];
        let range = |span: &Span| {
            let first = (span.from as usize + 1) / BUCKET_ROWS;
            let last = (span.to as usize - 1) / BUCKET_ROWS;
            first..=last
        };
        for span in &self.spans {
            for b in range(span) {
                counts[b + 1] += 1;
            }
        }
        for b in 0..buckets {
            counts[b + 1] += counts[b];
        }
        let mut fill = counts.clone();
        let mut ids = vec![0u32; counts[buckets] as usize];
        for (id, span) in self.spans.iter().enumerate() {
            for b in range(span) {
                ids[fill[b] as usize] = id as u32;
                fill[b] += 1;
            }
        }
        self.bucket_start = counts;
        self.bucket_spans = ids;
    }
}

#[derive(Clone, Copy)]
struct Slot {
    expects: u32,
    color: u8,
    since: u32,
}

pub fn layout<'a>(rows: usize, parents: impl Fn(usize) -> &'a [u32], colors: u8) -> Layout {
    let mut out = Layout {
        node_lane: Vec::with_capacity(rows),
        node_color: Vec::with_capacity(rows),
        row_edges: Vec::with_capacity(rows + 1),
        row_outgoing: Vec::with_capacity(rows),
        edges: Vec::with_capacity(rows * 2),
        ..Layout::default()
    };
    let mut active: Vec<Option<Slot>> = Vec::new();
    let mut next_color: u8 = 0;
    let mut take_color = || {
        let c = next_color;
        next_color = (next_color + 1) % colors;
        c
    };
    let close = |spans: &mut Vec<Span>, lane: usize, slot: Slot, to: u32| {
        if to > slot.since + 1 {
            spans.push(Span {
                lane: lane as u16,
                color: slot.color,
                from: slot.since,
                to,
            });
        }
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
            close(&mut out.spans, lane, s, row_id);
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
        out.row_outgoing.push(out.edges.len() as u32);

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
                    since: row_id,
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
                            active[lane] = Some(Slot {
                                expects: p,
                                color,
                                since: row_id,
                            });
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
    for (lane, slot) in active.iter().enumerate() {
        if let Some(slot) = slot {
            close(&mut out.spans, lane, *slot, rows as u32);
        }
    }
    out.index_spans();
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

    fn reference(rows: usize, parents: &[Vec<u32>], colors: u8) -> Vec<(u16, u8, Vec<Edge>)> {
        #[derive(Clone, Copy)]
        struct Old {
            expects: u32,
            color: u8,
        }
        let mut out = Vec::new();
        let mut active: Vec<Option<Old>> = Vec::new();
        let mut next = 0u8;
        let mut take = || {
            let c = next;
            next = (next + 1) % colors;
            c
        };
        let free = |active: &mut Vec<Option<Old>>| match active.iter().position(Option::is_none) {
            Some(l) => l,
            None => {
                active.push(None);
                active.len() - 1
            }
        };
        for (row, row_parents) in parents.iter().enumerate().take(rows) {
            let mut edges = Vec::new();
            let mut node = None;
            for (lane, slot) in active.iter_mut().enumerate() {
                let Some(s) = *slot else { continue };
                if s.expects != row as u32 {
                    continue;
                }
                let kind = if node.is_none() {
                    node = Some((lane, s.color));
                    Top
                } else {
                    *slot = None;
                    JoinIn
                };
                edges.push(Edge {
                    kind,
                    lane: lane as u16,
                    color: s.color,
                });
            }
            let (node_lane, node_color) = node.unwrap_or_else(|| (free(&mut active), take()));
            for (lane, slot) in active.iter().enumerate() {
                if let Some(s) = slot
                    && lane != node_lane
                {
                    edges.push(Edge {
                        kind: Pass,
                        lane: lane as u16,
                        color: s.color,
                    });
                }
            }
            match row_parents.split_first() {
                None => active[node_lane] = None,
                Some((&first, rest)) => {
                    active[node_lane] = Some(Old {
                        expects: first,
                        color: node_color,
                    });
                    edges.push(Edge {
                        kind: Bottom,
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
                            .find(|(l, s)| *l != node_lane && s.is_some_and(|s| s.expects == p));
                        let (lane, color) = match existing {
                            Some((l, s)) => (l, s.unwrap().color),
                            None => {
                                let l = free(&mut active);
                                let c = take();
                                active[l] = Some(Old {
                                    expects: p,
                                    color: c,
                                });
                                (l, c)
                            }
                        };
                        edges.push(Edge {
                            kind: MergeOut,
                            lane: lane as u16,
                            color,
                        });
                    }
                }
            }
            while active.last().is_some_and(Option::is_none) {
                active.pop();
            }
            out.push((node_lane as u16, node_color, edges));
        }
        out
    }

    fn random_history(rows: usize, seed: u64) -> Vec<Vec<u32>> {
        let mut state = seed;
        let mut next = move |n: u64| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) % n.max(1)
        };
        (0..rows)
            .map(|row| {
                let left = (rows - row - 1) as u64;
                if left == 0 || next(40) == 0 {
                    return Vec::new();
                }
                let count = if next(6) == 0 { 2 + next(2) } else { 1 };
                let mut ps: Vec<u32> = (0..count)
                    .map(|_| (row as u64 + 1 + next(left.min(300))) as u32)
                    .collect();
                ps.dedup();
                ps
            })
            .collect()
    }

    #[test]
    fn spans_give_exactly_the_old_per_row_edges() {
        for (rows, seed) in [(50, 1), (400, 7), (2_000, 42), (5_000, 9), (300, 1234)] {
            let parents = random_history(rows, seed);
            let expected = reference(rows, &parents, 8);
            let got = layout(rows, |r| &parents[r], 8);
            for (row, (lane, color, edges)) in expected.iter().enumerate() {
                assert_eq!(
                    got.node_lane(row),
                    *lane,
                    "rows {rows} seed {seed} row {row}"
                );
                assert_eq!(
                    got.node_color(row),
                    *color,
                    "rows {rows} seed {seed} row {row}"
                );
                assert_eq!(&got.edges(row), edges, "rows {rows} seed {seed} row {row}");
            }
        }
    }
}
