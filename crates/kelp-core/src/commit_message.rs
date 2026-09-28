use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gix::ObjectId;
use serde::{Deserialize, Serialize};

pub const TYPES: [&str; 11] = [
    "feat", "fix", "docs", "chore", "refactor", "test", "perf", "ci", "build", "style", "revert",
];

const DETECT_WINDOW: usize = 50;
const DETECT_MIN_SUBJECTS: usize = 5;
const DETECT_SHARE: f32 = 0.5;
const CO_AUTHOR: &str = "Co-authored-by:";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Prefix {
    pub kind: String,
    pub scope: String,
    pub breaking: bool,
}

pub fn split_prefix(summary: &str) -> (Option<Prefix>, &str) {
    let Some(colon) = summary.find(": ") else {
        return (None, summary);
    };
    let head = &summary[..colon];
    let (head, breaking) = match head.strip_suffix('!') {
        Some(rest) => (rest, true),
        None => (head, false),
    };
    let (kind, scope) = match head.find('(') {
        Some(open) if head.ends_with(')') => (&head[..open], &head[open + 1..head.len() - 1]),
        Some(_) => return (None, summary),
        None => (head, ""),
    };
    let valid_kind = !kind.is_empty() && kind.chars().all(|c| c.is_ascii_lowercase() || c == '-');
    if !valid_kind || scope.contains(['(', ')', ' ']) {
        return (None, summary);
    }
    let prefix = Prefix {
        kind: kind.to_string(),
        scope: scope.to_string(),
        breaking,
    };
    (Some(prefix), &summary[colon + 2..])
}

pub fn with_prefix(summary: &str, prefix: Option<&Prefix>) -> String {
    let (_, rest) = split_prefix(summary);
    match prefix {
        None => rest.to_string(),
        Some(p) if p.kind.is_empty() => rest.to_string(),
        Some(p) => {
            let scope = if p.scope.trim().is_empty() {
                String::new()
            } else {
                format!("({})", p.scope.trim())
            };
            let bang = if p.breaking { "!" } else { "" };
            format!("{}{scope}{bang}: {rest}", p.kind)
        }
    }
}

pub fn uses_conventional<S: AsRef<str>>(subjects: &[S]) -> bool {
    let recent: Vec<&str> = subjects
        .iter()
        .take(DETECT_WINDOW)
        .map(AsRef::as_ref)
        .collect();
    if recent.len() < DETECT_MIN_SUBJECTS {
        return false;
    }
    let matching = recent
        .iter()
        .filter(|s| {
            split_prefix(s)
                .0
                .is_some_and(|p| TYPES.contains(&p.kind.as_str()))
        })
        .count();
    matching as f32 / recent.len() as f32 >= DETECT_SHARE
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Person {
    pub name: String,
    pub email: String,
}

impl Person {
    pub fn trailer(&self) -> String {
        format!("{CO_AUTHOR} {} <{}>", self.name, self.email)
    }
}

pub fn co_authors(body: &str) -> Vec<Person> {
    body.lines().filter_map(parse_trailer).collect()
}

fn parse_trailer(line: &str) -> Option<Person> {
    let rest = line.trim().strip_prefix(CO_AUTHOR)?.trim();
    let open = rest.rfind('<')?;
    let close = rest.rfind('>')?;
    (open < close).then(|| Person {
        name: rest[..open].trim().to_string(),
        email: rest[open + 1..close].trim().to_string(),
    })
}

pub fn add_co_author(body: &str, person: &Person) -> String {
    if co_authors(body)
        .iter()
        .any(|p| p.email.eq_ignore_ascii_case(&person.email))
    {
        return body.to_string();
    }
    let trimmed = body.trim_end();
    let last_is_trailer = trimmed
        .lines()
        .last()
        .is_some_and(|l| parse_trailer(l).is_some());
    if trimmed.is_empty() {
        person.trailer()
    } else if last_is_trailer {
        format!("{trimmed}\n{}", person.trailer())
    } else {
        format!("{trimmed}\n\n{}", person.trailer())
    }
}

pub fn remove_co_author(body: &str, email: &str) -> String {
    let kept: Vec<&str> = body
        .lines()
        .filter(|line| parse_trailer(line).is_none_or(|p| !p.email.eq_ignore_ascii_case(email)))
        .collect();
    kept.join("\n").trim_end().to_string()
}

pub fn people(dir: &Path, ids: &[ObjectId], exclude_email: Option<&str>) -> Vec<Person> {
    let Ok(repo) = gix::discover(dir) else {
        return Vec::new();
    };
    let mut counts: HashMap<String, (Person, usize)> = HashMap::new();
    for id in ids {
        let Ok(commit) = repo.find_commit(*id) else {
            continue;
        };
        let Ok(author) = commit.author() else {
            continue;
        };
        let email = author.email.to_string();
        if exclude_email.is_some_and(|me| me.eq_ignore_ascii_case(&email)) || email.is_empty() {
            continue;
        }
        let entry = counts.entry(email.to_ascii_lowercase()).or_insert_with(|| {
            (
                Person {
                    name: author.name.to_string(),
                    email: email.clone(),
                },
                0,
            )
        });
        entry.1 += 1;
    }
    let mut list: Vec<(Person, usize)> = counts.into_values().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));
    list.into_iter().map(|(p, _)| p).collect()
}

pub fn subjects(dir: &Path, ids: &[ObjectId]) -> Vec<String> {
    let Ok(repo) = gix::discover(dir) else {
        return Vec::new();
    };
    ids.iter()
        .take(DETECT_WINDOW)
        .filter_map(|id| repo.find_commit(*id).ok())
        .filter_map(|c| c.message().ok().map(|m| m.title.to_string()))
        .map(|t| t.trim().to_string())
        .collect()
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub conventional: Option<bool>,
}

impl Prefs {
    fn file(common_dir: &Path) -> PathBuf {
        common_dir.join("kelp").join("commit.json")
    }

    pub fn load(common_dir: &Path) -> Self {
        std::fs::read(Self::file(common_dir))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, common_dir: &Path) {
        let file = Self::file(common_dir);
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(file, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefix(kind: &str, scope: &str, breaking: bool) -> Prefix {
        Prefix {
            kind: kind.into(),
            scope: scope.into(),
            breaking,
        }
    }

    #[test]
    fn prefixes_are_split_off_the_summary() {
        assert_eq!(
            split_prefix("feat(graph)!: draw arcs"),
            (Some(prefix("feat", "graph", true)), "draw arcs")
        );
        assert_eq!(
            split_prefix("fix: typo"),
            (Some(prefix("fix", "", false)), "typo")
        );
        assert_eq!(split_prefix("Merge branch 'x': y").0, None);
        assert_eq!(split_prefix("Update README").0, None);
        assert_eq!(split_prefix("feat(a b): nope").0, None);
    }

    #[test]
    fn a_new_prefix_replaces_the_old_one() {
        assert_eq!(
            with_prefix("fix: typo", Some(&prefix("docs", "readme", false))),
            "docs(readme): typo"
        );
        assert_eq!(
            with_prefix("add arcs", Some(&prefix("feat", "", true))),
            "feat!: add arcs"
        );
        assert_eq!(with_prefix("feat(x): add arcs", None), "add arcs");
    }

    #[test]
    fn conventional_history_is_detected() {
        let yes = [
            "feat: a",
            "fix(ui): b",
            "docs: c",
            "chore: d",
            "Merge branch 'x'",
            "refactor: e",
        ];
        assert!(uses_conventional(&yes));
        let no = ["Add a", "Fix b", "Update c", "feat: d", "Tweak e", "Bump f"];
        assert!(!uses_conventional(&no));
        assert!(!uses_conventional(&["feat: a", "fix: b"]));
    }

    #[test]
    fn co_author_trailers_are_added_once_and_removed() {
        let maya = Person {
            name: "Maya Lindqvist".into(),
            email: "maya@example.com".into(),
        };
        let sam = Person {
            name: "Sam Whitaker".into(),
            email: "sam@example.com".into(),
        };
        let body = add_co_author("Explains the change.", &maya);
        assert_eq!(
            body,
            "Explains the change.\n\nCo-authored-by: Maya Lindqvist <maya@example.com>"
        );
        assert_eq!(add_co_author(&body, &maya), body);
        let both = add_co_author(&body, &sam);
        assert!(
            both.ends_with("<maya@example.com>\nCo-authored-by: Sam Whitaker <sam@example.com>")
        );
        assert_eq!(co_authors(&both), vec![maya.clone(), sam.clone()]);
        assert_eq!(
            remove_co_author(&both, "MAYA@example.com"),
            add_co_author("Explains the change.", &sam)
        );
        assert_eq!(add_co_author("", &sam), sam.trailer());
    }
}
