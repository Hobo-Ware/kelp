use std::collections::{HashMap, HashSet};
use std::path::Path;

use gix::ObjectId;

use crate::git_cli;

const DAY: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Period {
    #[default]
    Any,
    Day,
    Week,
    Month,
    Custom {
        from: Option<i64>,
        to: Option<i64>,
    },
}

impl Period {
    pub fn bounds(self, now: i64) -> (Option<i64>, Option<i64>) {
        match self {
            Period::Any => (None, None),
            Period::Day => (Some(now - DAY), None),
            Period::Week => (Some(now - 7 * DAY), None),
            Period::Month => (Some(now - 30 * DAY), None),
            Period::Custom { from, to } => (from, to.map(|t| t + DAY - 1)),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    pub author: String,
    pub path: String,
    pub period: Period,
    pub mine: Option<String>,
}

impl Filter {
    pub fn conditions(&self) -> usize {
        [
            !self.author.trim().is_empty(),
            !self.path.trim().is_empty(),
            self.period != Period::Any,
            self.mine.is_some(),
        ]
        .into_iter()
        .filter(|&on| on)
        .count()
    }

    pub fn is_empty(&self) -> bool {
        self.conditions() == 0
    }

    pub fn keeps(&self, name: &str, email: &str, time: i64, now: i64) -> bool {
        let author = self.author.trim().to_lowercase();
        if !author.is_empty()
            && !name.to_lowercase().contains(&author)
            && !email.to_lowercase().contains(&author)
        {
            return false;
        }
        if let Some(mine) = &self.mine
            && !email.eq_ignore_ascii_case(mine)
        {
            return false;
        }
        let (from, to) = self.period.bounds(now);
        from.is_none_or(|from| time >= from) && to.is_none_or(|to| time <= to)
    }
}

pub fn matching_rows(
    dir: &Path,
    ids: &[ObjectId],
    filter: &Filter,
    now: i64,
    cancelled: &dyn Fn() -> bool,
) -> anyhow::Result<Option<Vec<bool>>> {
    let repo = gix::discover(dir)?;
    let touching = match filter.path.trim() {
        "" => None,
        path => Some(path_commits(dir, path)?),
    };
    let needs_commit = filter.conditions() > usize::from(touching.is_some());
    let mut rows = Vec::with_capacity(ids.len());
    for (i, id) in ids.iter().enumerate() {
        if i % 512 == 0 && cancelled() {
            return Ok(None);
        }
        if touching.as_ref().is_some_and(|set| !set.contains(id)) {
            rows.push(false);
            continue;
        }
        if !needs_commit {
            rows.push(true);
            continue;
        }
        let keep = repo.find_commit(*id).ok().and_then(|commit| {
            let author = commit.author().ok()?;
            let time = author.time().ok()?.seconds;
            Some(filter.keeps(
                &author.name.to_string(),
                &author.email.to_string(),
                time,
                now,
            ))
        });
        rows.push(keep.unwrap_or(false));
    }
    Ok(Some(rows))
}

pub fn path_commits(dir: &Path, path: &str) -> anyhow::Result<HashSet<ObjectId>> {
    let out = git_cli::run(
        dir,
        &["log", "--all", "--full-history", "--format=%H", "--", path],
    )?;
    Ok(out
        .lines()
        .filter_map(|line| ObjectId::from_hex(line.trim().as_bytes()).ok())
        .collect())
}

pub fn authors(dir: &Path, ids: &[ObjectId]) -> anyhow::Result<Vec<String>> {
    let repo = gix::discover(dir)?;
    let mut counts: HashMap<String, usize> = HashMap::new();
    for id in ids {
        if let Ok(commit) = repo.find_commit(*id)
            && let Ok(author) = commit.author()
        {
            *counts.entry(author.name.to_string()).or_default() += 1;
        }
    }
    let mut names: Vec<(String, usize)> = counts.into_iter().collect();
    names.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(names.into_iter().map(|(name, _)| name).collect())
}

pub fn user_email(dir: &Path) -> Option<String> {
    git_cli::run(dir, &["config", "user.email"])
        .ok()
        .map(|out| out.trim().to_string())
        .filter(|email| !email.is_empty())
}

pub fn parse_day(text: &str) -> Option<i64> {
    let mut parts = text.trim().splitn(3, '-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * DAY)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_790_000_000;

    #[test]
    fn author_matches_name_or_email_ignoring_case() {
        let filter = Filter {
            author: "maya".into(),
            ..Filter::default()
        };
        assert!(filter.keeps("Maya Lindqvist", "m@example.com", NOW, NOW));
        assert!(filter.keeps("M. L.", "MAYA@example.com", NOW, NOW));
        assert!(!filter.keeps("Sam", "sam@example.com", NOW, NOW));
    }

    #[test]
    fn only_mine_compares_the_whole_email() {
        let filter = Filter {
            mine: Some("me@example.com".into()),
            ..Filter::default()
        };
        assert!(filter.keeps("Me", "ME@example.com", NOW, NOW));
        assert!(!filter.keeps("Me", "me@example.com.au", NOW, NOW));
    }

    #[test]
    fn periods_bound_the_commit_time() {
        let week = Filter {
            period: Period::Week,
            ..Filter::default()
        };
        assert!(week.keeps("a", "a", NOW - 6 * DAY, NOW));
        assert!(!week.keeps("a", "a", NOW - 8 * DAY, NOW));
        let custom = Filter {
            period: Period::Custom {
                from: parse_day("2026-09-01"),
                to: parse_day("2026-09-10"),
            },
            ..Filter::default()
        };
        let sept_10_evening = parse_day("2026-09-10").unwrap() + 20 * 3600;
        assert!(custom.keeps("a", "a", sept_10_evening, NOW));
        assert!(!custom.keeps("a", "a", parse_day("2026-09-11").unwrap(), NOW));
        assert!(!custom.keeps("a", "a", parse_day("2026-08-31").unwrap(), NOW));
    }

    #[test]
    fn calendar_days_parse_to_utc_midnight() {
        assert_eq!(parse_day("1970-01-01"), Some(0));
        assert_eq!(parse_day("2000-03-01"), Some(951_868_800));
        assert_eq!(parse_day("2026-9-28"), Some(1_790_553_600));
        assert_eq!(parse_day("2026-13-01"), None);
        assert_eq!(parse_day("soon"), None);
    }

    #[test]
    fn conditions_are_counted() {
        let filter = Filter {
            author: "x".into(),
            path: "src/".into(),
            period: Period::Day,
            mine: None,
        };
        assert_eq!(filter.conditions(), 3);
        assert!(Filter::default().is_empty());
    }
}
