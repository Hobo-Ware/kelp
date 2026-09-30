use gix::ObjectId;
use gix::object::tree::diff::ChangeDetached;

#[derive(Debug, Clone)]
pub struct Summary {
    pub title: String,
    pub body_preview: String,
    pub author: String,
    pub email: String,
    /// Committer date, so an amended commit reads as recent.
    pub time: i64,
}

#[derive(Debug, Clone)]
pub struct Details {
    pub id: ObjectId,
    pub title: String,
    pub body: String,
    pub author: String,
    pub email: String,
    /// Author date; it survives amends and rebases.
    pub time: i64,
    /// Committer date; moves whenever the commit is rewritten.
    pub commit_time: i64,
    pub committer: String,
    pub committer_email: String,
    pub parents: Vec<ObjectId>,
    pub changes: Vec<FileChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
}

pub fn summary(repo: &gix::Repository, id: ObjectId) -> anyhow::Result<Summary> {
    let commit = repo.find_commit(id)?;
    let message = commit.message().map_err(|e| e.into_error())?;
    let author = commit.author().map_err(|e| e.into_error())?;
    let committer = commit.committer().map_err(|e| e.into_error())?;
    let body_preview = message
        .body
        .map(|b| {
            b.to_string()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    Ok(Summary {
        title: message.title.to_string().trim().to_string(),
        body_preview,
        author: author.name.to_string(),
        email: author.email.to_string(),
        time: committer.time().map_err(|e| e.into_error())?.seconds,
    })
}

pub fn details(repo: &gix::Repository, id: ObjectId) -> anyhow::Result<Details> {
    let commit = repo.find_commit(id)?;
    let message = commit.message().map_err(|e| e.into_error())?;
    let author = commit.author().map_err(|e| e.into_error())?;
    let committer = commit.committer().map_err(|e| e.into_error())?;
    let parents: Vec<ObjectId> = commit.parent_ids().map(|p| p.detach()).collect();
    let tree = commit.tree()?;
    let parent_tree = match parents.first() {
        Some(p) => Some(repo.find_commit(*p)?.tree()?),
        None => None,
    };
    let mut changes: Vec<FileChange> = repo
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)?
        .into_iter()
        .filter_map(file_change)
        .collect();
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Details {
        id,
        title: message.title.to_string().trim().to_string(),
        body: message
            .body
            .map(|b| reflow(&b.to_string()))
            .unwrap_or_default(),
        author: author.name.to_string(),
        email: author.email.to_string(),
        time: author.time().map_err(|e| e.into_error())?.seconds,
        commit_time: committer.time().map_err(|e| e.into_error())?.seconds,
        committer: committer.name.to_string(),
        committer_email: committer.email.to_string(),
        parents,
        changes,
    })
}

fn file_change(change: ChangeDetached) -> Option<FileChange> {
    let (path, kind, mode) = match change {
        ChangeDetached::Addition {
            location,
            entry_mode,
            ..
        } => (location, ChangeKind::Added, entry_mode),
        ChangeDetached::Deletion {
            location,
            entry_mode,
            ..
        } => (location, ChangeKind::Deleted, entry_mode),
        ChangeDetached::Modification {
            location,
            entry_mode,
            ..
        } => (location, ChangeKind::Modified, entry_mode),
        ChangeDetached::Rewrite {
            location,
            entry_mode,
            ..
        } => (location, ChangeKind::Renamed, entry_mode),
    };
    (!mode.is_tree()).then(|| FileChange {
        path: path.to_string(),
        kind,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

pub fn list_dir(repo: &gix::Repository, id: ObjectId, dir: &str) -> anyhow::Result<Vec<TreeEntry>> {
    let root = repo.find_commit(id)?.tree()?;
    let tree = if dir.is_empty() {
        root
    } else {
        match root.lookup_entry_by_path(dir)? {
            Some(entry) if entry.mode().is_tree() => entry.object()?.into_tree(),
            _ => return Ok(Vec::new()),
        }
    };
    let mut entries: Vec<TreeEntry> = tree
        .iter()
        .filter_map(Result::ok)
        .map(|entry| {
            let name = entry.filename().to_string();
            let path = if dir.is_empty() {
                name.clone()
            } else {
                format!("{dir}/{name}")
            };
            TreeEntry {
                is_dir: entry.mode().is_tree(),
                name,
                path,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

pub fn reflow(body: &str) -> String {
    body.split("\n\n")
        .map(|paragraph| {
            let mut out = String::new();
            for line in paragraph.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let starts_item = trimmed.starts_with(['-', '*', '•'])
                    || trimmed
                        .split_once(". ")
                        .is_some_and(|(n, _)| n.chars().all(|c| c.is_ascii_digit()));
                if !out.is_empty() {
                    out.push(if starts_item { '\n' } else { ' ' });
                }
                out.push_str(trimmed);
            }
            out
        })
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn relative_time(then: i64, now: i64) -> String {
    let secs = (now - then).max(0);
    let (n, unit) = match secs {
        0..60 => return "just now".into(),
        60..3_600 => (secs / 60, "min"),
        3_600..86_400 => (secs / 3_600, "hour"),
        86_400..604_800 => (secs / 86_400, "day"),
        604_800..2_629_800 => (secs / 604_800, "week"),
        2_629_800..31_557_600 => (secs / 2_629_800, "month"),
        _ => (secs / 31_557_600, "year"),
    };
    match (n, unit) {
        (1, "day") => "yesterday".into(),
        (n, "min") => format!("{n} min ago"),
        (1, unit) => format!("1 {unit} ago"),
        (n, unit) => format!("{n} {unit}s ago"),
    }
}

pub fn calendar_time(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let minutes = secs.rem_euclid(86_400) / 60;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year}-{month:02}-{day:02} {:02}:{:02} UTC",
        minutes / 60,
        minutes % 60
    )
}

#[cfg(test)]
mod tests {
    use super::{calendar_time, reflow, relative_time};

    #[test]
    fn calendar_times_are_utc_dates() {
        assert_eq!(calendar_time(0), "1970-01-01 00:00 UTC");
        assert_eq!(calendar_time(951_782_400), "2000-02-29 00:00 UTC");
        assert_eq!(calendar_time(1_790_530_500), "2026-09-27 17:35 UTC");
        assert_eq!(calendar_time(-60), "1969-12-31 23:59 UTC");
    }

    #[test]
    fn reflow_joins_wrapped_lines_but_keeps_list_items() {
        let body = "The scrub felt clunky,\nmostly on touch:\n\n- touch-action trapped\n  scrolls\n- pointer capture\n1. first\n2. second";
        assert_eq!(
            reflow(body),
            "The scrub felt clunky, mostly on touch:\n\n- touch-action trapped scrolls\n- pointer capture\n1. first\n2. second"
        );
    }

    #[test]
    fn relative_times_read_naturally() {
        assert_eq!(relative_time(0, 30), "just now");
        assert_eq!(relative_time(0, 720), "12 min ago");
        assert_eq!(relative_time(0, 3_600), "1 hour ago");
        assert_eq!(relative_time(0, 90_000), "yesterday");
        assert_eq!(relative_time(0, 3 * 86_400), "3 days ago");
        assert_eq!(relative_time(0, 2 * 31_557_600), "2 years ago");
    }
}
