use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::avatar::GitHubRepo;

pub const BATCH: usize = 50;
const CONTEXTS_PER_COMMIT: usize = 30;
const SETTLED_FOR: i64 = 60 * 60;
const RUNNING_FOR: i64 = 60;
const NONE_FOR: i64 = 10 * 60;
const BACKOFF_FIRST: Duration = Duration::from_secs(60);
const BACKOFF_MAX: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Success,
    Failure,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub state: State,
    pub failing: Vec<String>,
    pub pending: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub status: Option<Status>,
    pub fetched: i64,
}

impl Entry {
    pub fn is_stale(&self, now: i64) -> bool {
        let ttl = match self.status.as_ref().map(|s| s.state) {
            Some(State::Pending) => RUNNING_FOR,
            Some(_) => SETTLED_FOR,
            None => NONE_FOR,
        };
        now - self.fetched >= ttl
    }
}

#[derive(Debug)]
pub enum FetchError {
    RateLimited,
    Other(anyhow::Error),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::RateLimited => write!(f, "GitHub rate limit reached"),
            FetchError::Other(e) => write!(f, "{e:#}"),
        }
    }
}

pub fn query(repo: &GitHubRepo, oids: &[String]) -> String {
    let node = "__typename ... on CheckRun { name conclusion status } \
                ... on StatusContext { context state }";
    let commits: Vec<String> = oids
        .iter()
        .enumerate()
        .map(|(i, oid)| {
            format!(
                "c{i}: object(oid: \"{oid}\") {{ ... on Commit {{ statusCheckRollup {{ state \
                 contexts(first: {CONTEXTS_PER_COMMIT}) {{ nodes {{ {node} }} }} }} }} }}"
            )
        })
        .collect();
    format!(
        "query {{ repository(owner: \"{}\", name: \"{}\") {{ {} }} }}",
        repo.owner,
        repo.name,
        commits.join(" ")
    )
}

pub fn parse(json: &str, oids: &[String]) -> Result<Vec<(String, Option<Status>)>, FetchError> {
    let value: Value = serde_json::from_str(json)
        .context("GitHub returned invalid JSON")
        .map_err(FetchError::Other)?;
    if let Some(errors) = value["errors"].as_array()
        && errors
            .iter()
            .any(|e| e["type"].as_str() == Some("RATE_LIMITED"))
    {
        return Err(FetchError::RateLimited);
    }
    let repository = &value["data"]["repository"];
    if repository.is_null() {
        let message = value["errors"][0]["message"].as_str().unwrap_or("no data");
        return Err(FetchError::Other(anyhow::anyhow!("{message}")));
    }
    Ok(oids
        .iter()
        .enumerate()
        .map(|(i, oid)| (oid.clone(), status(&repository[format!("c{i}")])))
        .collect())
}

fn status(commit: &Value) -> Option<Status> {
    let rollup = &commit["statusCheckRollup"];
    let state = match rollup["state"].as_str()? {
        "SUCCESS" => State::Success,
        "FAILURE" | "ERROR" => State::Failure,
        _ => State::Pending,
    };
    let mut failing = Vec::new();
    let mut pending = Vec::new();
    for node in rollup["contexts"]["nodes"].as_array().into_iter().flatten() {
        let (name, outcome) = match node["__typename"].as_str() {
            Some("CheckRun") => (
                node["name"].as_str(),
                node["conclusion"]
                    .as_str()
                    .or_else(|| node["status"].as_str()),
            ),
            _ => (node["context"].as_str(), node["state"].as_str()),
        };
        let Some(name) = name.map(str::to_string) else {
            continue;
        };
        match outcome {
            Some(
                "FAILURE" | "ERROR" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED"
                | "STARTUP_FAILURE",
            ) => failing.push(name),
            Some("SUCCESS" | "NEUTRAL" | "SKIPPED") => {}
            _ => pending.push(name),
        }
    }
    Some(Status {
        state,
        failing,
        pending,
    })
}

pub fn rate_limited(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("rate limit") || lower.contains("http 403") || lower.contains("http 429")
}

pub fn fetch(
    repo: &GitHubRepo,
    oids: &[String],
) -> Result<Vec<(String, Option<Status>)>, FetchError> {
    if std::env::var_os("KELP_OFFLINE").is_some() {
        return Err(FetchError::Other(anyhow::anyhow!("offline")));
    }
    let mut all = Vec::with_capacity(oids.len());
    for batch in oids.chunks(BATCH) {
        let text = query(repo, batch);
        let json = crate::pulls::run_gh(&["api", "graphql", "-f", &format!("query={text}")])
            .map_err(|e| {
                if rate_limited(&e.to_string()) {
                    FetchError::RateLimited
                } else {
                    FetchError::Other(e)
                }
            })?;
        all.extend(parse(&json, batch)?);
    }
    Ok(all)
}

pub fn checks_url(repo: &GitHubRepo, oid: &str) -> String {
    format!(
        "https://github.com/{}/{}/commit/{oid}/checks",
        repo.owner, repo.name
    )
}

pub fn cache_file(repo: &GitHubRepo) -> PathBuf {
    crate::avatar::cache_dir()
        .join("pulls")
        .join(format!("{}__{}.checks.json", repo.owner, repo.name))
}

pub fn load_cached(path: &Path) -> HashMap<String, Entry> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_cached(path: &Path, entries: &HashMap<String, Entry>) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec(entries) {
        let _ = std::fs::write(path, json);
    }
}

pub fn wanted<'a>(
    candidates: impl IntoIterator<Item = &'a str>,
    known: &HashMap<String, Entry>,
    now: i64,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for oid in candidates {
        let stale = known.get(oid).is_none_or(|entry| entry.is_stale(now));
        if stale && !out.iter().any(|o| o == oid) {
            out.push(oid.to_string());
            if out.len() == BATCH {
                break;
            }
        }
    }
    out
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    failures: u32,
}

impl Backoff {
    pub fn failed(&mut self) -> Duration {
        self.failures = self.failures.saturating_add(1);
        let factor = 2u32.saturating_pow(self.failures - 1);
        (BACKOFF_FIRST * factor).min(BACKOFF_MAX)
    }

    pub fn succeeded(&mut self) {
        self.failures = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> GitHubRepo {
        GitHubRepo {
            owner: "hobo-ware".into(),
            name: "kelp".into(),
        }
    }

    const RESPONSE: &str = r#"{"data": {"repository": {
      "c0": {"statusCheckRollup": {"state": "SUCCESS", "contexts": {"nodes": [
        {"__typename": "CheckRun", "name": "test", "conclusion": "SUCCESS", "status": "COMPLETED"}]}}},
      "c1": {"statusCheckRollup": {"state": "FAILURE", "contexts": {"nodes": [
        {"__typename": "CheckRun", "name": "lint", "conclusion": "FAILURE", "status": "COMPLETED"},
        {"__typename": "StatusContext", "context": "deploy", "state": "PENDING"},
        {"__typename": "CheckRun", "name": "e2e", "conclusion": null, "status": "IN_PROGRESS"}]}}},
      "c2": {"statusCheckRollup": {"state": "PENDING", "contexts": {"nodes": []}}},
      "c3": {"statusCheckRollup": null},
      "c4": null
    }}}"#;

    fn oids(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("{i:040}")).collect()
    }

    #[test]
    fn rollups_become_states_with_failing_and_pending_names() {
        let got = parse(RESPONSE, &oids(5)).unwrap();
        assert_eq!(got[0].1.as_ref().unwrap().state, State::Success);
        let failed = got[1].1.as_ref().unwrap();
        assert_eq!(failed.state, State::Failure);
        assert_eq!(failed.failing, ["lint"]);
        assert_eq!(failed.pending, ["deploy", "e2e"]);
        assert_eq!(got[2].1.as_ref().unwrap().state, State::Pending);
        assert_eq!(got[3].1, None);
        assert_eq!(got[4].1, None);
    }

    #[test]
    fn graphql_errors_are_reported() {
        let limited =
            r#"{"errors": [{"type": "RATE_LIMITED", "message": "API rate limit exceeded"}]}"#;
        assert!(matches!(
            parse(limited, &oids(1)),
            Err(FetchError::RateLimited)
        ));
        let broken =
            r#"{"data": {"repository": null}, "errors": [{"message": "Could not resolve"}]}"#;
        assert!(
            matches!(parse(broken, &oids(1)), Err(FetchError::Other(e)) if e.to_string().contains("resolve"))
        );
        assert!(rate_limited("HTTP 403: API rate limit exceeded for user"));
        assert!(rate_limited("gh: HTTP 429"));
        assert!(!rate_limited("HTTP 502: Bad Gateway"));
    }

    #[test]
    fn the_query_aliases_each_commit() {
        let text = query(&repo(), &["abc".into(), "def".into()]);
        assert!(text.contains("repository(owner: \"hobo-ware\", name: \"kelp\")"));
        assert!(text.contains("c0: object(oid: \"abc\")"));
        assert!(text.contains("c1: object(oid: \"def\")"));
    }

    #[test]
    fn entries_go_stale_by_state() {
        let entry = |state: Option<State>, fetched| Entry {
            status: state.map(|state| Status {
                state,
                failing: vec![],
                pending: vec![],
            }),
            fetched,
        };
        assert!(!entry(Some(State::Success), 1000).is_stale(1000 + 30 * 60));
        assert!(entry(Some(State::Success), 1000).is_stale(1000 + 60 * 60));
        assert!(!entry(Some(State::Pending), 1000).is_stale(1030));
        assert!(entry(Some(State::Pending), 1000).is_stale(1060));
        assert!(entry(None, 1000).is_stale(1000 + 10 * 60));
    }

    #[test]
    fn wanted_skips_fresh_dedupes_and_caps_at_a_batch() {
        let mut known = HashMap::new();
        known.insert(
            "fresh".to_string(),
            Entry {
                status: None,
                fetched: 100,
            },
        );
        let got = wanted(["fresh", "a", "a", "b"], &known, 120);
        assert_eq!(got, ["a", "b"]);
        let many: Vec<String> = (0..80).map(|i| i.to_string()).collect();
        assert_eq!(
            wanted(many.iter().map(String::as_str), &known, 0).len(),
            BATCH
        );
    }

    #[test]
    fn backoff_doubles_up_to_a_cap_and_resets() {
        let mut backoff = Backoff::default();
        assert_eq!(backoff.failed(), Duration::from_secs(60));
        assert_eq!(backoff.failed(), Duration::from_secs(120));
        for _ in 0..10 {
            backoff.failed();
        }
        assert_eq!(backoff.failed(), BACKOFF_MAX);
        backoff.succeeded();
        assert_eq!(backoff.failed(), Duration::from_secs(60));
    }

    #[test]
    fn the_cache_round_trips() {
        let path = std::env::temp_dir().join(format!("kelp-checks-{}.json", std::process::id()));
        let mut entries = HashMap::new();
        entries.insert(
            "abc".to_string(),
            Entry {
                status: Some(Status {
                    state: State::Failure,
                    failing: vec!["lint".into()],
                    pending: vec![],
                }),
                fetched: 5,
            },
        );
        save_cached(&path, &entries);
        assert_eq!(load_cached(&path), entries);
        let _ = std::fs::remove_file(&path);
    }
}
