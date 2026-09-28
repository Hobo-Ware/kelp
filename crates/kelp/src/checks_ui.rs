use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use kelp_core::avatar::GitHubRepo;
use kelp_core::checks::{self, Backoff, Entry, FetchError, State, Status};

const PENDING_RECHECK: Duration = Duration::from_secs(60);

pub type Fetched = Result<Vec<(String, Option<Status>)>, (bool, String)>;

#[derive(Default)]
pub struct ChecksState {
    known: HashMap<String, Entry>,
    pub statuses: HashMap<gix::ObjectId, Status>,
    cache: Option<PathBuf>,
    fake: Option<HashMap<String, Status>>,
    in_flight: bool,
    retry_at: Option<Instant>,
    backoff: Backoff,
}

impl ChecksState {
    pub fn new(github: Option<&GitHubRepo>) -> Self {
        let fake = std::env::var_os("KELP_FAKE_CHECKS")
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        let cache = github.map(checks::cache_file);
        let known = match (&fake, &cache) {
            (None, Some(path)) => checks::load_cached(path),
            _ => HashMap::new(),
        };
        let mut state = Self {
            known,
            cache,
            fake,
            ..Self::default()
        };
        state.rebuild_statuses();
        state
    }

    pub fn next_batch(&mut self, visible: &[gix::ObjectId], now: i64) -> Option<Vec<String>> {
        if let Some(fake) = &self.fake {
            for id in visible {
                let hex = id.to_string();
                if let Some((_, status)) = fake.iter().find(|(prefix, _)| hex.starts_with(*prefix))
                {
                    self.statuses.insert(*id, status.clone());
                }
            }
            return None;
        }
        if self.cache.is_none() || self.in_flight || std::env::var_os("KELP_OFFLINE").is_some() {
            return None;
        }
        if self.retry_at.is_some_and(|at| Instant::now() < at) {
            return None;
        }
        let hex: Vec<String> = visible.iter().map(|id| id.to_string()).collect();
        let batch = checks::wanted(hex.iter().map(String::as_str), &self.known, now);
        if batch.is_empty() {
            return None;
        }
        self.in_flight = true;
        Some(batch)
    }

    pub fn fetch(github: &GitHubRepo, batch: &[String]) -> Fetched {
        checks::fetch(github, batch)
            .map_err(|e| (matches!(e, FetchError::RateLimited), e.to_string()))
    }

    pub fn apply(&mut self, fetched: Fetched, now: i64) -> Option<String> {
        self.in_flight = false;
        match fetched {
            Ok(found) => {
                self.backoff.succeeded();
                self.retry_at = None;
                for (oid, status) in found {
                    self.known.insert(
                        oid,
                        Entry {
                            status,
                            fetched: now,
                        },
                    );
                }
                if let Some(path) = &self.cache {
                    checks::save_cached(path, &self.known);
                }
                self.rebuild_statuses();
                None
            }
            Err((limited, message)) => {
                let wait = self.backoff.failed();
                self.retry_at = Some(Instant::now() + wait);
                limited.then(|| format!("{message}; CI status paused for {}s", wait.as_secs()))
            }
        }
    }

    pub fn recheck_after(&self, visible: &[gix::ObjectId]) -> Option<Duration> {
        visible
            .iter()
            .any(|id| {
                self.statuses
                    .get(id)
                    .is_some_and(|s| s.state == State::Pending)
            })
            .then_some(PENDING_RECHECK)
    }

    fn rebuild_statuses(&mut self) {
        self.statuses = self
            .known
            .iter()
            .filter_map(|(oid, entry)| {
                let status = entry.status.clone()?;
                Some((gix::ObjectId::from_hex(oid.as_bytes()).ok()?, status))
            })
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(n: u8) -> gix::ObjectId {
        gix::ObjectId::from_hex(format!("{n:040x}").as_bytes()).unwrap()
    }

    fn state_with_cache() -> ChecksState {
        ChecksState {
            cache: Some(
                std::env::temp_dir().join(format!("kelp-checks-ui-{}.json", std::process::id())),
            ),
            ..ChecksState::default()
        }
    }

    #[test]
    fn a_batch_is_not_requested_twice_while_one_is_running() {
        let mut state = state_with_cache();
        let visible = [oid(1), oid(2)];
        assert_eq!(state.next_batch(&visible, 100).map(|b| b.len()), Some(2));
        assert_eq!(state.next_batch(&visible, 100), None);
        let found = vec![
            (
                oid(1).to_string(),
                Some(Status {
                    state: State::Failure,
                    failing: vec!["lint".into()],
                    pending: vec![],
                }),
            ),
            (oid(2).to_string(), None),
        ];
        assert_eq!(state.apply(Ok(found), 100), None);
        assert_eq!(state.statuses[&oid(1)].state, State::Failure);
        assert_eq!(state.next_batch(&visible, 101), None);
        if let Some(path) = &state.cache {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn a_rate_limit_pauses_further_requests() {
        let mut state = state_with_cache();
        let visible = [oid(3)];
        assert!(state.next_batch(&visible, 0).is_some());
        let note = state.apply(Err((true, "GitHub rate limit reached".into())), 0);
        assert!(note.unwrap().contains("paused for 60s"));
        assert_eq!(state.next_batch(&visible, 0), None);
    }

    #[test]
    fn running_checks_ask_for_a_recheck() {
        let mut state = ChecksState::default();
        state.statuses.insert(
            oid(4),
            Status {
                state: State::Pending,
                failing: vec![],
                pending: vec!["e2e".into()],
            },
        );
        assert_eq!(state.recheck_after(&[oid(4)]), Some(PENDING_RECHECK));
        assert_eq!(state.recheck_after(&[oid(5)]), None);
    }
}
