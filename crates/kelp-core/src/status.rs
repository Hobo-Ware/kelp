use std::path::Path;

use crate::commit::{ChangeKind, FileChange};
use crate::git_cli;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkingStatus {
    pub staged: Vec<FileChange>,
    pub unstaged: Vec<FileChange>,
    pub conflicted: Vec<String>,
    pub untracked: Vec<String>,
    pub folders: Vec<UntrackedFolder>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UntrackedMode {
    #[default]
    Folders,
    Files,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntrackedFolder {
    pub path: String,
    pub files: usize,
}

pub fn is_folder(path: &str) -> bool {
    path.ends_with('/')
}

impl WorkingStatus {
    pub fn is_empty(&self) -> bool {
        self.staged.is_empty() && self.unstaged.is_empty() && self.conflicted.is_empty()
    }

    pub fn file_count(&self, path: &str) -> usize {
        self.folders
            .iter()
            .find(|f| f.path == path)
            .map_or(1, |f| f.files)
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
    working_status_with(workdir, UntrackedMode::Folders)
}

pub fn working_status_with(workdir: &Path, mode: UntrackedMode) -> anyhow::Result<WorkingStatus> {
    let untracked = match mode {
        UntrackedMode::Folders => "--untracked-files=normal",
        UntrackedMode::Files => "--untracked-files=all",
    };
    let out = git_cli::run(
        workdir,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-z",
            untracked,
        ],
    )?;
    let mut status = parse_porcelain(&out);
    count_folder_files(workdir, &mut status.folders)?;
    Ok(status)
}

pub fn folder_files(workdir: &Path, folder: &str) -> anyhow::Result<Vec<String>> {
    let out = list_untracked(workdir, &[folder])?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect())
}

fn list_untracked(workdir: &Path, folders: &[&str]) -> anyhow::Result<String> {
    let mut args = vec![
        "--literal-pathspecs",
        "--no-optional-locks",
        "ls-files",
        "--others",
        "--exclude-standard",
        "-z",
        "--",
    ];
    args.extend(folders);
    git_cli::run(workdir, &args)
}

fn count_folder_files(workdir: &Path, folders: &mut [UntrackedFolder]) -> anyhow::Result<()> {
    if folders.is_empty() {
        return Ok(());
    }
    let paths: Vec<&str> = folders.iter().map(|f| f.path.as_str()).collect();
    let out = list_untracked(workdir, &paths)?;
    tally(&out, folders);
    Ok(())
}

fn tally(listing: &str, folders: &mut [UntrackedFolder]) {
    for folder in folders.iter_mut() {
        folder.files = 0;
    }
    for file in listing.split('\0').filter(|p| !p.is_empty()) {
        if let Some(folder) = folders.iter_mut().find(|f| file.starts_with(&f.path)) {
            folder.files += 1;
        }
    }
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
            if is_folder(&path) {
                status.folders.push(UntrackedFolder {
                    path: path.clone(),
                    files: 0,
                });
            }
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
    status.folders.sort_by(|a, b| a.path.cmp(&b.path));
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
        assert!(s.folders.is_empty());
    }

    #[test]
    fn an_untracked_folder_is_one_unstaged_entry() {
        let out = "?? .claude/autoharness/\0?? a/b/\0 M src/lib.rs\0?? notes.md\0";
        let s = parse_porcelain(out);
        assert_eq!(
            paths(&s.unstaged),
            [
                (".claude/autoharness/", ChangeKind::Added),
                ("a/b/", ChangeKind::Added),
                ("notes.md", ChangeKind::Added),
                ("src/lib.rs", ChangeKind::Modified)
            ]
        );
        assert_eq!(s.untracked, [".claude/autoharness/", "a/b/", "notes.md"]);
        let folders: Vec<&str> = s.folders.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(folders, [".claude/autoharness/", "a/b/"]);
        assert_eq!(s.all().len(), 4);
    }

    #[test]
    fn is_folder_follows_the_trailing_slash() {
        for (path, folder) in [
            ("dir/", true),
            ("a/b/", true),
            ("file.rs", false),
            ("a/file.rs", false),
            ("", false),
        ] {
            assert_eq!(is_folder(path), folder, "{path:?}");
        }
    }

    #[test]
    fn tally_counts_files_under_each_folder() {
        let folder = |path: &str| UntrackedFolder {
            path: path.into(),
            files: 99,
        };
        let cases: [(&str, Vec<(&str, usize)>); 4] = [
            ("", vec![("a/", 0), ("b/", 0)]),
            ("a/1\0a/sub/2\0a/sub/3\0", vec![("a/", 3), ("b/", 0)]),
            ("a/1\0b/1\0b/2\0", vec![("a/", 1), ("b/", 2)]),
            ("ab/1\0stray\0", vec![("a/", 0), ("b/", 0)]),
        ];
        for (listing, expected) in cases {
            let mut folders = vec![folder("a/"), folder("b/")];
            tally(listing, &mut folders);
            let got: Vec<(&str, usize)> =
                folders.iter().map(|f| (f.path.as_str(), f.files)).collect();
            assert_eq!(got, expected, "{listing:?}");
        }
    }
}
