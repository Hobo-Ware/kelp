use std::path::Path;

use gix::ObjectId;
use similar::{ChangeTag, TextDiff};

const CONTEXT_LINES: usize = 3;
const MAX_DIFF_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone)]
pub struct Line {
    pub kind: LineKind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<Line>,
}

#[derive(Debug, Clone)]
pub enum Body {
    Text(Vec<Hunk>),
    Binary,
    TooLarge,
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub body: Body,
    pub added: usize,
    pub removed: usize,
    pub old_text: Option<String>,
    pub new_text: Option<String>,
}

pub fn commit_file(
    repo: &gix::Repository,
    commit: ObjectId,
    path: &str,
) -> anyhow::Result<FileDiff> {
    let commit = repo.find_commit(commit)?;
    let new = blob_at(&commit.tree()?, path)?;
    let old = match commit.parent_ids().next() {
        Some(parent) => blob_at(&repo.find_commit(parent.detach())?.tree()?, path)?,
        None => None,
    };
    Ok(build(path, old.as_deref(), new.as_deref()))
}

pub fn working_file(
    repo: &gix::Repository,
    workdir: &Path,
    path: &str,
) -> anyhow::Result<FileDiff> {
    let old = match repo.head_commit() {
        Ok(head) => blob_at(&head.tree()?, path)?,
        Err(_) => None,
    };
    let new = std::fs::read(workdir.join(path)).ok();
    Ok(build(path, old.as_deref(), new.as_deref()))
}

pub fn file_at(
    repo: &gix::Repository,
    commit: ObjectId,
    path: &str,
) -> anyhow::Result<Option<Vec<u8>>> {
    blob_at(&repo.find_commit(commit)?.tree()?, path)
}

fn blob_at(tree: &gix::Tree<'_>, path: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let Some(entry) = tree.lookup_entry_by_path(path)? else {
        return Ok(None);
    };
    if !entry.mode().is_blob() {
        return Ok(None);
    }
    Ok(Some(entry.object()?.detach().data))
}

pub fn build(path: &str, old: Option<&[u8]>, new: Option<&[u8]>) -> FileDiff {
    let old = old.unwrap_or_default();
    let new = new.unwrap_or_default();
    let empty = |body| FileDiff {
        path: path.to_string(),
        body,
        added: 0,
        removed: 0,
        old_text: None,
        new_text: None,
    };
    if old.len() + new.len() > MAX_DIFF_BYTES {
        return empty(Body::TooLarge);
    }
    let (Some(old_text), Some(new_text)) = (as_text(old), as_text(new)) else {
        return empty(Body::Binary);
    };
    let diff = TextDiff::from_lines(old_text, new_text);
    let (mut added, mut removed) = (0, 0);
    let hunks = diff
        .grouped_ops(CONTEXT_LINES)
        .iter()
        .map(|group| {
            let mut lines = Vec::new();
            for op in group {
                for change in diff.iter_changes(op) {
                    let kind = match change.tag() {
                        ChangeTag::Equal => LineKind::Context,
                        ChangeTag::Insert => {
                            added += 1;
                            LineKind::Added
                        }
                        ChangeTag::Delete => {
                            removed += 1;
                            LineKind::Removed
                        }
                    };
                    lines.push(Line {
                        kind,
                        old: change.old_index().map(|i| i as u32 + 1),
                        new: change.new_index().map(|i| i as u32 + 1),
                        text: change
                            .value()
                            .trim_end_matches(['\n', '\r'])
                            .replace('\t', "    "),
                    });
                }
            }
            Hunk {
                header: hunk_header(&lines),
                lines,
            }
        })
        .collect();
    FileDiff {
        path: path.to_string(),
        body: Body::Text(hunks),
        added,
        removed,
        old_text: Some(old_text.to_string()),
        new_text: Some(new_text.to_string()),
    }
}

fn as_text(bytes: &[u8]) -> Option<&str> {
    if bytes.iter().take(8000).any(|&b| b == 0) {
        return None;
    }
    std::str::from_utf8(bytes).ok()
}

fn hunk_header(lines: &[Line]) -> String {
    let span = |pick: fn(&Line) -> Option<u32>| {
        let nums: Vec<u32> = lines.iter().filter_map(pick).collect();
        match (nums.first(), nums.len()) {
            (Some(start), len) => format!("{start},{len}"),
            (None, _) => "0,0".into(),
        }
    };
    format!("@@ -{} +{} @@", span(|l| l.old), span(|l| l.new))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_added_and_removed_lines_with_numbers() {
        let d = build(
            "a.rs",
            Some(b"one\ntwo\nthree\n"),
            Some(b"one\n2\nthree\nfour\n"),
        );
        assert_eq!((d.added, d.removed), (2, 1));
        let Body::Text(hunks) = d.body else {
            panic!("expected text")
        };
        let lines: Vec<_> = hunks[0]
            .lines
            .iter()
            .map(|l| (l.kind, l.old, l.new, l.text.as_str()))
            .collect();
        assert_eq!(
            lines,
            [
                (LineKind::Context, Some(1), Some(1), "one"),
                (LineKind::Removed, Some(2), None, "two"),
                (LineKind::Added, None, Some(2), "2"),
                (LineKind::Context, Some(3), Some(3), "three"),
                (LineKind::Added, None, Some(4), "four"),
            ]
        );
        assert_eq!(hunks[0].header, "@@ -1,3 +1,4 @@");
    }

    #[test]
    fn new_file_is_all_additions() {
        let d = build("n.txt", None, Some(b"a\nb\n"));
        assert_eq!((d.added, d.removed), (2, 0));
    }

    #[test]
    fn nul_bytes_mean_binary() {
        assert!(matches!(
            build("img.png", None, Some(b"\x89PNG\0\0")).body,
            Body::Binary
        ));
    }
}
