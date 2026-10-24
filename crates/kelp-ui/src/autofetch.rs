//! The timer behind background fetching: `AutoFetch` says when a fetch is due and
//! [`AutoFetch::run`] starts it on a [`Jobs`] runner, so Kelp and the apps that embed it fetch on
//! the same schedule.

use std::path::Path;
use std::time::{Duration, Instant};

use kelp_core::ops::Op;

use crate::jobs::Jobs;

pub const LABEL: &str = "Auto-fetching";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    Now,
    In(Duration),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    Wait(Duration),
    Skip,
    Fetch,
}

#[derive(Debug, Clone, Copy)]
pub struct AutoFetch {
    every: Duration,
    last: Instant,
}

impl AutoFetch {
    #[must_use]
    pub fn new(every: Duration, last: Instant) -> Self {
        Self { every, last }
    }

    #[must_use]
    pub fn every(&self) -> Duration {
        self.every
    }

    pub fn set_every(&mut self, every: Duration) {
        self.every = every;
    }

    #[must_use]
    pub fn due(&self, now: Instant) -> Due {
        let due = self.last + self.every;
        if now < due {
            Due::In(due - now)
        } else {
            Due::Now
        }
    }

    /// Moves the clock when a fetch is due. A due fetch that finds one still running is skipped
    /// and tried again one interval later.
    pub fn tick(&mut self, now: Instant, busy: bool) -> Tick {
        match self.due(now) {
            Due::In(wait) => Tick::Wait(wait),
            Due::Now => {
                self.last = now;
                if busy { Tick::Skip } else { Tick::Fetch }
            }
        }
    }

    /// One look at the timer: starts `Op::Fetch` in `dir` on `jobs` when it is due and nothing is
    /// fetching, and asks `ctx` for the next wake. `wrap` turns the fetch result into the job
    /// output the host reads back from `Jobs::finished`.
    pub fn run<T: Send + 'static>(
        &mut self,
        ctx: &egui::Context,
        jobs: &mut Jobs<T>,
        dir: &Path,
        busy: bool,
        wrap: impl FnOnce(anyhow::Result<String>) -> T + Send + 'static,
    ) {
        let tick = self.tick(Instant::now(), busy);
        match tick {
            Tick::Wait(wait) => ctx.request_repaint_after(wait),
            Tick::Skip => ctx.request_repaint_after(self.every),
            Tick::Fetch => {
                ctx.request_repaint_after(self.every);
                let dir = dir.to_path_buf();
                jobs.spawn(LABEL, move || wrap(Op::Fetch.run(&dir)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    fn timer(start: Instant) -> AutoFetch {
        AutoFetch::new(5 * MINUTE, start)
    }

    #[test]
    fn nothing_is_due_before_one_interval_has_passed() {
        let start = Instant::now();
        let timer = timer(start);
        assert_eq!(timer.due(start), Due::In(5 * MINUTE));
        assert_eq!(timer.due(start + 2 * MINUTE), Due::In(3 * MINUTE));
        assert_eq!(
            timer.due(start + 5 * MINUTE - Duration::from_millis(1)),
            Due::In(Duration::from_millis(1))
        );
    }

    #[test]
    fn a_fetch_is_due_at_the_interval_and_after() {
        let start = Instant::now();
        let timer = timer(start);
        assert_eq!(timer.due(start + 5 * MINUTE), Due::Now);
        assert_eq!(timer.due(start + 9 * MINUTE), Due::Now);
    }

    #[test]
    fn a_due_tick_restarts_the_interval() {
        let start = Instant::now();
        let mut timer = timer(start);
        let at = start + 6 * MINUTE;
        assert_eq!(timer.tick(at, false), Tick::Fetch);
        assert_eq!(timer.tick(at + MINUTE, false), Tick::Wait(4 * MINUTE));
        assert_eq!(timer.tick(at + 5 * MINUTE, false), Tick::Fetch);
    }

    #[test]
    fn a_fetch_still_running_is_skipped_until_the_next_interval() {
        let start = Instant::now();
        let mut timer = timer(start);
        let at = start + 5 * MINUTE;
        assert_eq!(timer.tick(at, true), Tick::Skip);
        assert_eq!(timer.tick(at + MINUTE, false), Tick::Wait(4 * MINUTE));
        assert_eq!(timer.tick(at + 5 * MINUTE, false), Tick::Fetch);
    }

    #[test]
    fn a_new_interval_applies_to_the_time_already_waited() {
        let start = Instant::now();
        let mut timer = timer(start);
        timer.set_every(MINUTE);
        assert_eq!(timer.every(), MINUTE);
        assert_eq!(timer.due(start + 2 * MINUTE), Due::Now);
        timer.set_every(30 * MINUTE);
        assert_eq!(timer.due(start + 2 * MINUTE), Due::In(28 * MINUTE));
    }
}
