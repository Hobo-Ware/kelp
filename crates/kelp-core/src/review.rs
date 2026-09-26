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
        let start = index.saturating_sub(CONTEXT);
        let end = (index + 1 + CONTEXT).min(lines.len());
        Self {
            side,
            line_hint: index as u32 + 1,
            text: lines[index].to_string(),
            before: lines[start..index].iter().map(|s| s.to_string()).collect(),
            after: lines[index + 1..end]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
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
        let after = self
            .after
            .iter()
            .enumerate()
            .filter(|(k, text)| lines.get(i + k + 1) == Some(&text.as_str()))
            .count();
        before + after
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
                out.push_str(&format!(
                    "\n**Line {}**{status}\n\n",
                    thread.anchor.line_hint
                ));
                out.push_str(&format!("```\n{}\n```\n\n", thread.anchor.text));
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
}
