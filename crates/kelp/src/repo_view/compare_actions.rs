use eframe::egui;
use gix::ObjectId;
use kelp_core::compare::{RangeChange, Side};

use super::{Center, Repo};
use crate::compare_view::{CompareView, Event};
use crate::diff_view::{DiffSource, DiffView};

impl Repo {
    pub fn start_compare(&mut self, ctx: &egui::Context, base: ObjectId, target: Side) {
        let (base, target) = match target {
            Side::Commit(other)
                if newer_first(self.history.row(&base), self.history.row(&other)) =>
            {
                (other, Side::Commit(base))
            }
            _ => (base, target),
        };
        self.compare_pick = None;
        self.compare = Some(CompareView::new(
            ctx,
            &self.repo,
            self.dir.clone(),
            base,
            target,
        ));
        self.close_range_diff();
    }

    pub fn stop_compare(&mut self) {
        self.compare = None;
        self.compare_pick = None;
        self.close_range_diff();
    }

    pub fn pick_compare(&mut self, rev: &str) {
        match self.resolve(rev) {
            Some(id) => self.compare_pick = Some(id),
            None => self.notify(format!("Could not find {rev}"), true),
        }
    }

    pub fn is_picking_compare(&self) -> bool {
        self.compare_pick.is_some()
    }

    pub fn compare_pick_hint(&self) -> Option<String> {
        self.compare_pick.map(|id| {
            format!(
                "Click a commit to compare with {} · Esc cancels",
                id.to_hex_with_len(7)
            )
        })
    }

    pub fn compare_revs(&mut self, ctx: &egui::Context, base: &str, target: Option<&str>) {
        let Some(base_id) = self.resolve(base) else {
            self.notify(format!("Could not find {base}"), true);
            return;
        };
        let target = match target {
            None => Side::WorkTree,
            Some(rev) => match self.resolve(rev) {
                Some(id) => Side::Commit(id),
                None => {
                    self.notify(format!("Could not find {rev}"), true);
                    return;
                }
            },
        };
        self.start_compare(ctx, base_id, target);
    }

    pub fn compare_from_spec(&mut self, ctx: &egui::Context, spec: &str) {
        let (base, target) = spec.split_once("..").unwrap_or((spec, "HEAD"));
        let target = (target != "worktree").then_some(target);
        self.compare_revs(ctx, base, target);
    }

    pub fn compare_with_row(&mut self, ctx: &egui::Context, row: usize) {
        let clicked = self.history.id(row);
        let base = self.compare_pick.take().or(match self.selected {
            Some(super::Selection::Commit(selected)) if selected != row => {
                Some(self.history.id(selected))
            }
            _ => None,
        });
        match base {
            Some(base) if base != clicked => self.start_compare(ctx, base, Side::Commit(clicked)),
            _ => self.select(super::Selection::Commit(row)),
        }
    }

    pub fn compare_event(&mut self, ctx: &egui::Context, event: Event) {
        match event {
            Event::None => {}
            Event::Close => self.stop_compare(),
            Event::Swap => {
                if let Some(view) = &self.compare
                    && let Side::Commit(target) = view.target
                {
                    let base = view.base;
                    self.compare = Some(CompareView::new(
                        ctx,
                        &self.repo,
                        self.dir.clone(),
                        target,
                        Side::Commit(base),
                    ));
                    self.close_range_diff();
                }
            }
            Event::Open(change) => self.open_range_diff(&change),
        }
    }

    pub fn range_diff_path(&self) -> Option<&str> {
        match &self.center {
            Center::Diff(view) if matches!(view.source, DiffSource::Range(..)) => Some(view.path()),
            _ => None,
        }
    }

    fn open_range_diff(&mut self, change: &RangeChange) {
        let Some(compare) = &self.compare else {
            return;
        };
        let source = DiffSource::Range(compare.base, compare.target_id());
        match DiffView::load_moved(
            &self.repo,
            self.workdir.as_deref(),
            source,
            &change.path,
            change.old_path.as_deref(),
        ) {
            Ok(view) => self.show_diff(view),
            Err(e) => self.notify(format!("Could not open {}: {e:#}", change.path), true),
        }
    }

    fn close_range_diff(&mut self) {
        if self.range_diff_path().is_some() {
            self.center = Center::Graph;
        }
    }

    fn resolve(&self, rev: &str) -> Option<ObjectId> {
        let object = self.repo.rev_parse_single(rev).ok()?.object().ok()?;
        Some(object.peel_to_commit().ok()?.id)
    }
}

fn newer_first(base_row: Option<usize>, target_row: Option<usize>) -> bool {
    matches!((base_row, target_row), (Some(base), Some(target)) if base < target)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use eframe::egui::{self, Key, Modifiers, Pos2, RawInput, Rect, vec2};
    use kelp_core::commit::ChangeKind;
    use kelp_core::compare::{RangeChange, Side};
    use kelp_core::git_cli::run;

    use super::newer_first;
    use crate::compare_view::Event;
    use crate::repo_view::{Center, Repo};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kelp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        run(&dir, &["init", "-q", "-b", "main"]).unwrap();
        run(&dir, &["config", "user.email", "t@example.com"]).unwrap();
        run(&dir, &["config", "user.name", "T"]).unwrap();
        for n in 0..3 {
            std::fs::write(dir.join("a.txt"), n.to_string()).unwrap();
            run(&dir, &["add", "."]).unwrap();
            run(&dir, &["commit", "-q", "-m", &format!("commit {n}")]).unwrap();
        }
        dir
    }

    fn press(ctx: &egui::Context, repo: &mut Repo, key: Key, modifiers: Modifiers) {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            modifiers,
            ..Default::default()
        };
        let _ = ctx.run_ui(input, |ui| repo.handle_keys(ui));
    }

    #[test]
    fn a_compare_orders_its_ends_and_esc_stops_it() {
        let dir = scratch("compare-keys");
        let ctx = egui::Context::default();
        let (git, history) = kelp_core::history::History::open(&dir).unwrap();
        let mut repo = Repo::new(&ctx, git, history, Duration::ZERO);
        let (newest, oldest) = (repo.history.id(0), repo.history.id(2));
        repo.compare_from_spec(&ctx, "HEAD..HEAD~2");
        let compare = repo.compare.as_ref().expect("comparing");
        assert_eq!(compare.base, oldest);
        assert_eq!(compare.target, Side::Commit(newest));

        press(&ctx, &mut repo, Key::Escape, Modifiers::NONE);
        assert!(repo.compare.is_none());

        repo.pick_compare("HEAD~1");
        repo.compare_with_row(&ctx, 0);
        assert_eq!(
            repo.compare.as_ref().map(|c| c.base),
            Some(repo.history.id(1))
        );

        let change = RangeChange {
            path: "a.txt".into(),
            old_path: None,
            kind: ChangeKind::Modified,
        };
        repo.compare_event(&ctx, Event::Open(change));
        assert_eq!(repo.range_diff_path(), Some("a.txt"));
        let Center::Diff(view) = &repo.center else {
            panic!("the range diff should be open");
        };
        assert_eq!((view.diff.added, view.diff.removed), (1, 1));
        repo.stop_compare();
        assert!(repo.range_diff_path().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cmd_shift_f_toggles_the_filter_bar_and_esc_closes_it() {
        let dir = scratch("filter-keys");
        let ctx = egui::Context::default();
        let (git, history) = kelp_core::history::History::open(&dir).unwrap();
        let mut repo = Repo::new(&ctx, git, history, Duration::ZERO);
        assert!(!repo.filter.open);
        press(
            &ctx,
            &mut repo,
            Key::F,
            Modifiers::COMMAND | Modifiers::SHIFT,
        );
        assert!(repo.filter.open);
        assert!(!repo.search.open, "Cmd+Shift+F is not search");
        press(&ctx, &mut repo, Key::Escape, Modifiers::NONE);
        assert!(!repo.filter.open);
        press(&ctx, &mut repo, Key::F, Modifiers::COMMAND);
        assert!(repo.search.open);
        assert!(!repo.filter.open);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_older_commit_becomes_a() {
        assert!(newer_first(Some(1), Some(5)));
        assert!(!newer_first(Some(5), Some(1)));
        assert!(!newer_first(None, Some(1)));
        assert!(!newer_first(Some(3), None));
    }
}
