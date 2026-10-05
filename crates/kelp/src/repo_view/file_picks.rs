use std::collections::HashMap;

use kelp_core::commit::FileChange;

use super::{Center, Repo, Selection};
use crate::details;
use crate::diff_view::DiffView;
use crate::multi_diff_view::MultiDiff;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickHow {
    Only,
    Toggle,
    Range,
}

#[derive(Default)]
pub struct FilePicks {
    set: Vec<String>,
    anchor: Option<String>,
    cursor: Option<String>,
}

impl FilePicks {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn shown(&self, multi: bool, open: Option<String>) -> Vec<String> {
        if multi {
            self.set.clone()
        } else {
            open.into_iter().collect()
        }
    }

    pub fn cursor(&self, multi: bool) -> Option<String> {
        self.cursor.clone().filter(|_| multi)
    }
}

impl Repo {
    fn changed_files(&self) -> &[FileChange] {
        self.details
            .as_ref()
            .map_or(&[][..], |d| d.changes.as_slice())
    }

    fn open_path(&self) -> Option<String> {
        match &self.center {
            Center::Diff(view) => Some(view.path().to_string()),
            _ => None,
        }
    }

    pub fn pick_file(&mut self, path: &str, how: PickHow) {
        let order = details::display_order(self.changed_files(), self.file_list_mode);
        let index_of = |p: &str| order.iter().position(|o| o == p);
        match how {
            PickHow::Only => {
                self.picks.set = vec![path.to_string()];
                self.picks.anchor = Some(path.to_string());
            }
            PickHow::Toggle => {
                if !matches!(self.center, Center::Multi(_)) {
                    self.picks.set = self.open_path().into_iter().collect();
                }
                match self.picks.set.iter().position(|p| p == path) {
                    Some(i) => {
                        self.picks.set.remove(i);
                    }
                    None => self.picks.set.push(path.to_string()),
                }
                self.picks.set.sort_by_key(|p| index_of(p));
                self.picks.anchor = Some(path.to_string());
            }
            PickHow::Range => {
                let anchor = if matches!(self.center, Center::Multi(_)) {
                    self.picks.anchor.clone()
                } else {
                    self.open_path()
                }
                .unwrap_or_else(|| path.to_string());
                match (index_of(&anchor), index_of(path)) {
                    (Some(a), Some(b)) => {
                        self.picks.set = order[a.min(b)..=a.max(b)].to_vec();
                    }
                    _ => self.picks.set = vec![path.to_string()],
                }
                self.picks.anchor = Some(anchor);
            }
        }
        self.picks.cursor = Some(path.to_string());
        self.apply_picks();
    }

    fn apply_picks(&mut self) {
        match self.picks.set.len() {
            0 => self.center = Center::Graph,
            1 => {
                let path = self.picks.set[0].clone();
                self.open_diff(&path);
            }
            _ => {
                let paths = self.picks.set.clone();
                self.open_files(&paths);
            }
        }
    }

    fn loaded_views(&mut self) -> HashMap<String, DiffView> {
        if !matches!(self.selected, Some(Selection::Commit(_))) {
            return HashMap::new();
        }
        match std::mem::replace(&mut self.center, Center::Graph) {
            Center::Multi(multi) => multi
                .views
                .into_iter()
                .map(|view| (view.path().to_string(), view))
                .collect(),
            Center::Diff(view) => HashMap::from([(view.path().to_string(), *view)]),
            other => {
                self.center = other;
                HashMap::new()
            }
        }
    }

    fn open_files(&mut self, paths: &[String]) {
        let mut reusable = self.loaded_views();
        let mut views = Vec::new();
        let mut loaded = Vec::new();
        for path in paths {
            if let Some(view) = reusable.remove(path) {
                views.push(view);
                loaded.push(path.clone());
                continue;
            }
            let Some(source) = self.diff_source_for(path) else {
                continue;
            };
            match DiffView::load(&self.repo, self.workdir.as_deref(), source, path) {
                Ok(mut view) => {
                    view.set_layout(self.diff_layout);
                    views.push(view);
                    loaded.push(path.clone());
                }
                Err(e) => self.notify(format!("Could not open {path}: {e:#}"), true),
            }
        }
        let first = loaded.first().cloned();
        self.picks.set = loaded;
        match views.len() {
            0 => {}
            1 => {
                if let Some(path) = first {
                    self.open_diff(&path);
                }
            }
            _ => {
                let views = views.into_iter().map(DiffView::embedded).collect();
                self.center = Center::Multi(Box::new(MultiDiff::new(views)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use eframe::egui;
    use kelp_core::git_cli::run;

    use super::PickHow;
    use crate::repo_view::{Center, Repo, Selection};

    fn picked(repo: &Repo) -> Vec<&str> {
        repo.picks.set.iter().map(String::as_str).collect()
    }

    #[test]
    fn cmd_and_shift_picks_stack_files_in_list_order() {
        let dir = std::env::temp_dir().join(format!("kelp-file-picks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.email", "t@example.com"],
            &["config", "user.name", "T"],
        ] {
            run(&dir, args).unwrap();
        }
        for name in ["a.txt", "b.txt", "c.txt", "d.txt"] {
            std::fs::write(dir.join(name), "one\n").unwrap();
        }
        run(&dir, &["add", "."]).unwrap();
        run(&dir, &["commit", "-q", "-m", "four files"]).unwrap();

        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let (git, history) = kelp_core::history::History::open(&dir).unwrap();
        let mut repo = Repo::new(&ctx, git, history, Duration::ZERO);
        repo.select(Selection::Commit(0));
        let started = Instant::now();
        while repo.details.is_none() && started.elapsed() < Duration::from_secs(5) {
            repo.poll(&ctx, None);
            std::thread::sleep(Duration::from_millis(20));
        }

        repo.pick_file("b.txt", PickHow::Only);
        assert!(matches!(repo.center, Center::Diff(_)));
        repo.pick_file("d.txt", PickHow::Toggle);
        repo.pick_file("a.txt", PickHow::Toggle);
        assert!(matches!(repo.center, Center::Multi(_)));
        assert_eq!(picked(&repo), ["a.txt", "b.txt", "d.txt"]);

        repo.pick_file("b.txt", PickHow::Toggle);
        assert_eq!(picked(&repo), ["a.txt", "d.txt"]);
        repo.pick_file("a.txt", PickHow::Toggle);
        assert!(matches!(repo.center, Center::Diff(_)));

        repo.pick_file("b.txt", PickHow::Only);
        repo.pick_file("d.txt", PickHow::Range);
        assert_eq!(picked(&repo), ["b.txt", "c.txt", "d.txt"]);

        let (_, fresh) = kelp_core::history::History::open(&dir).unwrap();
        repo.replace_history(fresh);
        assert_eq!(picked(&repo), ["b.txt", "c.txt", "d.txt"]);
        assert!(matches!(repo.center, Center::Multi(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn commit_with_files(name: &str, files: &[&str]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.email", "t@example.com"],
            &["config", "user.name", "T"],
        ] {
            run(&dir, args).unwrap();
        }
        for name in files {
            std::fs::write(dir.join(name), "one\n").unwrap();
        }
        run(&dir, &["add", "."]).unwrap();
        run(&dir, &["commit", "-q", "-m", "files"]).unwrap();
        dir
    }

    fn open_commit(dir: &std::path::Path) -> Repo {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let (git, history) = kelp_core::history::History::open(dir).unwrap();
        let mut repo = Repo::new(&ctx, git, history, Duration::ZERO);
        repo.select(Selection::Commit(0));
        let started = Instant::now();
        while repo.details.is_none() && started.elapsed() < Duration::from_secs(5) {
            repo.poll(&ctx, None);
            std::thread::sleep(Duration::from_millis(20));
        }
        repo
    }

    fn layouts(repo: &Repo) -> Vec<crate::diff_view::Layout> {
        match &repo.center {
            Center::Multi(multi) => multi.views.iter().map(|v| v.layout()).collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn adding_a_pick_keeps_the_diffs_that_are_already_loaded() {
        use crate::diff_view::Layout;
        let dir = commit_with_files("kelp-picks-reuse", &["a.txt", "b.txt", "c.txt"]);
        let mut repo = open_commit(&dir);

        repo.pick_file("a.txt", PickHow::Only);
        repo.pick_file("b.txt", PickHow::Toggle);
        assert_eq!(layouts(&repo), [Layout::Unified, Layout::Unified]);
        if let Center::Multi(multi) = &mut repo.center {
            multi.views[0].set_layout(Layout::Split);
        }

        repo.pick_file("c.txt", PickHow::Toggle);
        assert_eq!(picked(&repo), ["a.txt", "b.txt", "c.txt"]);
        assert_eq!(
            layouts(&repo),
            [Layout::Split, Layout::Unified, Layout::Unified],
            "the first diff was loaded again instead of kept"
        );

        repo.pick_file("b.txt", PickHow::Toggle);
        assert_eq!(picked(&repo), ["a.txt", "c.txt"]);
        assert_eq!(layouts(&repo), [Layout::Split, Layout::Unified]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_open_diff_becomes_the_first_of_the_stack() {
        use crate::diff_view::Layout;
        let dir = commit_with_files("kelp-picks-first", &["a.txt", "b.txt"]);
        let mut repo = open_commit(&dir);

        repo.pick_file("a.txt", PickHow::Only);
        if let Center::Diff(view) = &mut repo.center {
            view.set_layout(Layout::Split);
        }
        repo.pick_file("b.txt", PickHow::Toggle);
        assert_eq!(layouts(&repo), [Layout::Split, Layout::Unified]);

        repo.pick_file("b.txt", PickHow::Toggle);
        assert!(matches!(repo.center, Center::Diff(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
