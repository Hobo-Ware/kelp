use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::Path;
use std::time::{Duration, Instant};

use gix::ObjectId;
use gix::hashtable::HashMap;
use gix::revision::walk::Sorting;
use gix::traverse::commit::simple::CommitTimeOrder;

use crate::graph::{self, Layout};
use crate::refs::Refs;
use crate::view::ViewFilter;

pub const LANE_COLORS: u8 = 8;

pub struct History {
    ids: Vec<ObjectId>,
    parent_start: Vec<u32>,
    parents: Vec<u32>,
    rows_by_id: HashMap<ObjectId, u32>,
    pub layout: Layout,
    pub refs: Refs,
    pub timings: Timings,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Timings {
    pub walk: Duration,
    pub sort: Duration,
    pub layout: Duration,
}

impl History {
    pub fn load(repo: &gix::Repository) -> anyhow::Result<Self> {
        Self::load_filtered(repo, &ViewFilter::load(repo.common_dir()))
    }

    pub fn load_filtered(repo: &gix::Repository, view: &ViewFilter) -> anyhow::Result<Self> {
        let mut refs = Refs::load(repo)?;
        refs.apply(view);
        let started = Instant::now();
        let Walked {
            ids,
            times,
            parents: raw_parents,
            index: mut rows_by_id,
        } = walk(repo, refs.tips())?;
        let walked = Instant::now();
        let order = date_order(&times, &raw_parents);
        drop(times);
        let mut row_of = vec![0u32; ids.len()];
        for (row, &i) in order.iter().enumerate() {
            row_of[i as usize] = row as u32;
        }
        let mut sorted_ids = Vec::with_capacity(ids.len());
        let mut parent_start = Vec::with_capacity(ids.len() + 1);
        let mut parents = Vec::with_capacity(ids.len() + ids.len() / 8);
        for &i in &order {
            sorted_ids.push(ids[i as usize]);
            parent_start.push(parents.len() as u32);
            parents.extend(
                raw_parents
                    .of(i as usize)
                    .iter()
                    .map(|&p| row_of[p as usize]),
            );
        }
        for row in rows_by_id.values_mut() {
            *row = row_of[*row as usize];
        }
        drop(raw_parents);
        parent_start.push(parents.len() as u32);
        let sorted = Instant::now();
        let layout = graph::layout(
            sorted_ids.len(),
            |row| &parents[parent_start[row] as usize..parent_start[row + 1] as usize],
            LANE_COLORS,
        );
        let laid_out = Instant::now();
        refs.attach_rows(&rows_by_id);
        Ok(Self {
            ids: sorted_ids,
            parent_start,
            parents,
            rows_by_id,
            layout,
            refs,
            timings: Timings {
                walk: walked - started,
                sort: sorted - walked,
                layout: laid_out - sorted,
            },
        })
    }

    pub fn open(path: &Path) -> anyhow::Result<(gix::Repository, Self)> {
        let repo = gix::discover(path)?;
        let history = Self::load(&repo)?;
        Ok((repo, history))
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn ids(&self) -> &[ObjectId] {
        &self.ids
    }

    pub fn id(&self, row: usize) -> ObjectId {
        self.ids[row]
    }

    pub fn row(&self, id: &ObjectId) -> Option<usize> {
        self.rows_by_id.get(id).map(|&r| r as usize)
    }

    pub fn parents(&self, row: usize) -> &[u32] {
        &self.parents[self.parent_start[row] as usize..self.parent_start[row + 1] as usize]
    }
}

pub struct Parents {
    start: Vec<u32>,
    list: Vec<u32>,
}

impl Parents {
    pub fn from_lists(lists: &[Vec<u32>]) -> Self {
        let mut parents = Self {
            start: Vec::with_capacity(lists.len() + 1),
            list: Vec::new(),
        };
        for ps in lists {
            parents.start.push(parents.list.len() as u32);
            parents.list.extend_from_slice(ps);
        }
        parents.start.push(parents.list.len() as u32);
        parents
    }

    fn len(&self) -> usize {
        self.start.len().saturating_sub(1)
    }

    fn of(&self, i: usize) -> &[u32] {
        &self.list[self.start[i] as usize..self.start[i + 1] as usize]
    }
}

struct Walked {
    ids: Vec<ObjectId>,
    times: Vec<i64>,
    parents: Parents,
    index: HashMap<ObjectId, u32>,
}

fn walk(repo: &gix::Repository, tips: Vec<ObjectId>) -> anyhow::Result<Walked> {
    let mut ids = Vec::new();
    let mut times = Vec::new();
    let mut oid_start = vec![0u32];
    let mut parent_oids: Vec<ObjectId> = Vec::new();
    if !tips.is_empty() {
        let walk = repo
            .rev_walk(tips)
            .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
            .use_commit_graph(true)
            .all()?;
        for info in walk {
            let info = info?;
            ids.push(info.id);
            times.push(info.commit_time.unwrap_or_default());
            parent_oids.extend(info.parent_ids.iter().copied());
            oid_start.push(parent_oids.len() as u32);
        }
    }
    let mut index = HashMap::default();
    index.reserve(ids.len());
    for (i, id) in ids.iter().enumerate() {
        index.insert(*id, i as u32);
    }
    let mut parents = Parents {
        start: Vec::with_capacity(ids.len() + 1),
        list: Vec::with_capacity(parent_oids.len()),
    };
    for i in 0..ids.len() {
        parents.start.push(parents.list.len() as u32);
        let own = &parent_oids[oid_start[i] as usize..oid_start[i + 1] as usize];
        parents
            .list
            .extend(own.iter().filter_map(|p| index.get(p).copied()));
    }
    parents.start.push(parents.list.len() as u32);
    Ok(Walked {
        ids,
        times,
        parents,
        index,
    })
}

pub fn date_order(times: &[i64], parents: &Parents) -> Vec<u32> {
    let n = times.len();
    debug_assert_eq!(n, parents.len());
    let mut children = vec![0u32; n];
    for &p in &parents.list {
        children[p as usize] += 1;
    }
    let mut ready: BinaryHeap<(i64, Reverse<u32>)> = (0..n as u32)
        .filter(|&i| children[i as usize] == 0)
        .map(|i| (times[i as usize], Reverse(i)))
        .collect();
    let mut order = Vec::with_capacity(n);
    while let Some((_, Reverse(i))) = ready.pop() {
        order.push(i);
        for &p in parents.of(i as usize) {
            children[p as usize] -= 1;
            if children[p as usize] == 0 {
                ready.push((times[p as usize], Reverse(p)));
            }
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parents_never_come_before_children_even_with_clock_skew() {
        let times = [100, 500, 50];
        let parents = vec![vec![1], vec![], vec![0]];
        assert_eq!(
            date_order(&times, &Parents::from_lists(&parents)),
            [2, 0, 1]
        );
    }

    #[test]
    fn newest_ready_commit_goes_first() {
        let times = [10, 30, 20, 0];
        let parents = vec![vec![3], vec![3], vec![3], vec![]];
        assert_eq!(
            date_order(&times, &Parents::from_lists(&parents)),
            [1, 2, 0, 3]
        );
    }
}
