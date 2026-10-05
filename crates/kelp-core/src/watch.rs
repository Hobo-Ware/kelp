use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::{RecursiveMode, Watcher as _};

const SETTLE: Duration = Duration::from_millis(400);
const MAX_CHECKED_PATHS: usize = 256;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Change {
    pub history: bool,
    pub status: bool,
}

impl Change {
    fn any(self) -> bool {
        self.history || self.status
    }

    fn merge(&mut self, other: Change) {
        self.history |= other.history;
        self.status |= other.status;
    }
}

pub struct Watcher {
    _inner: notify::RecommendedWatcher,
}

pub struct Layout {
    pub workdir: Option<PathBuf>,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
}

impl Layout {
    pub fn of(repo: &gix::Repository) -> Self {
        let real = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        Self {
            workdir: repo.workdir().map(real),
            git_dir: real(repo.path()),
            common_dir: real(repo.common_dir()),
        }
    }

    fn roots(&self) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = Vec::new();
        for dir in [
            self.workdir.as_ref(),
            Some(&self.git_dir),
            Some(&self.common_dir),
        ]
        .into_iter()
        .flatten()
        {
            if !roots.iter().any(|root| dir.starts_with(root)) {
                roots.retain(|root| !root.starts_with(dir));
                roots.push(dir.clone());
            }
        }
        roots
    }

    pub fn classify(&self, path: &Path) -> Classified {
        for git in [&self.git_dir, &self.common_dir] {
            if let Ok(rel) = path.strip_prefix(git) {
                return Classified::Git(git_change(rel));
            }
        }
        match &self.workdir {
            Some(workdir) => match path.strip_prefix(workdir) {
                Ok(rel) if !rel.as_os_str().is_empty() => Classified::Worktree(rel.to_path_buf()),
                _ => Classified::Git(Change::default()),
            },
            None => Classified::Git(Change::default()),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Classified {
    Git(Change),
    Worktree(PathBuf),
}

fn git_change(rel: &Path) -> Change {
    let text = rel.to_string_lossy();
    if text.ends_with(".lock") {
        return Change::default();
    }
    let first = rel
        .components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_default();
    match first.as_str() {
        "HEAD" | "ORIG_HEAD" | "FETCH_HEAD" | "MERGE_HEAD" | "REBASE_HEAD" | "CHERRY_PICK_HEAD"
        | "packed-refs" | "refs" | "rebase-merge" | "rebase-apply" => Change {
            history: true,
            status: true,
        },
        "index" => Change {
            history: false,
            status: true,
        },
        "worktrees" => git_change(&rel.iter().skip(2).collect::<PathBuf>()),
        _ => Change::default(),
    }
}

struct IgnoreCache {
    workdir: PathBuf,
    ignored_dirs: HashSet<PathBuf>,
    known_dirs: HashSet<PathBuf>,
}

impl IgnoreCache {
    fn new(workdir: PathBuf) -> Self {
        Self {
            workdir,
            ignored_dirs: HashSet::new(),
            known_dirs: HashSet::new(),
        }
    }

    fn under_ignored_dir(&self, rel: &Path) -> bool {
        rel.ancestors()
            .skip(1)
            .any(|dir| self.ignored_dirs.contains(dir))
    }

    fn any_relevant(&mut self, paths: HashSet<PathBuf>) -> bool {
        let candidates: Vec<PathBuf> = paths
            .into_iter()
            .filter(|rel| !self.under_ignored_dir(rel))
            .collect();
        if candidates.is_empty() {
            return false;
        }
        if candidates.len() > MAX_CHECKED_PATHS {
            return true;
        }
        let new_dirs: Vec<PathBuf> = candidates
            .iter()
            .flat_map(|rel| rel.ancestors().skip(1))
            .filter(|dir| !dir.as_os_str().is_empty() && !self.known_dirs.contains(*dir))
            .map(Path::to_path_buf)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let candidate_keys: Vec<(PathBuf, String)> = candidates
            .into_iter()
            .map(|rel| {
                let key = self.query_key(&rel);
                (rel, key)
            })
            .collect();
        let mut query: Vec<String> = new_dirs
            .iter()
            .map(|dir| format!("{}/", dir.to_string_lossy()))
            .collect();
        query.extend(candidate_keys.iter().map(|(_, key)| key.clone()));
        let Some(ignored) = self.check_ignore(&query) else {
            return true;
        };
        for dir in new_dirs {
            let key = format!("{}/", dir.to_string_lossy());
            if ignored.contains(&key) {
                self.ignored_dirs.insert(dir.clone());
            }
            self.known_dirs.insert(dir);
        }
        for (rel, key) in &candidate_keys {
            if key.ends_with('/') && ignored.contains(key) {
                self.ignored_dirs.insert(rel.clone());
            }
        }
        candidate_keys
            .iter()
            .any(|(rel, key)| !self.under_ignored_dir(rel) && !ignored.contains(key))
    }

    fn query_key(&self, rel: &Path) -> String {
        let path = rel.to_string_lossy();
        if self.workdir.join(rel).is_dir() {
            format!("{path}/")
        } else {
            path.into_owned()
        }
    }

    fn check_ignore(&self, paths: &[String]) -> Option<HashSet<String>> {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let log = crate::console::start("git", &["check-ignore", "--stdin", "-z"], &self.workdir);
        let mut child = Command::new("git")
            .current_dir(&self.workdir)
            .args(["check-ignore", "--stdin", "-z"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let input: String = paths.iter().map(|p| format!("{p}\0")).collect();
        child.stdin.take()?.write_all(input.as_bytes()).ok()?;
        let output = child.wait_with_output().ok()?;
        log.finish_output(&output);
        let none_ignored = output.status.code() == Some(1);
        if !output.status.success() && !none_ignored {
            return None;
        }
        Some(
            String::from_utf8_lossy(&output.stdout)
                .split('\0')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
        )
    }
}

impl Watcher {
    /// Watches the work tree and git directories and calls `on_change` after
    /// each burst of relevant file events settles. Ignored files never count.
    pub fn start(
        layout: Layout,
        on_change: impl Fn(Change) + Send + 'static,
    ) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
        let mut inner = notify::recommended_watcher(tx)?;
        for root in layout.roots() {
            inner.watch(&root, RecursiveMode::Recursive)?;
        }
        std::thread::Builder::new()
            .name("kelp-watch".into())
            .spawn(move || settle_loop(layout, rx, on_change))?;
        Ok(Self { _inner: inner })
    }
}

fn settle_loop(
    layout: Layout,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    on_change: impl Fn(Change),
) {
    let mut ignore = layout.workdir.clone().map(IgnoreCache::new);
    while let Ok(first) = rx.recv() {
        let mut events = vec![first];
        while let Ok(next) = rx.recv_timeout(SETTLE) {
            events.push(next);
        }
        let mut change = Change::default();
        let mut worktree_paths = HashSet::new();
        for path in events.into_iter().flatten().flat_map(|e| e.paths) {
            match layout.classify(&path) {
                Classified::Git(c) => change.merge(c),
                Classified::Worktree(rel) => {
                    worktree_paths.insert(rel);
                }
            }
        }
        if !change.status
            && !worktree_paths.is_empty()
            && ignore
                .as_mut()
                .is_some_and(|cache| cache.any_relevant(worktree_paths))
        {
            change.status = true;
        }
        if change.any() {
            on_change(change);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Signal(mpsc::Sender<()>);

    impl Drop for Signal {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }

    #[test]
    fn dropping_a_watcher_ends_its_worker_thread() {
        let dir = std::env::temp_dir().join(format!("kelp-watch-drop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        let real = dir.canonicalize().unwrap();
        let layout = Layout {
            workdir: Some(real.clone()),
            git_dir: real.join(".git"),
            common_dir: real.join(".git"),
        };
        let (tx, ended) = mpsc::channel();
        let signal = Signal(tx);
        let watcher = Watcher::start(layout, move |_| {
            let _keep = &signal;
        })
        .unwrap();
        assert!(ended.recv_timeout(Duration::from_millis(300)).is_err());

        drop(watcher);
        assert!(
            ended.recv_timeout(Duration::from_secs(5)).is_ok(),
            "the worker thread kept running after its watcher was dropped"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn layout() -> Layout {
        Layout {
            workdir: Some("/r".into()),
            git_dir: "/r/.git".into(),
            common_dir: "/r/.git".into(),
        }
    }

    #[test]
    fn ref_and_head_changes_reload_history() {
        let l = layout();
        let both = Classified::Git(Change {
            history: true,
            status: true,
        });
        assert_eq!(l.classify(Path::new("/r/.git/HEAD")), both);
        assert_eq!(l.classify(Path::new("/r/.git/refs/heads/main")), both);
        assert_eq!(l.classify(Path::new("/r/.git/packed-refs")), both);
    }

    #[test]
    fn index_changes_only_refresh_status() {
        assert_eq!(
            layout().classify(Path::new("/r/.git/index")),
            Classified::Git(Change {
                history: false,
                status: true
            })
        );
    }

    #[test]
    fn objects_locks_and_kelp_files_are_quiet() {
        let l = layout();
        for p in [
            "/r/.git/objects/ab/cdef",
            "/r/.git/refs/heads/main.lock",
            "/r/.git/index.lock",
            "/r/.git/kelp/comments.json",
        ] {
            assert_eq!(
                l.classify(Path::new(p)),
                Classified::Git(Change::default()),
                "{p}"
            );
        }
    }

    #[test]
    fn linked_worktree_head_counts() {
        assert!(git_change(Path::new("worktrees/feat/HEAD")).history);
        assert!(!git_change(Path::new("worktrees/feat/logs/HEAD")).history);
    }

    #[test]
    fn worktree_files_are_relative() {
        assert_eq!(
            layout().classify(Path::new("/r/src/main.rs")),
            Classified::Worktree("src/main.rs".into())
        );
    }

    #[test]
    fn nested_git_dir_is_one_root() {
        assert_eq!(layout().roots(), vec![PathBuf::from("/r")]);
        let linked = Layout {
            workdir: Some("/w".into()),
            git_dir: "/r/.git/worktrees/w".into(),
            common_dir: "/r/.git".into(),
        };
        assert_eq!(
            linked.roots(),
            vec![PathBuf::from("/w"), PathBuf::from("/r/.git")]
        );
    }
}
