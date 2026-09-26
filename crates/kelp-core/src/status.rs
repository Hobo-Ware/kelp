use std::path::Path;

use crate::commit::{ChangeKind, FileChange};
use crate::git_cli;

pub fn working_changes(workdir: &Path) -> anyhow::Result<Vec<FileChange>> {
    let out = git_cli::run(
        workdir,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    Ok(parse_porcelain(&out))
}

pub fn parse_porcelain(out: &str) -> Vec<FileChange> {
    let mut changes = Vec::new();
    let mut entries = out.split('\0').filter(|e| !e.is_empty());
    while let Some(entry) = entries.next() {
        let Some((code, path)) = entry.split_at_checked(3) else {
            continue;
        };
        let mut code = code.chars();
        let (x, y) = (code.next().unwrap_or(' '), code.next().unwrap_or(' '));
        let kind = match (x, y) {
            ('R', _) | (_, 'R') => {
                entries.next();
                ChangeKind::Renamed
            }
            ('D', _) | (_, 'D') => ChangeKind::Deleted,
            ('?', _) | ('A', _) => ChangeKind::Added,
            _ => ChangeKind::Modified,
        };
        changes.push(FileChange {
            path: path.to_string(),
            kind,
        });
    }
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modified_added_deleted_renamed_and_untracked() {
        let out = " M src/a.rs\0A  src/new.rs\0 D gone.rs\0R  moved.rs\0old.rs\0?? notes.md\0";
        let kinds: Vec<_> = parse_porcelain(out)
            .into_iter()
            .map(|c| (c.path, c.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                ("gone.rs".to_string(), ChangeKind::Deleted),
                ("moved.rs".to_string(), ChangeKind::Renamed),
                ("notes.md".to_string(), ChangeKind::Added),
                ("src/a.rs".to_string(), ChangeKind::Modified),
                ("src/new.rs".to_string(), ChangeKind::Added),
            ]
        );
    }
}
