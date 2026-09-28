use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::avatar::GitHubRepo;

pub const FRESH_FOR: Duration = Duration::from_secs(5 * 60);
const GH_ALL_LIMIT: &str = "200";
const GH_OPEN_LIMIT: &str = "100";
const GH_ALL_FIELDS: &str = "number,title,headRefName,headRepositoryOwner,state,isDraft,url";
const GH_OPEN_FIELDS: &str = "number,title,headRefName,headRepositoryOwner,state,isDraft,url,reviewDecision,statusCheckRollup";
const GH_TIMEOUT: Duration = Duration::from_secs(20);
const REST_PAGES_WITH_TOKEN: usize = 2;
const REST_PAGES_ANONYMOUS: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Open,
    Draft,
    Merged,
    Closed,
}

impl State {
    pub fn label(self) -> &'static str {
        match self {
            State::Open => "Open",
            State::Draft => "Draft",
            State::Merged => "Merged",
            State::Closed => "Closed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Checks {
    Passing,
    Failing,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pull {
    pub number: u64,
    pub title: String,
    pub head: String,
    pub head_owner: Option<String>,
    pub state: State,
    pub url: String,
    pub review: Option<String>,
    pub checks: Option<Checks>,
}

impl Pull {
    pub fn review_text(&self) -> Option<&'static str> {
        match self.review.as_deref()? {
            "APPROVED" => Some("Approved"),
            "CHANGES_REQUESTED" => Some("Changes requested"),
            "REVIEW_REQUIRED" => Some("Review required"),
            _ => None,
        }
    }

    pub fn checks_text(&self) -> Option<&'static str> {
        Some(match self.checks? {
            Checks::Passing => "Checks passing",
            Checks::Failing => "Checks failing",
            Checks::Pending => "Checks running",
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Pulls {
    owner: String,
    list: Vec<Pull>,
}

impl Pulls {
    pub fn new(repo: &GitHubRepo, list: Vec<Pull>) -> Self {
        Self {
            owner: repo.owner.clone(),
            list,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// The pull request whose head is `branch` in this repository, preferring
    /// an open one over the newest closed or merged one.
    pub fn for_branch(&self, branch: &str) -> Option<&Pull> {
        self.list
            .iter()
            .filter(|p| p.head == branch)
            .filter(|p| {
                p.head_owner
                    .as_deref()
                    .is_none_or(|owner| owner.eq_ignore_ascii_case(&self.owner))
            })
            .max_by_key(|p| (matches!(p.state, State::Open | State::Draft), p.number))
    }

    pub fn for_remote_branch(&self, remote_ref: &str) -> Option<&Pull> {
        let (_, branch) = remote_ref.split_once('/')?;
        self.for_branch(branch)
    }
}

pub fn parse_gh(json: &str) -> anyhow::Result<Vec<Pull>> {
    let items: Vec<Value> = serde_json::from_str(json).context("gh returned invalid JSON")?;
    Ok(items.iter().filter_map(gh_pull).collect())
}

fn gh_pull(item: &Value) -> Option<Pull> {
    let state = match (item["state"].as_str()?, item["isDraft"].as_bool()) {
        ("OPEN", Some(true)) => State::Draft,
        ("OPEN", _) => State::Open,
        ("MERGED", _) => State::Merged,
        _ => State::Closed,
    };
    Some(Pull {
        number: item["number"].as_u64()?,
        title: item["title"].as_str().unwrap_or_default().to_string(),
        head: item["headRefName"].as_str()?.to_string(),
        head_owner: item["headRepositoryOwner"]["login"]
            .as_str()
            .map(str::to_string),
        state,
        url: item["url"].as_str()?.to_string(),
        review: item["reviewDecision"]
            .as_str()
            .filter(|r| !r.is_empty())
            .map(str::to_string),
        checks: rollup(&item["statusCheckRollup"]),
    })
}

fn rollup(checks: &Value) -> Option<Checks> {
    let checks = checks.as_array().filter(|c| !c.is_empty())?;
    let mut pending = false;
    for check in checks {
        let outcome = check["conclusion"]
            .as_str()
            .filter(|c| !c.is_empty())
            .or_else(|| check["state"].as_str());
        match outcome {
            Some(
                "FAILURE" | "ERROR" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED"
                | "STARTUP_FAILURE",
            ) => return Some(Checks::Failing),
            Some("SUCCESS" | "NEUTRAL" | "SKIPPED") => {}
            _ => pending = true,
        }
    }
    Some(if pending {
        Checks::Pending
    } else {
        Checks::Passing
    })
}

pub fn parse_rest(json: &str) -> anyhow::Result<Vec<Pull>> {
    let items: Vec<Value> = serde_json::from_str(json).context("GitHub returned invalid JSON")?;
    Ok(items.iter().filter_map(rest_pull).collect())
}

fn rest_pull(item: &Value) -> Option<Pull> {
    let state = match (
        item["state"].as_str()?,
        item["draft"].as_bool(),
        item["merged_at"].is_string(),
    ) {
        ("open", Some(true), _) => State::Draft,
        ("open", _, _) => State::Open,
        (_, _, true) => State::Merged,
        _ => State::Closed,
    };
    Some(Pull {
        number: item["number"].as_u64()?,
        title: item["title"].as_str().unwrap_or_default().to_string(),
        head: item["head"]["ref"].as_str()?.to_string(),
        head_owner: item["head"]["repo"]["owner"]["login"]
            .as_str()
            .map(str::to_string),
        state,
        url: item["html_url"].as_str()?.to_string(),
        review: None,
        checks: None,
    })
}

pub fn cache_file(repo: &GitHubRepo) -> PathBuf {
    crate::avatar::cache_dir()
        .join("pulls")
        .join(format!("{}__{}.json", repo.owner, repo.name))
}

pub fn load_cached(path: &Path) -> Option<(Vec<Pull>, bool)> {
    let bytes = std::fs::read(path).ok()?;
    let list = serde_json::from_slice(&bytes).ok()?;
    let fresh = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < FRESH_FOR);
    Some((list, fresh))
}

pub fn save_cached(path: &Path, list: &[Pull]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec(list) {
        let _ = std::fs::write(path, json);
    }
}

pub fn fetch(repo: &GitHubRepo) -> anyhow::Result<Vec<Pull>> {
    if std::env::var_os("KELP_OFFLINE").is_some() {
        bail!("offline");
    }
    match fetch_with_gh(repo) {
        Ok(list) => Ok(list),
        Err(_) => fetch_with_rest(repo, crate::avatar::gh_token()),
    }
}

fn fetch_with_gh(repo: &GitHubRepo) -> anyhow::Result<Vec<Pull>> {
    let all = parse_gh(&gh_pr_list(repo, "all", GH_ALL_LIMIT, GH_ALL_FIELDS)?)?;
    let open = parse_gh(&gh_pr_list(repo, "open", GH_OPEN_LIMIT, GH_OPEN_FIELDS)?)?;
    Ok(with_open_details(all, open))
}

fn with_open_details(mut all: Vec<Pull>, open: Vec<Pull>) -> Vec<Pull> {
    for detailed in open {
        match all.iter_mut().find(|p| p.number == detailed.number) {
            Some(slot) => *slot = detailed,
            None => all.push(detailed),
        }
    }
    all
}

fn gh_pr_list(repo: &GitHubRepo, state: &str, limit: &str, fields: &str) -> anyhow::Result<String> {
    let slug = format!("{}/{}", repo.owner, repo.name);
    run_gh(&[
        "pr", "list", "--repo", &slug, "--state", state, "--limit", limit, "--json", fields,
    ])
}

/// Runs `gh` with its output drained while it runs, so answers larger than
/// the pipe buffer never stall it, and gives up after a timeout.
pub fn run_gh(args: &[&str]) -> anyhow::Result<String> {
    use std::process::Stdio;
    let log = crate::console::start("gh", args, Path::new(""));
    let mut child = Command::new("gh")
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("gh is not installed")?;
    let drain = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = drain(child.stdout.take().map(|p| Box::new(p) as _));
    let stderr = drain(child.stderr.take().map(|p| Box::new(p) as _));
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > GH_TIMEOUT {
            let _ = child.kill();
            log.finish(None, b"", b"timed out");
            bail!(
                "gh {} took longer than {}s",
                args.first().unwrap_or(&""),
                GH_TIMEOUT.as_secs()
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    log.finish(status.code(), &stdout, &stderr);
    if !status.success() {
        let out = String::from_utf8_lossy(&stdout);
        let err = String::from_utf8_lossy(&stderr);
        bail!("{} {}", err.trim(), out.trim());
    }
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}

fn fetch_with_rest(repo: &GitHubRepo, token: Option<String>) -> anyhow::Result<Vec<Pull>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .user_agent("kelp-git-client")
        .build()
        .into();
    let pages = if token.is_some() {
        REST_PAGES_WITH_TOKEN
    } else {
        REST_PAGES_ANONYMOUS
    };
    let mut list = Vec::new();
    for page in 1..=pages {
        let url = format!(
            "https://api.github.com/repos/{}/{}/pulls?state=all&per_page=100&sort=updated&direction=desc&page={page}",
            repo.owner, repo.name
        );
        let mut request = agent
            .get(&url)
            .header("Accept", "application/vnd.github+json");
        if let Some(token) = &token {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }
        let body = request.call()?.body_mut().read_to_string()?;
        let batch = parse_rest(&body)?;
        let done = batch.len() < 100;
        list.extend(batch);
        if done {
            break;
        }
    }
    Ok(list)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListTab {
    Open,
    Mine,
    ReviewRequested,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullSummary {
    pub pull: Pull,
    pub author: String,
    pub updated: Option<i64>,
}

const GH_LIST_FIELDS: &str = "number,title,author,headRefName,headRepositoryOwner,state,isDraft,url,reviewDecision,statusCheckRollup,updatedAt";
const GH_LIST_LIMIT: &str = "100";

pub fn list_open(repo: &GitHubRepo, tab: ListTab) -> anyhow::Result<Vec<PullSummary>> {
    if std::env::var_os("KELP_OFFLINE").is_some() {
        bail!("offline");
    }
    let slug = format!("{}/{}", repo.owner, repo.name);
    let mut args = vec![
        "pr",
        "list",
        "--repo",
        &slug,
        "--state",
        "open",
        "--limit",
        GH_LIST_LIMIT,
        "--json",
        GH_LIST_FIELDS,
    ];
    match tab {
        ListTab::Open => {}
        ListTab::Mine => args.extend(["--search", "author:@me"]),
        ListTab::ReviewRequested => args.extend(["--search", "review-requested:@me"]),
    }
    parse_list(&run_gh(&args)?)
}

pub fn parse_list(json: &str) -> anyhow::Result<Vec<PullSummary>> {
    let items: Vec<Value> = serde_json::from_str(json).context("gh returned invalid JSON")?;
    Ok(items
        .iter()
        .filter_map(|item| {
            Some(PullSummary {
                pull: gh_pull(item)?,
                author: item["author"]["login"]
                    .as_str()
                    .unwrap_or("ghost")
                    .to_string(),
                updated: item["updatedAt"].as_str().and_then(iso_to_unix),
            })
        })
        .collect())
}

pub fn iso_to_unix(text: &str) -> Option<i64> {
    let (date, time) = text.trim_end_matches('Z').split_once('T')?;
    let mut d = date.splitn(3, '-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time
        .splitn(3, ':')
        .map(|p| p.split('.').next()?.parse::<i64>().ok());
    let (h, min, sec) = (t.next()??, t.next()??, t.next()??);
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + min * 60 + sec)
}

pub fn compare_url(repo: &GitHubRepo, base: &str, branch: &str) -> String {
    format!(
        "https://github.com/{}/{}/compare/{base}...{branch}?expand=1",
        repo.owner, repo.name
    )
}

pub fn gh_available() -> bool {
    Command::new("gh")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GH: &str = r#"[
      {"number": 12, "title": "Add export", "headRefName": "feat/export",
       "headRepositoryOwner": {"login": "hobo-ware"}, "state": "OPEN", "isDraft": false,
       "url": "https://github.com/Hobo-Ware/kelp/pull/12", "reviewDecision": "APPROVED",
       "statusCheckRollup": [
         {"__typename": "CheckRun", "status": "COMPLETED", "conclusion": "SUCCESS"},
         {"__typename": "StatusContext", "state": "SUCCESS"}
       ]},
      {"number": 11, "title": "WIP drag", "headRefName": "fix/drag",
       "headRepositoryOwner": {"login": "Hobo-Ware"}, "state": "OPEN", "isDraft": true,
       "url": "https://github.com/Hobo-Ware/kelp/pull/11", "reviewDecision": "",
       "statusCheckRollup": [
         {"__typename": "CheckRun", "status": "IN_PROGRESS", "conclusion": ""}
       ]},
      {"number": 9, "title": "Old export", "headRefName": "feat/export",
       "headRepositoryOwner": {"login": "Hobo-Ware"}, "state": "MERGED", "isDraft": false,
       "url": "https://github.com/Hobo-Ware/kelp/pull/9", "reviewDecision": null,
       "statusCheckRollup": [
         {"__typename": "CheckRun", "status": "COMPLETED", "conclusion": "FAILURE"}
       ]},
      {"number": 8, "title": "Fork", "headRefName": "main",
       "headRepositoryOwner": {"login": "someone"}, "state": "CLOSED", "isDraft": false,
       "url": "https://github.com/Hobo-Ware/kelp/pull/8", "reviewDecision": null,
       "statusCheckRollup": []}
    ]"#;

    const REST: &str = r#"[
      {"number": 5, "title": "Open one", "state": "open", "draft": false, "merged_at": null,
       "html_url": "https://github.com/o/r/pull/5",
       "head": {"ref": "feat/a", "repo": {"owner": {"login": "o"}}}},
      {"number": 4, "title": "Draft one", "state": "open", "draft": true, "merged_at": null,
       "html_url": "https://github.com/o/r/pull/4",
       "head": {"ref": "feat/b", "repo": {"owner": {"login": "o"}}}},
      {"number": 3, "title": "Merged", "state": "closed", "draft": false,
       "merged_at": "2026-09-01T00:00:00Z", "html_url": "https://github.com/o/r/pull/3",
       "head": {"ref": "feat/c", "repo": null}},
      {"number": 2, "title": "Closed", "state": "closed", "draft": false, "merged_at": null,
       "html_url": "https://github.com/o/r/pull/2",
       "head": {"ref": "feat/d", "repo": {"owner": {"login": "fork"}}}}
    ]"#;

    fn repo() -> GitHubRepo {
        GitHubRepo {
            owner: "Hobo-Ware".into(),
            name: "kelp".into(),
        }
    }

    #[test]
    fn gh_states_reviews_and_checks() {
        let list = parse_gh(GH).unwrap();
        assert_eq!(list.len(), 4);
        assert_eq!(list[0].state, State::Open);
        assert_eq!(list[0].checks, Some(Checks::Passing));
        assert_eq!(list[0].review_text(), Some("Approved"));
        assert_eq!(list[1].state, State::Draft);
        assert_eq!(list[1].checks, Some(Checks::Pending));
        assert_eq!(list[1].review, None);
        assert_eq!(list[2].state, State::Merged);
        assert_eq!(list[2].checks, Some(Checks::Failing));
        assert_eq!(list[3].state, State::Closed);
        assert_eq!(list[3].checks, None);
    }

    #[test]
    fn rest_states_and_missing_repo() {
        let list = parse_rest(REST).unwrap();
        let states: Vec<State> = list.iter().map(|p| p.state).collect();
        assert_eq!(
            states,
            [State::Open, State::Draft, State::Merged, State::Closed]
        );
        assert_eq!(list[2].head_owner, None);
        assert!(list.iter().all(|p| p.checks.is_none()));
    }

    #[test]
    fn branches_prefer_open_and_skip_forks() {
        let pulls = Pulls::new(&repo(), parse_gh(GH).unwrap());
        assert_eq!(pulls.for_branch("feat/export").map(|p| p.number), Some(12));
        assert_eq!(pulls.for_branch("fix/drag").map(|p| p.number), Some(11));
        assert_eq!(pulls.for_branch("main"), None, "fork heads never match");
        assert_eq!(
            pulls
                .for_remote_branch("origin/feat/export")
                .map(|p| p.number),
            Some(12)
        );
        assert_eq!(pulls.for_branch("nope"), None);
    }

    #[test]
    fn closed_only_branches_get_the_newest() {
        let list = vec![
            Pull {
                number: 3,
                state: State::Closed,
                ..parse_gh(GH).unwrap()[3].clone()
            },
            Pull {
                number: 7,
                head_owner: None,
                state: State::Merged,
                ..parse_gh(GH).unwrap()[3].clone()
            },
        ];
        let pulls = Pulls::new(&repo(), list);
        assert_eq!(pulls.for_branch("main").map(|p| p.number), Some(7));
    }

    #[test]
    fn cache_round_trip_and_freshness() {
        let dir = std::env::temp_dir().join(format!("kelp-pulls-{}", std::process::id()));
        let path = dir.join("pulls.json");
        let list = parse_gh(GH).unwrap();
        save_cached(&path, &list);
        let (loaded, fresh) = load_cached(&path).unwrap();
        assert_eq!(loaded, list);
        assert!(fresh);
        let old = SystemTime::now() - FRESH_FOR - Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        assert!(!load_cached(&path).unwrap().1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn compare_links_point_at_the_branch() {
        assert_eq!(
            compare_url(&repo(), "main", "feat/x"),
            "https://github.com/Hobo-Ware/kelp/compare/main...feat/x?expand=1"
        );
    }

    #[test]
    fn open_details_replace_the_light_rows() {
        let light = parse_gh(GH).unwrap();
        let mut detailed = light[0].clone();
        detailed.review = Some("APPROVED".into());
        let extra = Pull {
            number: 999,
            ..detailed.clone()
        };
        let merged = with_open_details(light.clone(), vec![detailed, extra]);
        assert_eq!(merged.len(), light.len() + 1);
        assert_eq!(merged[0].review.as_deref(), Some("APPROVED"));
        assert!(merged.iter().any(|p| p.number == 999));
    }

    #[test]
    fn iso_dates_become_unix_seconds() {
        assert_eq!(iso_to_unix("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(iso_to_unix("2026-09-28T12:30:05Z"), Some(1_790_598_605));
        assert_eq!(iso_to_unix("2000-02-29T00:00:00.123Z"), Some(951_782_400));
        assert_eq!(iso_to_unix("nope"), None);
    }

    #[test]
    fn the_open_list_keeps_author_and_update_time() {
        let json = r#"[{"number": 7, "title": "Try it", "author": {"login": "maya"},
            "headRefName": "feat/x", "headRepositoryOwner": {"login": "hobo-ware"},
            "state": "OPEN", "isDraft": true, "url": "https://github.com/o/r/pull/7",
            "reviewDecision": "REVIEW_REQUIRED", "statusCheckRollup": [],
            "updatedAt": "2026-09-28T12:30:05Z"}]"#;
        let list = parse_list(json).unwrap();
        assert_eq!(list[0].author, "maya");
        assert_eq!(list[0].pull.state, State::Draft);
        assert_eq!(list[0].updated, Some(1_790_598_605));
        assert_eq!(list[0].pull.review_text(), Some("Review required"));
    }
}
