use std::path::Path;

use crate::commit::{ChangeKind, FileChange};
use crate::git_cli;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkingStatus {
    pub staged: Vec<FileChange>,
    pub unstaged: Vec<FileChange>,
    pub conflicted: Vec<String>,
    pub untracked: Vec<String>,
}

impl WorkingStatus {
    pub fn is_empty(&self) -> bool {
        self.staged.is_empty() && self.unstaged.is_empty() && self.conflicted.is_empty()
    }

    pub fn all(&self) -> Vec<FileChange> {
        let mut all: Vec<FileChange> = self.unstaged.clone();
        for change in &self.staged {
            if !all.iter().any(|c| c.path == change.path) {
                all.push(change.clone());
            }
        }
        for path in &self.conflicted {
            if !all.iter().any(|c| &c.path == path) {
                all.push(FileChange {
                    path: path.clone(),
                    kind: ChangeKind::Modified,
                });
            }
        }
        all.sort_by(|a, b| a.path.cmp(&b.path));
        all
    }
}

pub fn working_status(workdir: &Path) -> anyhow::Result<WorkingStatus> {
    let out = git_cli::run(
        workdir,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    Ok(parse_porcelain(&out))
}

fn kind_of(code: char) -> Option<ChangeKind> {
    match code {
        'M' | 'T' => Some(ChangeKind::Modified),
        'A' | 'C' => Some(ChangeKind::Added),
        'D' => Some(ChangeKind::Deleted),
        'R' => Some(ChangeKind::Renamed),
        _ => None,
    }
}

pub fn parse_porcelain(out: &str) -> WorkingStatus {
    let mut status = WorkingStatus::default();
    let mut entries = out.split('\0').filter(|e| !e.is_empty());
    while let Some(entry) = entries.next() {
        let Some((code, path)) = entry.split_at_checked(3) else {
            continue;
        };
        let mut code = code.chars();
        let (x, y) = (code.next().unwrap_or(' '), code.next().unwrap_or(' '));
        if x == 'R' || x == 'C' || y == 'R' {
            entries.next();
        }
        let path = path.to_string();
        let conflict = x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D');
        if conflict {
            status.conflicted.push(path);
            continue;
        }
        if x == '?' {
            status.untracked.push(path.clone());
            status.unstaged.push(FileChange {
                path,
                kind: ChangeKind::Added,
            });
            continue;
        }
        if let Some(kind) = kind_of(x) {
            status.staged.push(FileChange {
                path: path.clone(),
                kind,
            });
        }
        if let Some(kind) = kind_of(y) {
            status.unstaged.push(FileChange { path, kind });
        }
    }
    for list in [&mut status.staged, &mut status.unstaged] {
        list.sort_by(|a, b| a.path.cmp(&b.path));
    }
    status.conflicted.sort();
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[FileChange]) -> Vec<(&str, ChangeKind)> {
        list.iter().map(|c| (c.path.as_str(), c.kind)).collect()
    }

    #[test]
    fn splits_staged_and_unstaged() {
        let out = "MM both.rs\0M  staged.rs\0 M unstaged.rs\0A  new.rs\0 D gone.rs\0R  moved.rs\0old.rs\0?? notes.md\0UU clash.rs\0";
        let s = parse_porcelain(out);
        assert_eq!(
            paths(&s.staged),
            [
                ("both.rs", ChangeKind::Modified),
                ("moved.rs", ChangeKind::Renamed),
                ("new.rs", ChangeKind::Added),
                ("staged.rs", ChangeKind::Modified)
            ]
        );
        assert_eq!(
            paths(&s.unstaged),
            [
                ("both.rs", ChangeKind::Modified),
                ("gone.rs", ChangeKind::Deleted),
                ("notes.md", ChangeKind::Added),
                ("unstaged.rs", ChangeKind::Modified)
            ]
        );
        assert_eq!(s.conflicted, ["clash.rs"]);
        assert_eq!(s.untracked, ["notes.md"]);
        assert_eq!(s.all().len(), 8);
    }
}
