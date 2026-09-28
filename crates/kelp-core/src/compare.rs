use std::path::Path;

use anyhow::Context;
use gix::ObjectId;

use crate::commit::ChangeKind;
use crate::git_cli;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Commit(ObjectId),
    WorkTree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeChange {
    pub path: String,
    pub old_path: Option<String>,
    pub kind: ChangeKind,
}

pub fn changed_files(dir: &Path, base: ObjectId, target: Side) -> anyhow::Result<Vec<RangeChange>> {
    let base = base.to_string();
    let target = match target {
        Side::Commit(id) => Some(id.to_string()),
        Side::WorkTree => None,
    };
    let mut args = vec!["diff", "--name-status", "-z", "-M", base.as_str()];
    args.extend(target.as_deref());
    let out = git_cli::run(dir, &args).context("could not compare the two revisions")?;
    Ok(parse_name_status(&out))
}

pub fn parse_name_status(out: &str) -> Vec<RangeChange> {
    let mut fields = out.split('\0').filter(|f| !f.is_empty());
    let mut changes = Vec::new();
    while let Some(status) = fields.next() {
        let kind = match status.chars().next() {
            Some('A') => ChangeKind::Added,
            Some('D') => ChangeKind::Deleted,
            Some('R' | 'C') => ChangeKind::Renamed,
            Some(_) => ChangeKind::Modified,
            None => continue,
        };
        let (old_path, path) = if kind == ChangeKind::Renamed {
            let (Some(old), Some(new)) = (fields.next(), fields.next()) else {
                break;
            };
            (Some(old.to_string()), new.to_string())
        } else {
            let Some(path) = fields.next() else {
                break;
            };
            (None, path.to_string())
        };
        changes.push(RangeChange {
            path,
            old_path,
            kind,
        });
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_status_with_renames_and_spaces() {
        let out = "M\0src/a b.rs\0R087\0old/x.rs\0new/x.rs\0A\0added.txt\0D\0gone.txt\0";
        let changes = parse_name_status(out);
        assert_eq!(changes.len(), 4);
        assert_eq!(changes[0].path, "src/a b.rs");
        assert_eq!(changes[1].kind, ChangeKind::Renamed);
        assert_eq!(changes[1].old_path.as_deref(), Some("old/x.rs"));
        assert_eq!(changes[1].path, "new/x.rs");
        assert_eq!(changes[2].kind, ChangeKind::Added);
        assert_eq!(changes[3].kind, ChangeKind::Deleted);
    }
}
