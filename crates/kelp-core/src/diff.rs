use std::collections::BTreeSet;
use std::path::Path;

use gix::ObjectId;
use similar::{ChangeTag, TextDiff};

use crate::preview::Sides;

const CONTEXT_LINES: usize = 3;
const MAX_DIFF_BYTES: usize = 8 * 1024 * 1024;
const MAX_EMPHASIS_LINE: usize = 500;
const MIN_EMPHASIS_RATIO: f32 = 0.4;

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
    pub raw: String,
    pub emphasis: Vec<(usize, usize)>,
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
    pub preview: Option<Sides>,
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

pub fn staged_file(repo: &gix::Repository, path: &str) -> anyhow::Result<FileDiff> {
    let old = match repo.head_commit() {
        Ok(head) => blob_at(&head.tree()?, path)?,
        Err(_) => None,
    };
    let new = index_blob(repo, path)?;
    Ok(build(path, old.as_deref(), new.as_deref()))
}

pub fn unstaged_file(
    repo: &gix::Repository,
    workdir: &Path,
    path: &str,
) -> anyhow::Result<FileDiff> {
    let old = index_blob(repo, path)?;
    let new = std::fs::read(workdir.join(path)).ok();
    Ok(build(path, old.as_deref(), new.as_deref()))
}

pub fn index_blob(repo: &gix::Repository, path: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let index = repo.index_or_empty()?;
    let Some(entry) = index.entry_by_path(path.into()) else {
        return Ok(None);
    };
    Ok(Some(repo.find_object(entry.id)?.detach().data))
}

pub fn hunk_patch(diff: &FileDiff, hunk: usize) -> Option<String> {
    let Body::Text(hunks) = &diff.body else {
        return None;
    };
    let lines = &hunks.get(hunk)?.lines;
    let old_text = diff.old_text.as_deref().unwrap_or_default();
    let new_text = diff.new_text.as_deref().unwrap_or_default();
    let old_count = old_text.lines().count() as u32;
    let new_count = new_text.lines().count() as u32;
    let old_missing_newline = !old_text.is_empty() && !old_text.ends_with('\n');
    let new_missing_newline = !new_text.is_empty() && !new_text.ends_with('\n');

    let old_len = lines.iter().filter(|l| l.kind != LineKind::Added).count();
    let new_len = lines.iter().filter(|l| l.kind != LineKind::Removed).count();
    let old_start = if old_len == 0 {
        0
    } else {
        lines.iter().find_map(|l| l.old).unwrap_or(0)
    };
    let new_start = if new_len == 0 {
        0
    } else {
        lines.iter().find_map(|l| l.new).unwrap_or(0)
    };

    let path = &diff.path;
    let mut patch = format!("diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n");
    patch.push_str(&format!(
        "@@ -{old_start},{old_len} +{new_start},{new_len} @@\n"
    ));
    for line in lines {
        let prefix = match line.kind {
            LineKind::Context => ' ',
            LineKind::Added => '+',
            LineKind::Removed => '-',
        };
        patch.push(prefix);
        patch.push_str(&line.raw);
        patch.push('\n');
        let ends_old =
            line.kind != LineKind::Added && old_missing_newline && line.old == Some(old_count);
        let ends_new =
            line.kind != LineKind::Removed && new_missing_newline && line.new == Some(new_count);
        if ends_old || ends_new {
            patch.push_str("\\ No newline at end of file\n");
        }
    }
    Some(patch)
}

pub fn range_file(
    repo: &gix::Repository,
    workdir: Option<&Path>,
    base: ObjectId,
    target: Option<ObjectId>,
    path: &str,
    old_path: Option<&str>,
) -> anyhow::Result<FileDiff> {
    let old = blob_at(&repo.find_commit(base)?.tree()?, old_path.unwrap_or(path))?;
    let new = match (target, workdir) {
        (Some(id), _) => blob_at(&repo.find_commit(id)?.tree()?, path)?,
        (None, Some(workdir)) => std::fs::read(workdir.join(path)).ok(),
        (None, None) => anyhow::bail!("this repository has no working tree"),
    };
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
    let preview = Sides::capture(path, old, new);
    let old = old.unwrap_or_default();
    let new = new.unwrap_or_default();
    let empty = |body| FileDiff {
        path: path.to_string(),
        body,
        added: 0,
        removed: 0,
        old_text: None,
        new_text: None,
        preview: preview.clone(),
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
                        raw: change
                            .value()
                            .strip_suffix('\n')
                            .unwrap_or(change.value())
                            .to_string(),
                        emphasis: Vec::new(),
                    });
                }
            }
            emphasize_pairs(&mut lines);
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
        preview,
    }
}

pub fn lines_patch(
    diff: &FileDiff,
    indexes_across_hunks: &BTreeSet<usize>,
    staged: bool,
) -> Option<String> {
    let Body::Text(hunks) = &diff.body else {
        return None;
    };
    let (old_text, new_text) = (diff.old_text.as_deref()?, diff.new_text.as_deref()?);
    let (base, incoming) = if staged {
        (new_text, old_text)
    } else {
        (old_text, new_text)
    };
    let base_lines: Vec<&str> = base.split_inclusive('\n').collect();
    let incoming_lines: Vec<&str> = incoming.split_inclusive('\n').collect();
    let eol = if base.contains("\r\n") { "\r\n" } else { "\n" };
    let base_number = |l: &Line| if staged { l.new } else { l.old };
    let incoming_number = |l: &Line| if staged { l.old } else { l.new };
    let incoming_kind = if staged {
        LineKind::Removed
    } else {
        LineKind::Added
    };

    let mut target = String::with_capacity(base.len());
    let mut copied = 0;
    let lines = hunks.iter().flat_map(|h| &h.lines).enumerate();
    for (index, line) in lines {
        let chosen = indexes_across_hunks.contains(&index);
        if line.kind == incoming_kind {
            if chosen {
                push_line(
                    &mut target,
                    incoming_lines.get(incoming_number(line)? as usize - 1)?,
                    eol,
                );
            }
            continue;
        }
        let number = base_number(line)? as usize;
        let keep_until = if line.kind != LineKind::Context && chosen {
            number - 1
        } else {
            number
        };
        for base_line in base_lines.get(copied..keep_until.max(copied))? {
            push_line(&mut target, base_line, eol);
        }
        copied = number;
    }
    for base_line in base_lines.get(copied..)? {
        push_line(&mut target, base_line, eol);
    }
    if target == base {
        return None;
    }
    let (from, to) = if staged {
        (target.as_str(), base)
    } else {
        (base, target.as_str())
    };
    let path = &diff.path;
    let body = TextDiff::from_lines(from, to)
        .unified_diff()
        .context_radius(CONTEXT_LINES)
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string();
    Some(format!("diff --git a/{path} b/{path}\n{body}"))
}

/// Applied with `git apply --reverse` to the work tree of an unstaged diff,
/// takes back only the chosen changed lines.
pub fn discard_lines_patch(
    diff: &FileDiff,
    indexes_across_hunks: &BTreeSet<usize>,
) -> Option<String> {
    lines_patch(diff, indexes_across_hunks, true)
}

fn push_line(target: &mut String, line: &str, eol: &str) {
    if !target.is_empty() && !target.ends_with('\n') {
        target.push_str(eol);
    }
    target.push_str(line);
}

fn emphasize_pairs(lines: &mut [Line]) {
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind != LineKind::Removed {
            i += 1;
            continue;
        }
        let removed_end = run_end(lines, i, LineKind::Removed);
        let added_end = run_end(lines, removed_end, LineKind::Added);
        let pairs = (removed_end - i).min(added_end - removed_end);
        for k in 0..pairs {
            let (old, new) = (i + k, removed_end + k);
            if let Some((old_spans, new_spans)) = word_emphasis(&lines[old].text, &lines[new].text)
            {
                lines[old].emphasis = old_spans;
                lines[new].emphasis = new_spans;
            }
        }
        i = added_end.max(i + 1);
    }
}

fn run_end(lines: &[Line], from: usize, kind: LineKind) -> usize {
    from + lines[from..].iter().take_while(|l| l.kind == kind).count()
}

type Spans = Vec<(usize, usize)>;

fn word_emphasis(old: &str, new: &str) -> Option<(Spans, Spans)> {
    if old.len() > MAX_EMPHASIS_LINE || new.len() > MAX_EMPHASIS_LINE {
        return None;
    }
    let (old_tokens, new_tokens) = (tokens(old), tokens(new));
    let ops = similar::capture_diff_slices(
        similar::Algorithm::Myers,
        &token_texts(&old_tokens),
        &token_texts(&new_tokens),
    );
    let equal: usize = ops
        .iter()
        .filter(|op| op.tag() == similar::DiffTag::Equal)
        .map(|op| op.old_range().len())
        .sum();
    let total = old_tokens.len() + new_tokens.len();
    if total == 0 || (2 * equal) as f32 / (total as f32) < MIN_EMPHASIS_RATIO {
        return None;
    }
    let (mut old_spans, mut new_spans) = (Vec::new(), Vec::new());
    for op in &ops {
        if op.tag() == similar::DiffTag::Equal {
            continue;
        }
        cover(&mut old_spans, &old_tokens, op.old_range());
        cover(&mut new_spans, &new_tokens, op.new_range());
    }
    (!old_spans.is_empty() || !new_spans.is_empty()).then_some((old_spans, new_spans))
}

fn tokens(line: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut word_start = None;
    for (at, c) in line.char_indices() {
        let in_word = c.is_alphanumeric() || c == '_';
        match (in_word, word_start) {
            (true, None) => word_start = Some(at),
            (true, Some(_)) => {}
            (false, start) => {
                if let Some(start) = start {
                    out.push((start, &line[start..at]));
                    word_start = None;
                }
                out.push((at, &line[at..at + c.len_utf8()]));
            }
        }
    }
    if let Some(start) = word_start {
        out.push((start, &line[start..]));
    }
    out
}

fn token_texts<'a>(tokens: &[(usize, &'a str)]) -> Vec<&'a str> {
    tokens.iter().map(|&(_, text)| text).collect()
}

fn cover(spans: &mut Spans, tokens: &[(usize, &str)], range: std::ops::Range<usize>) {
    if let (Some(first), Some(last)) = (
        tokens.get(range.start),
        range.end.checked_sub(1).and_then(|i| tokens.get(i)),
    ) {
        extend_span(spans, first.0, last.0 + last.1.len());
    }
}

fn extend_span(spans: &mut Spans, start: usize, end: usize) {
    match spans.last_mut() {
        Some(last) if last.1 == start => last.1 = end,
        _ => spans.push((start, end)),
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
    fn hunk_patch_has_exact_header_and_raw_lines() {
        let d = build(
            "a.rs",
            Some(b"one\n\ttwo\nthree\n"),
            Some(b"one\n\t2\nthree\n"),
        );
        let patch = hunk_patch(&d, 0).unwrap();
        assert_eq!(
            patch,
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,3 +1,3 @@\n one\n-\ttwo\n+\t2\n three\n"
        );
    }

    #[test]
    fn hunk_patch_marks_missing_trailing_newline() {
        let d = build("a.txt", Some(b"a\nb"), Some(b"a\nc"));
        let patch = hunk_patch(&d, 0).unwrap();
        assert!(
            patch.ends_with("-b\n\\ No newline at end of file\n+c\n\\ No newline at end of file\n"),
            "{patch}"
        );
    }

    #[test]
    fn changed_words_are_emphasized_in_paired_lines() {
        let d = build(
            "a.rs",
            Some(b"let total = price * 2;\nkeep\n"),
            Some(b"let total = price * 3;\nkeep\n"),
        );
        let Body::Text(hunks) = d.body else {
            panic!("expected text")
        };
        let spans = |kind| {
            let line = hunks[0].lines.iter().find(|l| l.kind == kind).unwrap();
            line.emphasis
                .iter()
                .map(|&(a, b)| line.text[a..b].to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(spans(LineKind::Removed), ["2"]);
        assert_eq!(spans(LineKind::Added), ["3"]);

        let version = word_emphasis("\"v0.1.0\",", "\"v0.1.1\",").unwrap();
        assert_eq!((version.0, version.1), (vec![(6, 7)], vec![(6, 7)]));
    }

    #[test]
    fn unrelated_lines_get_no_emphasis() {
        let d = build("a.rs", Some(b"fn main() {}\n"), Some(b"struct Point;\n"));
        let Body::Text(hunks) = d.body else {
            panic!("expected text")
        };
        assert!(hunks[0].lines.iter().all(|l| l.emphasis.is_empty()));
    }

    #[test]
    fn lines_patch_keeps_unselected_changes_out() {
        let d = build("a.txt", Some(b"a\nb\nc\n"), Some(b"a\nB\nc\nd\n"));
        let added_d = 4;
        let patch = lines_patch(&d, &BTreeSet::from([added_d]), false).unwrap();
        assert!(
            patch.contains("+d\n") && !patch.contains("+B") && !patch.contains("-b"),
            "{patch}"
        );
        assert!(lines_patch(&d, &BTreeSet::new(), false).is_none());
    }

    #[test]
    fn nul_bytes_mean_binary() {
        assert!(matches!(
            build("img.png", None, Some(b"\x89PNG\0\0")).body,
            Body::Binary
        ));
    }
}
