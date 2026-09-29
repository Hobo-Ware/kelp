use crate::repo_view::Selection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Commit(usize),
    CurrentWip,
    OtherWip(usize),
}

impl Row {
    pub fn selection(self) -> Option<Selection> {
        match self {
            Row::Commit(row) => Some(Selection::Commit(row)),
            Row::CurrentWip => Some(Selection::Wip),
            Row::OtherWip(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WipKind {
    Current,
    Other(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WipLink {
    None,
    Pass,
    Join,
}

pub struct RowMap {
    wips: Vec<(usize, WipKind)>,
    heads: Vec<usize>,
    pinned_head: Option<usize>,
    commits: usize,
}

impl RowMap {
    pub fn new(commits: usize, current: Option<usize>, others: &[usize]) -> Self {
        let in_history = |head: &usize| *head < commits.max(1);
        let pinned_head = current.filter(in_history).filter(|&head| head > 0);
        let mut wips: Vec<(usize, WipKind)> = current
            .map(|head| {
                (
                    if pinned_head.is_some() { 0 } else { head },
                    WipKind::Current,
                )
            })
            .into_iter()
            .chain(
                others
                    .iter()
                    .enumerate()
                    .map(|(i, &head)| (head, WipKind::Other(i))),
            )
            .filter(|(at, _)| in_history(at))
            .collect();
        wips.sort_by_key(|&(at, kind)| (at, kind != WipKind::Current));
        let heads = wips
            .iter()
            .filter(|(_, kind)| pinned_head.is_none() || *kind != WipKind::Current)
            .map(|&(at, _)| at)
            .collect();
        Self {
            wips,
            heads,
            pinned_head,
            commits,
        }
    }

    pub fn pinned_head(&self) -> Option<usize> {
        self.pinned_head
    }

    pub fn wip_link(&self, display: usize) -> WipLink {
        let Some(head) = self.pinned_head else {
            return WipLink::None;
        };
        let head_display = self.display(Row::Commit(head));
        match display {
            0 => WipLink::None,
            d if d < head_display => WipLink::Pass,
            d if d == head_display => WipLink::Join,
            _ => WipLink::None,
        }
    }

    pub fn total(&self) -> usize {
        self.commits + self.wips.len()
    }

    pub fn resolve(&self, display: usize) -> Row {
        let mut shift = 0;
        for &(head, kind) in &self.wips {
            let at = head + shift;
            if display == at {
                return match kind {
                    WipKind::Current => Row::CurrentWip,
                    WipKind::Other(i) => Row::OtherWip(i),
                };
            }
            if display < at {
                break;
            }
            shift += 1;
        }
        Row::Commit(display - shift)
    }

    pub fn display(&self, row: Row) -> usize {
        match row {
            Row::Commit(commit) => {
                commit + self.wips.iter().filter(|(head, _)| *head <= commit).count()
            }
            Row::CurrentWip => self.wip_display(WipKind::Current).unwrap_or(0),
            Row::OtherWip(i) => self.wip_display(WipKind::Other(i)).unwrap_or(0),
        }
    }

    pub fn display_of(&self, selection: Selection) -> usize {
        match selection {
            Selection::Commit(row) => self.display(Row::Commit(row)),
            Selection::Wip => self.display(Row::CurrentWip),
        }
    }

    pub fn has_wip_above(&self, commit: usize) -> bool {
        self.heads.contains(&commit)
    }

    fn wip_display(&self, wanted: WipKind) -> Option<usize> {
        self.wips
            .iter()
            .position(|&(_, kind)| kind == wanted)
            .map(|i| self.wips[i].0 + i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(map: &RowMap) -> Vec<Row> {
        (0..map.total()).map(|d| map.resolve(d)).collect()
    }

    #[test]
    fn no_wip_rows_is_the_plain_list() {
        let map = RowMap::new(3, None, &[]);
        assert_eq!(all(&map), [Row::Commit(0), Row::Commit(1), Row::Commit(2)]);
    }

    #[test]
    fn other_worktrees_sit_above_their_heads() {
        let map = RowMap::new(5, Some(2), &[0, 4]);
        assert_eq!(
            all(&map),
            [
                Row::CurrentWip,
                Row::OtherWip(0),
                Row::Commit(0),
                Row::Commit(1),
                Row::Commit(2),
                Row::Commit(3),
                Row::OtherWip(1),
                Row::Commit(4),
            ]
        );
    }

    #[test]
    fn the_current_worktree_comes_first_on_a_shared_head() {
        let map = RowMap::new(2, Some(0), &[0]);
        assert_eq!(
            all(&map),
            [
                Row::CurrentWip,
                Row::OtherWip(0),
                Row::Commit(0),
                Row::Commit(1)
            ]
        );
    }

    #[test]
    fn display_is_the_inverse_of_resolve() {
        let map = RowMap::new(6, Some(3), &[0, 3, 5]);
        for d in 0..map.total() {
            assert_eq!(map.display(map.resolve(d)), d, "display {d}");
        }
        assert_eq!(map.display_of(Selection::Wip), map.display(Row::CurrentWip));
        assert!(map.has_wip_above(3));
        assert!(!map.has_wip_above(1));
    }

    #[test]
    fn the_current_worktree_is_pinned_to_the_top_and_linked_to_its_head() {
        let map = RowMap::new(4, Some(2), &[1]);
        assert_eq!(
            all(&map),
            [
                Row::CurrentWip,
                Row::Commit(0),
                Row::OtherWip(0),
                Row::Commit(1),
                Row::Commit(2),
                Row::Commit(3),
            ]
        );
        assert_eq!(map.pinned_head(), Some(2));
        let links: Vec<WipLink> = (0..map.total()).map(|d| map.wip_link(d)).collect();
        use WipLink::*;
        assert_eq!(links, [None, Pass, Pass, Pass, Join, None]);
        assert!(!map.has_wip_above(0));
        assert!(map.has_wip_above(1));
        assert!(!map.has_wip_above(2));
    }

    #[test]
    fn a_head_at_the_top_needs_no_link() {
        let map = RowMap::new(3, Some(0), &[]);
        assert_eq!(all(&map)[0], Row::CurrentWip);
        assert_eq!(map.pinned_head(), None);
        assert!(map.has_wip_above(0));
        assert_eq!(map.wip_link(1), WipLink::None);
    }

    #[test]
    fn heads_outside_the_history_are_ignored() {
        let map = RowMap::new(2, None, &[7]);
        assert_eq!(map.total(), 2);
    }
}
