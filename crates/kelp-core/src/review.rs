use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const CONTEXT: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Old,
    New,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    pub side: Side,
    pub line_hint: u32,
    pub text: String,
    pub before: Vec<String>,
    pub after: Vec<String>,
    /// How many lines below `line_hint` the note covers. Zero is a single line.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub span: u32,
    /// The covered lines below `text`, one per extra line, for finding the block again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub block: Vec<String>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub author: String,
    pub time: i64,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    pub id: u64,
    pub path: String,
    pub commit: Option<String>,
    pub anchor: Anchor,
    pub resolved: bool,
    pub comments: Vec<Comment>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Review {
    pub threads: Vec<Thread>,
    next_id: u64,
    #[serde(skip)]
    file: PathBuf,
}

impl Anchor {
    pub fn at(lines: &[&str], index: usize, side: Side) -> Self {
        Self::over(lines, index, 0, side)
    }

    /// An anchor on `index` and the `span` lines below it, cut short at the end of `lines`.
    pub fn over(lines: &[&str], index: usize, span: u32, side: Side) -> Self {
        let span = (span as usize).min(lines.len() - 1 - index);
        let last = index + span;
        let start = index.saturating_sub(CONTEXT);
        let end = (last + 1 + CONTEXT).min(lines.len());
        let owned = |range: &[&str]| range.iter().map(|s| s.to_string()).collect();
        Self {
            side,
            line_hint: index as u32 + 1,
            text: lines[index].to_string(),
            before: owned(&lines[start..index]),
            after: owned(&lines[last + 1..end]),
            span: span as u32,
            block: owned(&lines[index + 1..=last]),
        }
    }

    /// The 1-based number of the last covered line.
    pub fn last_line_hint(&self) -> u32 {
        self.line_hint + self.span
    }

    /// The index of the last covered line when the first one is at `first` in a file of `len`
    /// lines.
    pub fn last_index(&self, first: usize, len: usize) -> usize {
        (first + self.span as usize).min(len.saturating_sub(1))
    }

    pub fn locate(&self, lines: &[&str]) -> Option<usize> {
        let hint = self.line_hint.saturating_sub(1) as usize;
        lines
            .iter()
            .enumerate()
            .filter(|(_, line)| **line == self.text)
            .map(|(i, _)| (i, self.context_score(lines, i)))
            .max_by(|(ia, sa), (ib, sb)| {
                sa.cmp(sb)
                    .then_with(|| ib.abs_diff(hint).cmp(&ia.abs_diff(hint)))
            })
            .map(|(i, _)| i)
    }

    fn context_score(&self, lines: &[&str], i: usize) -> usize {
        let before = self
            .before
            .iter()
            .rev()
            .enumerate()
            .filter(|(k, text)| i > *k && lines[i - k - 1] == text.as_str())
            .count();
        let block = self
            .block
            .iter()
            .enumerate()
            .filter(|(k, text)| lines.get(i + k + 1) == Some(&text.as_str()))
            .count();
        let after_from = i + self.block.len() + 1;
        let after = self
            .after
            .iter()
            .enumerate()
            .filter(|(k, text)| lines.get(after_from + k) == Some(&text.as_str()))
            .count();
        before + block + after
    }
}

impl Review {
    pub fn load(git_common_dir: &Path) -> Self {
        let file = git_common_dir.join("kelp").join("comments.json");
        let mut review: Review = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        review.file = file;
        review
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.file.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, &self.file)?;
        Ok(())
    }

    pub fn add(
        &mut self,
        path: &str,
        commit: Option<String>,
        anchor: Anchor,
        comment: Comment,
    ) -> u64 {
        self.next_id += 1;
        self.threads.push(Thread {
            id: self.next_id,
            path: path.to_string(),
            commit,
            anchor,
            resolved: false,
            comments: vec![comment],
        });
        self.next_id
    }

    pub fn reply(&mut self, id: u64, comment: Comment) {
        if let Some(thread) = self.threads.iter_mut().find(|t| t.id == id) {
            thread.comments.push(comment);
            thread.resolved = false;
        }
    }

    pub fn set_resolved(&mut self, id: u64, resolved: bool) {
        if let Some(thread) = self.threads.iter_mut().find(|t| t.id == id) {
            thread.resolved = resolved;
        }
    }

    pub fn delete(&mut self, id: u64) {
        self.threads.retain(|t| t.id != id);
    }

    pub fn for_path<'a>(&'a self, path: &'a str) -> impl Iterator<Item = &'a Thread> {
        self.threads.iter().filter(move |t| t.path == path)
    }

    pub fn count_for(&self, path: &str) -> usize {
        self.for_path(path).count()
    }

    pub fn open_count(&self) -> usize {
        self.threads.iter().filter(|t| !t.resolved).count()
    }

    /// Unresolved threads per commit they were written on.
    pub fn open_by_commit(&self) -> HashMap<gix::ObjectId, usize> {
        let mut counts = HashMap::new();
        let commits = self
            .threads
            .iter()
            .filter(|t| !t.resolved)
            .filter_map(|t| gix::ObjectId::from_hex(t.commit.as_deref()?.as_bytes()).ok());
        for id in commits {
            *counts.entry(id).or_insert(0) += 1;
        }
        counts
    }

    /// Unresolved threads written on uncommitted changes.
    pub fn open_uncommitted(&self) -> usize {
        self.threads
            .iter()
            .filter(|t| !t.resolved && t.commit.is_none())
            .count()
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::from("# Review\n");
        let mut paths: Vec<&str> = self.threads.iter().map(|t| t.path.as_str()).collect();
        paths.sort_unstable();
        paths.dedup();
        for path in paths {
            out.push_str(&format!("\n## `{path}`\n"));
            let mut threads: Vec<&Thread> = self.for_path(path).collect();
            threads.sort_by_key(|t| t.anchor.line_hint);
            for thread in threads {
                let status = if thread.resolved { " (resolved)" } else { "" };
                let anchor = &thread.anchor;
                let place = if anchor.span == 0 {
                    format!("Line {}", anchor.line_hint)
                } else {
                    format!("Lines {}-{}", anchor.line_hint, anchor.last_line_hint())
                };
                out.push_str(&format!("\n**{place}**{status}\n\n"));
                let code: Vec<&str> = std::iter::once(anchor.text.as_str())
                    .chain(anchor.block.iter().map(String::as_str))
                    .collect();
                out.push_str(&format!("```\n{}\n```\n\n", code.join("\n")));
                for comment in &thread.comments {
                    out.push_str(&format!(
                        "- **{}:** {}\n",
                        comment.author,
                        comment.body.replace('\n', "\n  ")
                    ));
                }
            }
        }
        out
    }
}

pub fn author_name(repo: &gix::Repository) -> String {
    repo.config_snapshot()
        .string("user.name")
        .map(|n| n.to_string())
        .unwrap_or_else(|| "You".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment(body: &str) -> Comment {
        Comment {
            author: "Vlad".into(),
            time: 0,
            body: body.into(),
        }
    }

    const FILE: &[&str] = &[
        "fn a() {",
        "    let x = 1;",
        "    let bend = row * 0.5;",
        "    draw(x);",
        "}",
    ];

    #[test]
    fn anchor_follows_its_line_when_code_is_added_above() {
        let anchor = Anchor::at(FILE, 2, Side::New);
        let edited = [
            "// new header",
            "use std::fmt;",
            "",
            "fn a() {",
            "    let x = 1;",
            "    let bend = row * 0.5;",
            "    draw(x);",
            "}",
        ];
        assert_eq!(anchor.locate(&edited), Some(5));
    }

    #[test]
    fn anchor_prefers_the_copy_with_matching_context() {
        let anchor = Anchor::at(FILE, 2, Side::New);
        let duplicated = [
            "fn b() {",
            "    let y = 2;",
            "    let bend = row * 0.5;",
            "}",
            "fn a() {",
            "    let x = 1;",
            "    let bend = row * 0.5;",
            "    draw(x);",
            "}",
        ];
        assert_eq!(anchor.locate(&duplicated), Some(6));
    }

    #[test]
    fn anchor_is_lost_when_the_line_is_gone() {
        let anchor = Anchor::at(FILE, 2, Side::New);
        assert_eq!(anchor.locate(&["fn a() {", "}"]), None);
    }

    #[test]
    fn comments_survive_a_restart() {
        let dir = std::env::temp_dir().join(format!("kelp-review-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut review = Review::load(&dir);
        let id = review.add(
            "src/graph.rs",
            None,
            Anchor::at(FILE, 2, Side::New),
            comment("half a row?"),
        );
        review.reply(id, comment("try a full row"));
        review.set_resolved(id, true);
        review.save().unwrap();

        let reloaded = Review::load(&dir);
        assert_eq!(reloaded.threads, review.threads);
        assert!(dir.join("kelp").join("comments.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn markdown_export_groups_by_file() {
        let mut review = Review::default();
        review.add(
            "b.rs",
            None,
            Anchor::at(FILE, 1, Side::New),
            comment("rename x"),
        );
        review.add(
            "a.rs",
            None,
            Anchor::at(FILE, 2, Side::New),
            comment("half a row?"),
        );
        let md = review.to_markdown();
        assert!(md.find("## `a.rs`").unwrap() < md.find("## `b.rs`").unwrap());
        assert!(md.contains("**Line 3**"));
        assert!(md.contains("- **Vlad:** half a row?"));
    }

    fn block_file() -> Vec<&'static str> {
        vec![
            "fn a() {",
            "    let x = 1;",
            "    let bend = row * 0.5;",
            "    draw(x);",
            "    flush();",
            "}",
        ]
    }

    #[test]
    fn a_span_keeps_the_first_line_as_text_and_the_rest_as_a_block() {
        let lines = block_file();
        let anchor = Anchor::over(&lines, 1, 2, Side::New);
        assert_eq!(anchor.line_hint, 2);
        assert_eq!(anchor.span, 2);
        assert_eq!(anchor.last_line_hint(), 4);
        assert_eq!(anchor.text, "    let x = 1;");
        assert_eq!(anchor.block, ["    let bend = row * 0.5;", "    draw(x);"]);
        assert_eq!(anchor.before, ["fn a() {"]);
        assert_eq!(anchor.after, ["    flush();", "}"]);
        assert_eq!(Anchor::at(&lines, 1, Side::New).span, 0);
        assert_eq!(Anchor::at(&lines, 1, Side::New).block.len(), 0);
    }

    #[test]
    fn a_span_is_cut_short_at_the_end_of_the_file() {
        let lines = block_file();
        let anchor = Anchor::over(&lines, 4, 9, Side::New);
        assert_eq!(anchor.span, 1);
        assert_eq!(anchor.block, ["}"]);
        assert_eq!(anchor.last_index(4, lines.len()), 5);
        assert_eq!(anchor.after.len(), 0);
    }

    #[test]
    fn a_span_anchor_is_found_again_in_a_shifted_or_edited_file() {
        let lines = block_file();
        let anchor = Anchor::over(&lines, 1, 2, Side::New);
        let cases: [(&[&str], Option<usize>); 5] = [
            (&lines, Some(1)),
            (
                &[
                    "// top",
                    "",
                    "fn a() {",
                    "    let x = 1;",
                    "    let bend = row * 0.5;",
                    "    draw(x);",
                    "    flush();",
                    "}",
                ],
                Some(3),
            ),
            (
                &[
                    "fn a() {",
                    "    let x = 1;",
                    "    let bend = row * 0.25;",
                    "    draw(x);",
                    "    flush();",
                    "}",
                ],
                Some(1),
            ),
            (
                &[
                    "fn b() {",
                    "    let x = 1;",
                    "    other();",
                    "}",
                    "fn a() {",
                    "    let x = 1;",
                    "    let bend = row * 0.5;",
                    "    draw(x);",
                    "    flush();",
                    "}",
                ],
                Some(5),
            ),
            (&["fn a() {", "    let y = 1;", "}"], None),
        ];
        for (edited, want) in cases {
            assert_eq!(anchor.locate(edited), want, "{edited:?}");
        }
    }

    #[test]
    fn the_block_breaks_a_tie_between_copies_of_the_first_line() {
        let anchor = Anchor::over(&["a", "dup", "one", "two", "z"], 1, 2, Side::New);
        let file = [
            "dup", "other", "x", "a", "dup", "one", "two", "z", "dup", "q",
        ];
        assert_eq!(anchor.locate(&file), Some(4));
    }

    #[test]
    fn a_note_saved_without_a_span_loads_as_one_line() {
        let json = r#"{"side":"New","line_hint":4,"text":"x","before":["w"],"after":["y"]}"#;
        let anchor: Anchor = serde_json::from_str(json).unwrap();
        assert_eq!(anchor.span, 0);
        assert_eq!(anchor.block.len(), 0);
        assert_eq!(anchor.last_line_hint(), 4);
    }

    #[test]
    fn single_line_notes_serialize_as_before_and_spans_round_trip() {
        let lines = block_file();
        let single = serde_json::to_string(&Anchor::at(&lines, 1, Side::New)).unwrap();
        assert!(
            !single.contains("span") && !single.contains("block"),
            "{single}"
        );
        let wide = Anchor::over(&lines, 1, 2, Side::Old);
        let back: Anchor = serde_json::from_str(&serde_json::to_string(&wide).unwrap()).unwrap();
        assert_eq!(back, wide);
    }

    #[test]
    fn markdown_names_the_range_and_quotes_the_block() {
        let lines = block_file();
        let mut review = Review::default();
        review.add(
            "a.rs",
            None,
            Anchor::over(&lines, 1, 2, Side::New),
            comment("split this"),
        );
        let md = review.to_markdown();
        assert!(md.contains("**Lines 2-4**"), "{md}");
        assert!(md.contains("```\n    let x = 1;\n    let bend = row * 0.5;\n    draw(x);\n```"));
    }

    #[test]
    fn open_notes_are_counted_per_commit_and_for_uncommitted_changes() {
        let a = gix::ObjectId::from_hex(&[b'a'; 40]).unwrap();
        let b = gix::ObjectId::from_hex(&[b'b'; 40]).unwrap();
        let mut review = Review::default();
        let mut add = |commit: Option<&gix::ObjectId>| {
            review.add(
                "f.rs",
                commit.map(|id| id.to_string()),
                Anchor::at(FILE, 1, Side::New),
                comment("hm"),
            )
        };
        let resolved = add(Some(&a));
        add(Some(&a));
        add(Some(&b));
        add(None);
        add(None);
        add(None);
        review.set_resolved(resolved, true);
        review.threads.push(Thread {
            commit: Some("not a hash".into()),
            ..review.threads[0].clone()
        });
        let counts = review.open_by_commit();
        assert_eq!(counts.len(), 2);
        assert_eq!((counts[&a], counts[&b]), (1, 1));
        assert_eq!(review.open_uncommitted(), 3);
        assert_eq!(Review::default().open_by_commit().len(), 0);
    }
}
