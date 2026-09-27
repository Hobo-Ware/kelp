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

pub struct RowMap {
    wips: Vec<(usize, WipKind)>,
    commits: usize,
}

impl RowMap {
    pub fn new(commits: usize, current: Option<usize>, others: &[usize]) -> Self {
        let mut wips: Vec<(usize, WipKind)> = current
            .map(|head| (head, WipKind::Current))
            .into_iter()
            .chain(
                others
                    .iter()
                    .enumerate()
                    .map(|(i, &head)| (head, WipKind::Other(i))),
            )
            .filter(|(head, _)| *head < commits.max(1))
            .collect();
        wips.sort_by_key(|&(head, kind)| (head, kind != WipKind::Current));
        Self { wips, commits }
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
        self.wips.iter().any(|(head, _)| *head == commit)
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
    fn wip_rows_sit_above_their_heads() {
        let map = RowMap::new(5, Some(2), &[0, 4]);
        assert_eq!(
            all(&map),
            [
                Row::OtherWip(0),
                Row::Commit(0),
                Row::Commit(1),
                Row::CurrentWip,
                Row::Commit(2),
                Row::Commit(3),
                Row::OtherWip(1),
                Row::Commit(4),
            ]
        );
    }

    #[test]
    fn the_current_worktree_comes_first_on_a_shared_head() {
        let map = RowMap::new(2, Some(1), &[1]);
        assert_eq!(
            all(&map),
            [
                Row::Commit(0),
                Row::CurrentWip,
                Row::OtherWip(0),
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
    fn heads_outside_the_history_are_ignored() {
        let map = RowMap::new(2, None, &[7]);
        assert_eq!(map.total(), 2);
    }
}
