use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, FontId, Key, Margin, RichText, Sense, Stroke, vec2};
use kelp_core::commit::{self, Details, FileChange};
use kelp_core::conflict::{self, InProgress};
use kelp_core::history::History;
use kelp_core::ops::{self, Op};
use kelp_core::refs::RefKind;
use kelp_core::review::{self, Review};
use kelp_core::undo;
use kelp_core::watch::{self, Watcher};
use kelp_core::workspace::{self, Stash, Worktree};
use kelp_core::{git_cli, status};

use crate::avatars::AvatarStore;
use crate::commands::Command;
use crate::conflict_view::{self, ConflictView};
use crate::dev_bench::ScrollBench;
use crate::dialogs::{self, Dialog, NewWorktree, Outcome, force_push_dialog};
use crate::diff_view::{self, DiffSource, DiffView};
use crate::graph_view::{self, GraphView};
use crate::icons::{self, Icon};
use crate::jobs::Jobs;
use crate::menus::{self, MenuContext};
use crate::rebase_view::{self, HeadReach, RebaseView};
use crate::settings::Settings;
use crate::{details, sidebar, theme, worktrees_view};

mod undo_actions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Wip,
    Commit(usize),
}

pub enum JobOutput {
    Reloaded(anyhow::Result<Box<History>>),
    Status(anyhow::Result<status::WorkingStatus>),
    CommitGraph(anyhow::Result<()>),
    Workspace(Box<WorkspaceInfo>),
    AutoFetch(anyhow::Result<String>),
    Opened(Result<(), String>),
    Op {
        label: String,
        quiet: bool,
        commit: bool,
        force_retry: Option<Op>,
        result: anyhow::Result<String>,
        undo: undo::Outcome,
    },
    Undone {
        record: Box<undo::Record>,
        result: anyhow::Result<Vec<String>>,
    },
    Redone {
        original: Box<undo::Record>,
        result: anyhow::Result<String>,
        outcome: undo::Outcome,
    },
    Search {
        query: String,
        rows: anyhow::Result<Vec<usize>>,
    },
}

enum PushPlan {
    Run(Op),
    Choose(Dialog),
    NoRemote,
}

#[derive(Default)]
pub struct Search {
    pub open: bool,
    pub query: String,
    pub searched: String,
    pub rows: Vec<usize>,
    pub current: usize,
    generation: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct LitKey {
    selected: Option<Selection>,
    search: Option<u64>,
    dim: bool,
    rows: usize,
}

#[derive(Default)]
pub struct WorkspaceInfo {
    pub stashes: Vec<Stash>,
    pub worktrees: Vec<WorktreeRow>,
    pub ahead_behind: HashMap<String, (usize, usize)>,
}

pub struct WorktreeRow {
    pub tree: Worktree,
    pub changes: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileListMode {
    Path,
    Tree,
}

pub enum Center {
    Graph,
    Diff(Box<DiffView>),
    Conflict(Box<ConflictView>),
    Worktrees,
    Rebase(Box<RebaseView>),
}

pub struct Toast {
    pub text: String,
    pub error: bool,
    pub shown_at: Instant,
}

pub struct Repo {
    pub dir: PathBuf,
    pub workdir: Option<PathBuf>,
    pub repo: gix::Repository,
    pub history: History,
    pub load_time: Duration,
    pub selected: Option<Selection>,
    pub details: Option<Details>,
    pub wip: Vec<FileChange>,
    pub status: status::WorkingStatus,
    pub commit_summary: String,
    pub commit_body: String,
    pub amend: bool,
    commit_in_flight: bool,
    pub workspace: WorkspaceInfo,
    pub graph: GraphView,
    pub center: Center,
    pub jobs: Jobs<JobOutput>,
    pub toast: Option<Toast>,
    pub avatars: AvatarStore,
    pub dialog: Option<Dialog>,
    pub outbox: Vec<PathBuf>,
    pub review: Review,
    pub author: String,
    pub file_list_mode: FileListMode,
    pub show_all_files: bool,
    pub tree_cache: HashMap<String, Vec<commit::TreeEntry>>,
    pub search: Search,
    lit_cache: Option<(LitKey, Vec<bool>)>,
    open_after_ops: Vec<PathBuf>,
    was_focused: Option<bool>,
    pub operation: Option<InProgress>,
    banner: conflict_view::Banner,
    diff_layout: diff_view::Layout,
    pub editor: String,
    _watcher: Option<Watcher>,
    watch_events: mpsc::Receiver<watch::Change>,
    reload_again: bool,
    status_again: bool,
    last_fetch: Instant,
    auto_fetch_failed: bool,
    undo: undo::Stack,
    head_reach: HeadReach,
    bench: Option<ScrollBench>,
}

impl Repo {
    pub fn new(
        ctx: &egui::Context,
        repo: gix::Repository,
        history: History,
        load_time: Duration,
    ) -> Self {
        let workdir = repo.workdir().map(Path::to_path_buf);
        let dir = workdir.clone().unwrap_or_else(|| repo.path().to_path_buf());
        let avatars =
            AvatarStore::new(ctx.clone(), kelp_core::avatar::GitHubRepo::from_repo(&repo));
        let review = Review::load(repo.common_dir());
        let stale_undo_dir = dir.clone();
        std::thread::spawn(move || undo::clear_saved(&stale_undo_dir));
        let author = review::author_name(&repo);
        let (watch_tx, watch_events) = mpsc::channel();
        let repaint = ctx.clone();
        let watcher = Watcher::start(watch::Layout::of(&repo), move |change| {
            if watch_tx.send(change).is_ok() {
                repaint.request_repaint();
            }
        })
        .inspect_err(|e| eprintln!("kelp: not watching {}: {e:#}", dir.display()))
        .ok();
        let mut ready = Self {
            review,
            author,
            file_list_mode: FileListMode::Path,
            show_all_files: false,
            tree_cache: HashMap::new(),
            search: Search::default(),
            lit_cache: None,
            dir,
            workdir,
            repo,
            history,
            load_time,
            selected: None,
            details: None,
            wip: Vec::new(),
            status: status::WorkingStatus::default(),
            commit_summary: String::new(),
            commit_body: String::new(),
            amend: false,
            commit_in_flight: false,
            workspace: WorkspaceInfo::default(),
            graph: GraphView::new(),
            center: Center::Graph,
            jobs: Jobs::new(ctx.clone()),
            toast: None,
            avatars,
            dialog: None,
            outbox: Vec::new(),
            open_after_ops: Vec::new(),
            was_focused: None,
            operation: None,
            banner: conflict_view::Banner::new(),
            diff_layout: diff_view::Layout::default(),
            editor: String::new(),
            _watcher: watcher,
            watch_events,
            reload_again: false,
            status_again: false,
            last_fetch: Instant::now(),
            auto_fetch_failed: false,
            undo: undo::Stack::default(),
            head_reach: HeadReach::new(),
            bench: ScrollBench::from_env(),
        };
        if let Some(row) = ready.head_row() {
            ready.select(Selection::Commit(row));
        }
        ready.refresh_status();
        ready.refresh_workspace();

        if let Some(row) = std::env::var("KELP_SELECT_COMMIT")
            .ok()
            .and_then(|rev| ready.repo.rev_parse_single(rev.as_str()).ok())
            .and_then(|id| ready.history.row(&id.detach()))
        {
            ready.selected = None;
            ready.reveal(Selection::Commit(row));
        }
        if let Ok(wanted) = std::env::var("KELP_OPEN_REBASE") {
            let (base, actions) = wanted.split_once(':').unwrap_or((&wanted, ""));
            let view = RebaseView::open(ctx, ready.dir.clone(), base.to_string());
            ready.center = Center::Rebase(Box::new(view.with_actions(actions)));
        }
        let open_diff = std::env::var("KELP_OPEN_DIFF").unwrap_or_default();
        let wanted_preview = open_diff.strip_prefix("preview:").map(str::to_string);
        let wanted_path = open_diff
            .strip_prefix("path:")
            .map(str::to_string)
            .or(wanted_preview.clone());
        if std::env::var_os("KELP_OPEN_DIFF").is_some()
            && let Some(path) = wanted_path.or_else(|| {
                ready
                    .details
                    .as_ref()
                    .and_then(|d| d.changes.first())
                    .map(|c| c.path.clone())
            })
        {
            ready.open_diff(&path);
            if std::env::var("KELP_OPEN_DIFF").as_deref() == Ok("split")
                && let Center::Diff(view) = &mut ready.center
            {
                view.set_layout(diff_view::Layout::Split);
            }
            if wanted_preview.is_some()
                && let Center::Diff(view) = &mut ready.center
            {
                view.show_preview();
            }
        }
        if std::env::var_os("KELP_SELECT_WIP").is_some() {
            ready.selected = Some(Selection::Wip);
        }
        ready.operation = conflict::in_progress(ready.repo.path());
        if let Ok(target) = std::env::var("KELP_OPEN_CONFLICT") {
            let (path, picks) = target.split_once('#').unwrap_or((&target, ""));
            ready.selected = Some(Selection::Wip);
            ready.open_conflict(path);
            if let Center::Conflict(view) = &mut ready.center {
                view.preselect(picks);
            }
        }
        if let Some(path) = std::env::var("KELP_OPEN_DIFF")
            .ok()
            .and_then(|v| v.strip_prefix("unstaged:").map(str::to_string))
        {
            ready.open_working_diff(&path, false);
        }
        if let Ok(query) = std::env::var("KELP_SEARCH") {
            ready.search.open = true;
            ready.search.query = query;
            ready.run_search();
        }
        if std::env::var_os("KELP_OPEN_WORKTREES").is_some() {
            ready.center = Center::Worktrees;
        }
        if std::env::var("KELP_OPEN_DIALOG").as_deref() == Ok("worktree") {
            let ctx = ready.menu_context();
            ready.dialog = Some(Dialog::NewWorktree(NewWorktree::new(
                ctx.repo_dir_name,
                "main".into(),
                None,
                ctx.local_branches,
            )));
        }
        let branch = ready.current_branch().unwrap_or("HEAD").to_string();
        match std::env::var("KELP_OPEN_DIALOG").as_deref() {
            Ok("reset-hard") => {
                if let Some(Selection::Commit(row)) = ready.selected {
                    let id = ready.history.id(row).to_string();
                    ready.execute(ctx, vec![Command::ResetHard(id)]);
                }
            }
            Ok("push-to") => {
                if let PushPlan::Choose(dialog) = ready.push_plan(&branch) {
                    ready.dialog = Some(dialog);
                }
            }
            Ok("force-push") => {
                ready.dialog = Some(force_push_dialog(Op::Push {
                    branch,
                    remote: "origin".into(),
                    set_upstream: false,
                    force_with_lease: true,
                }));
            }
            _ => {}
        }
        if !git_cli::has_commit_graph(&ready.repo) {
            let dir = ready.repo.path().to_path_buf();
            ready.jobs.spawn("Speeding up history", move || {
                JobOutput::CommitGraph(git_cli::write_commit_graph(&dir))
            });
        }
        ready
    }

    pub fn name(&self) -> String {
        self.dir
            .file_name()
            .map(|n| n.to_string_lossy().trim_end_matches(".git").to_string())
            .unwrap_or_else(|| self.dir.display().to_string())
    }

    pub fn head_row(&self) -> Option<usize> {
        self.history.refs.head.and_then(|h| self.history.row(&h))
    }

    pub fn current_branch(&self) -> Option<&str> {
        self.history.refs.head_branch.as_deref()
    }

    pub fn select(&mut self, selection: Selection) {
        if self.selected == Some(selection) {
            return;
        }
        self.selected = Some(selection);
        self.tree_cache.clear();
        self.details = match selection {
            Selection::Commit(row) => commit::details(&self.repo, self.history.id(row)).ok(),
            Selection::Wip => None,
        };
    }

    pub fn reveal(&mut self, selection: Selection) {
        self.select(selection);
        self.graph.scroll_to = Some(selection);
        if !matches!(self.center, Center::Graph) {
            self.center = Center::Graph;
        }
    }

    pub fn open_diff(&mut self, path: &str) {
        let source = match self.selected {
            Some(Selection::Wip) => {
                let staged_only = self.status.staged.iter().any(|c| c.path == path)
                    && !self.status.unstaged.iter().any(|c| c.path == path);
                if staged_only {
                    DiffSource::Staged
                } else {
                    DiffSource::Unstaged
                }
            }
            Some(Selection::Commit(row)) => {
                let id = self.history.id(row);
                let changed = self
                    .details
                    .as_ref()
                    .is_some_and(|d| d.changes.iter().any(|c| c.path == path));
                if changed {
                    DiffSource::Commit(id)
                } else {
                    DiffSource::File(id)
                }
            }
            None => return,
        };
        match DiffView::load(&self.repo, self.workdir.as_deref(), source, path) {
            Ok(view) => self.show_diff(view),
            Err(e) => self.notify(format!("Could not open file: {e:#}"), true),
        }
    }

    pub fn open_conflict(&mut self, path: &str) {
        let Some(workdir) = self.workdir.clone() else {
            return;
        };
        match ConflictView::load(&workdir, path) {
            Ok(view) => self.center = Center::Conflict(Box::new(view)),
            Err(e) => self.notify(format!("Could not open conflict: {e:#}"), true),
        }
    }

    fn run_conflict_job(&mut self, job: conflict_view::Job) {
        if matches!(
            job,
            conflict_view::Job::Resolve { .. } | conflict_view::Job::TakeSide { .. }
        ) {
            self.center = Center::Graph;
        }
        let label = job.label();
        let dir = self.dir.clone();
        let job_label = label.clone();
        self.jobs.spawn(label, move || JobOutput::Op {
            label: job_label.clone(),
            quiet: false,
            commit: false,
            force_retry: None,
            result: job.run(&dir),
            undo: undo::Outcome::NotUndoable {
                label: job_label,
                reason: "conflict steps are not tracked",
            },
        });
    }

    pub fn open_working_diff(&mut self, path: &str, staged: bool) {
        let source = if staged {
            DiffSource::Staged
        } else {
            DiffSource::Unstaged
        };
        match DiffView::load(&self.repo, self.workdir.as_deref(), source, path) {
            Ok(view) => self.show_diff(view),
            Err(e) => self.notify(format!("Could not open diff: {e:#}"), true),
        }
    }

    fn show_diff(&mut self, mut view: DiffView) {
        view.set_layout(self.diff_layout);
        self.center = Center::Diff(Box::new(view));
    }

    pub fn committing(&self) -> bool {
        self.commit_in_flight
    }

    pub fn has_head(&self) -> bool {
        self.repo.head_id().is_ok()
    }

    pub fn commit(&mut self) {
        let summary = self.commit_summary.trim();
        if summary.is_empty() || (self.status.staged.is_empty() && !self.amend) {
            return;
        }
        let body = self.commit_body.trim();
        let message = if body.is_empty() {
            summary.to_string()
        } else {
            format!("{summary}\n\n{body}")
        };
        self.commit_in_flight = true;
        self.run_op(Op::Commit {
            message,
            amend: self.amend,
        });
    }

    pub fn toggle_amend(&mut self) {
        self.amend = !self.amend;
        if self.amend
            && self.commit_summary.trim().is_empty()
            && let Some(head) = self.history.refs.head
            && let Ok(details) = commit::details(&self.repo, head)
        {
            self.commit_summary = details.title;
            self.commit_body = details.body;
        }
    }

    pub fn notify(&mut self, text: impl Into<String>, error: bool) {
        self.toast = Some(Toast {
            text: text.into(),
            error,
            shown_at: Instant::now(),
        });
    }

    pub fn menu_context(&self) -> MenuContext {
        MenuContext {
            current_branch: self.current_branch().map(str::to_string),
            repo_dir_name: self
                .dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("repo")
                .to_string(),
            local_branches: self
                .history
                .refs
                .of_kind(RefKind::Local)
                .map(|l| l.name.clone())
                .collect(),
            head: self.history.refs.head.map(|h| h.to_string()),
        }
    }

    pub fn execute(&mut self, ctx: &egui::Context, commands: Vec<Command>) {
        let mut ran_op = false;
        for command in commands {
            match command {
                Command::Run(op) => {
                    self.run_op(op);
                    ran_op = true;
                }
                Command::Push(branch) => match self.push_plan(&branch) {
                    PushPlan::Run(op) => {
                        self.run_op(op);
                        ran_op = true;
                    }
                    PushPlan::Choose(dialog) => self.dialog = Some(dialog),
                    PushPlan::NoRemote => {
                        self.notify("This repository has no remote to push to", true)
                    }
                },
                Command::ResetHard(commit) => {
                    let dropped = git_cli::run(
                        &self.dir,
                        &["rev-list", "--count", &format!("{commit}..HEAD")],
                    )
                    .ok()
                    .and_then(|out| out.trim().parse().ok())
                    .unwrap_or(0);
                    let dirty = self.wip.len().saturating_sub(self.status.untracked.len());
                    let branch = self.current_branch().unwrap_or("HEAD").to_string();
                    self.dialog =
                        Some(dialogs::reset_hard_dialog(&branch, &commit, dropped, dirty));
                }
                Command::Open(dialog) => self.dialog = Some(dialog),
                Command::Copy(text) => {
                    ctx.copy_text(text.clone());
                    self.notify(format!("Copied {text}"), false);
                }
                Command::Reveal(selection) => self.reveal(selection),
                Command::ShowWorktrees => self.center = Center::Worktrees,
                Command::OpenRebase(base) => {
                    let view = RebaseView::open(ctx, self.dir.clone(), base);
                    self.center = Center::Rebase(Box::new(view));
                }
                Command::StartRebase(start) => {
                    let dir = self.dir.clone();
                    self.center = Center::Graph;
                    self.jobs.spawn("Rebasing", move || JobOutput::Op {
                        label: "Interactive rebase".into(),
                        quiet: false,
                        commit: false,
                        force_retry: None,
                        result: start.run(&dir),
                        undo: undo::Outcome::NotUndoable {
                            label: "Interactive rebase".into(),
                            reason: "interactive rebases are not tracked yet (the reflog keeps the old commits)",
                        },
                    });
                    ran_op = true;
                }
                Command::OpenRepo(path) => {
                    let path = if path.is_relative() {
                        self.dir.join(path)
                    } else {
                        path
                    };
                    if ran_op {
                        self.open_after_ops.push(path);
                    } else {
                        self.outbox.push(path);
                    }
                }
                Command::OpenTerminal(path) => {
                    if let Err(e) = open_terminal(&path) {
                        self.notify(format!("Could not open a terminal: {e}"), true);
                    }
                }
                Command::OpenInEditor(rel) => match self.working_file(&rel) {
                    Some(path) => {
                        let setting = self.editor.clone();
                        self.jobs.spawn("Opening", move || {
                            let app = crate::open_with::editor_app(&setting);
                            JobOutput::Opened(
                                crate::open_with::open_in_editor(&path, app.as_deref())
                                    .map_err(|e| format!("Could not open {rel}: {e}")),
                            )
                        });
                    }
                    None => self.notify(format!("{rel} is not in the working tree"), true),
                },
                Command::RevealFile(rel) => match self.working_file(&rel) {
                    Some(path) => {
                        if let Err(e) = reveal_in_finder(&path) {
                            self.notify(format!("Could not reveal {rel}: {e}"), true);
                        }
                    }
                    None => self.notify(format!("{rel} is not in the working tree"), true),
                },
                Command::Undo => self.undo_last(),
            }
        }
    }

    fn working_file(&self, rel: &str) -> Option<PathBuf> {
        self.workdir
            .as_ref()
            .map(|w| w.join(rel))
            .filter(|p| p.exists())
    }

    fn push_plan(&self, branch: &str) -> PushPlan {
        let push = |remote: String, set_upstream| Op::Push {
            branch: branch.to_string(),
            remote,
            set_upstream,
            force_with_lease: false,
        };
        if let Some(remote) = workspace::upstream_remote(&self.repo, branch) {
            return PushPlan::Run(push(remote, false));
        }
        let remotes: Vec<String> = self
            .repo
            .remote_names()
            .iter()
            .map(|r| r.to_string())
            .collect();
        match workspace::default_push_remote(&remotes) {
            Some(remote) => PushPlan::Run(push(remote, true)),
            None if remotes.is_empty() => PushPlan::NoRemote,
            None => PushPlan::Choose(Dialog::PushTo {
                branch: branch.to_string(),
                remote: remotes[0].clone(),
                remotes,
            }),
        }
    }

    pub fn run_op(&mut self, op: Op) {
        let label = op.label();
        let dir = self.dir.clone();
        let job_label = label.clone();
        let quiet = matches!(
            op,
            Op::Stage(_)
                | Op::Unstage { .. }
                | Op::StageAll
                | Op::UnstageAll { .. }
                | Op::ApplyToIndex { .. }
        );
        let commit = matches!(op, Op::Commit { .. });
        let force_retry = match &op {
            Op::Push {
                branch,
                remote,
                set_upstream,
                force_with_lease: false,
            } => Some(Op::Push {
                branch: branch.clone(),
                remote: remote.clone(),
                set_upstream: *set_upstream,
                force_with_lease: true,
            }),
            _ => None,
        };
        self.jobs.spawn(label, move || {
            let (result, undo) = undo::run_recorded(&op, &dir);
            JobOutput::Op {
                label: job_label,
                quiet,
                commit,
                force_retry,
                result,
                undo,
            }
        });
    }

    pub fn run_search(&mut self) {
        let query = self.search.query.trim().to_string();
        if query.is_empty() {
            self.search.rows.clear();
            self.search.searched.clear();
            self.search.generation += 1;
            return;
        }
        let dir = self.dir.clone();
        let ids = self.history.ids().to_vec();
        self.jobs.spawn("Searching", move || {
            let rows = kelp_core::search::matching_rows(&dir, &ids, &query);
            JobOutput::Search { query, rows }
        });
    }

    fn step_search(&mut self, forward: bool) {
        let n = self.search.rows.len();
        if n == 0 {
            return;
        }
        self.search.current = if forward {
            (self.search.current + 1) % n
        } else {
            (self.search.current + n - 1) % n
        };
        let row = self.search.rows[self.search.current];
        self.reveal(Selection::Commit(row));
    }

    fn ensure_lit(&mut self, settings: &Settings) {
        let searching = self.search.open && !self.search.searched.is_empty();
        let key = LitKey {
            selected: self.selected,
            search: searching.then_some(self.search.generation),
            dim: settings.dim_outside_history,
            rows: self.history.len(),
        };
        let fading_history =
            settings.dim_outside_history && matches!(self.selected, Some(Selection::Commit(_)));
        if !searching && !fading_history {
            self.lit_cache = None;
            return;
        }
        if self.lit_cache.as_ref().is_none_or(|(k, _)| *k != key) {
            let mut lit = vec![false; self.history.len()];
            if searching {
                for &row in &self.search.rows {
                    lit[row] = true;
                }
            } else if let Some(Selection::Commit(start)) = self.selected {
                let mut stack = vec![start];
                while let Some(row) = stack.pop() {
                    if !std::mem::replace(&mut lit[row], true) {
                        stack.extend(self.history.parents(row).iter().map(|&p| p as usize));
                    }
                }
            }
            self.lit_cache = Some((key, lit));
        }
    }

    pub fn refresh_status(&mut self) {
        let Some(workdir) = self.workdir.clone() else {
            return;
        };
        if self.jobs.is_running("Checking changes") {
            self.status_again = true;
            return;
        }
        self.jobs.spawn("Checking changes", move || {
            JobOutput::Status(status::working_status(&workdir))
        });
    }

    pub fn refresh_workspace(&mut self) {
        if self.jobs.is_running("Reading worktrees") {
            return;
        }
        let dir = self.dir.clone();
        let branches: Vec<String> = self
            .history
            .refs
            .of_kind(RefKind::Local)
            .map(|l| l.name.clone())
            .collect();
        self.jobs.spawn("Reading worktrees", move || {
            let worktrees = workspace::worktrees(&dir)
                .unwrap_or_default()
                .into_iter()
                .map(|tree| {
                    let changes = if tree.prunable {
                        None
                    } else {
                        workspace::change_count(&tree.path)
                    };
                    WorktreeRow { tree, changes }
                })
                .collect();
            let ahead_behind = branches
                .into_iter()
                .filter_map(|b| workspace::ahead_behind(&dir, &b).map(|ab| (b, ab)))
                .collect();
            JobOutput::Workspace(Box::new(WorkspaceInfo {
                stashes: workspace::stashes(&dir).unwrap_or_default(),
                worktrees,
                ahead_behind,
            }))
        });
    }

    pub fn reload(&mut self) {
        if self.jobs.is_running("Reloading") {
            self.reload_again = true;
            return;
        }
        let dir = self.dir.clone();
        self.jobs.spawn("Reloading", move || {
            JobOutput::Reloaded(History::open(&dir).map(|(_, history)| Box::new(history)))
        });
    }

    pub fn poll(&mut self, ctx: &egui::Context, fetch_every: Option<Duration>) {
        self.avatars.poll();
        for output in self.jobs.finished() {
            match output {
                JobOutput::Status(Ok(working)) => {
                    if std::mem::take(&mut self.status_again) {
                        self.refresh_status();
                    }
                    self.operation = conflict::in_progress(self.repo.path());
                    if let Center::Conflict(view) = &self.center
                        && !working.conflicted.contains(&view.path)
                    {
                        self.center = Center::Graph;
                    }
                    self.wip = working.all();
                    self.status = working;
                    if let Center::Diff(view) = &mut self.center
                        && view.is_working()
                        && let Err(e) = view.reload(&self.repo, self.workdir.as_deref())
                    {
                        self.toast = Some(Toast {
                            text: format!("{e:#}"),
                            error: true,
                            shown_at: Instant::now(),
                        });
                    }
                    if self.wip.is_empty() && self.selected == Some(Selection::Wip) {
                        self.selected = None;
                        if let Some(row) = self.head_row() {
                            self.select(Selection::Commit(row));
                        }
                    }
                }
                JobOutput::Status(Err(e)) => self.notify(format!("{e:#}"), true),
                JobOutput::Reloaded(result) => {
                    match result {
                        Ok(history) => {
                            self.replace_history(*history);
                            self.refresh_workspace();
                        }
                        Err(e) => self.notify(format!("Reload failed: {e:#}"), true),
                    }
                    if std::mem::take(&mut self.reload_again) {
                        self.reload();
                    }
                }
                JobOutput::Opened(Ok(())) => {}
                JobOutput::Opened(Err(e)) => self.notify(e, true),
                JobOutput::AutoFetch(Ok(_)) => self.auto_fetch_failed = false,
                JobOutput::AutoFetch(Err(e)) => {
                    if !self.auto_fetch_failed {
                        self.notify(format!("Auto-fetch failed: {e:#}"), true);
                    }
                    self.auto_fetch_failed = true;
                }
                JobOutput::CommitGraph(Ok(())) => {}
                JobOutput::CommitGraph(Err(e)) => {
                    self.notify(format!("Could not write commit-graph: {e:#}"), true)
                }
                JobOutput::Workspace(info) => self.workspace = *info,
                JobOutput::Undone { record, result } => self.finish_undo(*record, result),
                JobOutput::Redone {
                    original,
                    result,
                    outcome,
                } => self.finish_redo(*original, result, outcome),
                JobOutput::Search { query, rows } => {
                    if query == self.search.query.trim() {
                        self.search.rows = rows.unwrap_or_default();
                        self.search.searched = query;
                        self.search.current = 0;
                        self.search.generation += 1;
                        if let Some(&row) = self.search.rows.first() {
                            self.reveal(Selection::Commit(row));
                        }
                    }
                }
                JobOutput::Op {
                    label,
                    quiet,
                    commit,
                    force_retry,
                    result,
                    undo,
                } => {
                    if commit {
                        self.commit_in_flight = false;
                    }
                    self.keep_undo(undo);
                    match result {
                        Ok(_) => {
                            if commit {
                                self.commit_summary.clear();
                                self.commit_body.clear();
                                self.amend = false;
                            }
                            if !quiet {
                                self.notify(format!("{label} done"), false);
                            }
                            self.outbox.append(&mut self.open_after_ops);
                        }
                        Err(e) => {
                            self.open_after_ops.clear();
                            let error = format!("{e:#}");
                            match force_retry {
                                Some(op) if ops::push_rejected(&error) => {
                                    self.dialog = Some(force_push_dialog(op))
                                }
                                _ => self.notify(error, true),
                            }
                        }
                    }
                    self.reload();
                    self.refresh_status();
                }
            }
        }
        self.watch_focus(ctx);
        self.apply_watch_events();
        self.auto_fetch(ctx, fetch_every);
    }

    fn apply_watch_events(&mut self) {
        let mut history = false;
        let mut status = false;
        for change in self.watch_events.try_iter() {
            history |= change.history;
            status |= change.status;
        }
        if history {
            self.reload();
        }
        if status {
            self.refresh_status();
        }
    }

    fn auto_fetch(&mut self, ctx: &egui::Context, every: Option<Duration>) {
        let Some(every) = every else {
            return;
        };
        if self.repo.remote_names().is_empty() {
            return;
        }
        let due = self.last_fetch + every;
        let now = Instant::now();
        if now < due {
            ctx.request_repaint_after(due - now);
            return;
        }
        self.last_fetch = now;
        ctx.request_repaint_after(every);
        if self.jobs.is_running("Fetching") || self.jobs.is_running("Auto-fetching") {
            return;
        }
        let dir = self.dir.clone();
        self.jobs.spawn("Auto-fetching", move || {
            JobOutput::AutoFetch(Op::Fetch.run(&dir))
        });
    }

    fn replace_history(&mut self, history: History) {
        let selected_id = match self.selected {
            Some(Selection::Commit(row)) => Some(self.history.id(row)),
            _ => None,
        };
        self.history = history;
        self.graph.clear_cache();
        let keep_wip = self.selected == Some(Selection::Wip);
        self.selected = None;
        if keep_wip {
            self.select(Selection::Wip);
        } else if let Some(row) = selected_id
            .and_then(|id| self.history.row(&id))
            .or_else(|| self.head_row())
        {
            self.select(Selection::Commit(row));
        }
    }

    fn watch_focus(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.focused);
        if focused && self.was_focused == Some(false) {
            self.refresh_status();
            self.reload();
        }
        self.was_focused = Some(focused);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, settings: &Settings) {
        let ctx = ui.ctx().clone();
        let mut commands = Vec::new();
        self.handle_keys(ui);

        egui::Panel::top("toolbar")
            .exact_size(56.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::symmetric(16, 0))
                    .stroke(Stroke::new(1.0, theme::BORDER)),
            )
            .show(ui, |ui| self.toolbar(ui, &mut commands));

        egui::Panel::bottom("status")
            .exact_size(26.0)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x0f, 0x11, 0x15))
                    .inner_margin(Margin::symmetric(14, 0)),
            )
            .show(ui, |ui| self.status_bar(ui));

        egui::Panel::left("sidebar")
            .default_size(250.0)
            .min_size(180.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0, theme::BORDER)),
            )
            .show(ui, |ui| sidebar::ui(ui, self, &mut commands));

        egui::Panel::right("details")
            .default_size(370.0)
            .min_size(260.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0, theme::BORDER)),
            )
            .show(ui, |ui| details::ui(ui, self));

        if let Some(operation) = self.operation.clone() {
            let conflicted = self.status.conflicted.len();
            let job = egui::Panel::top("operation-banner")
                .frame(egui::Frame::NONE)
                .show(ui, |ui| self.banner.ui(ui, &operation, conflicted))
                .inner;
            if let Some(job) = job {
                self.run_conflict_job(job);
            }
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BG))
            .show(ui, |ui| match &mut self.center {
                Center::Diff(view) => {
                    let event = view.ui(ui, &mut self.review, &self.author);
                    self.diff_layout = view.layout();
                    match event {
                        diff_view::Event::Close => self.center = Center::Graph,
                        diff_view::Event::Changed => {
                            if let Err(e) = self.review.save() {
                                self.toast = Some(Toast {
                                    text: format!("Could not save comment: {e:#}"),
                                    error: true,
                                    shown_at: Instant::now(),
                                });
                            }
                        }
                        diff_view::Event::Run(op) => commands.push(Command::Run(op)),
                        diff_view::Event::OpenInEditor => {
                            commands.push(Command::OpenInEditor(view.path().to_string()))
                        }
                        diff_view::Event::None => {}
                    }
                }
                Center::Conflict(view) => match view.ui(ui) {
                    conflict_view::Event::Close => self.center = Center::Graph,
                    conflict_view::Event::Run(job) => self.run_conflict_job(job),
                    conflict_view::Event::None => {}
                },
                Center::Worktrees => worktrees_view::ui(ui, self, &mut commands),
                Center::Rebase(view) => match view.ui(ui) {
                    rebase_view::Event::Close => self.center = Center::Graph,
                    rebase_view::Event::Start(start) => commands.push(Command::StartRebase(start)),
                    rebase_view::Event::None => {}
                },
                Center::Graph => self.graph_center(ui, &mut commands, settings),
            });

        if let Some(dialog) = &mut self.dialog {
            match dialogs::show(&ctx, dialog) {
                Outcome::Keep => {}
                Outcome::Close => self.dialog = None,
                Outcome::Confirm(confirmed) => {
                    self.dialog = None;
                    commands.extend(confirmed);
                }
            }
        }

        self.toast(ui);
        if !commands.is_empty() {
            self.execute(&ctx, commands);
        }
    }

    fn graph_center(
        &mut self,
        ui: &mut egui::Ui,
        commands: &mut Vec<Command>,
        settings: &Settings,
    ) {
        if self.search.open {
            self.search_bar(ui);
        }
        self.ensure_lit(settings);
        let head_row = self.head_row();
        let menu_ctx = self.menu_context();
        let Repo {
            repo: git,
            history,
            selected,
            graph,
            bench,
            avatars,
            wip: changes,
            lit_cache,
            head_reach,
            ..
        } = self;
        let wip = (!changes.is_empty()).then(|| graph_view::Wip {
            head_row: head_row.unwrap_or(0),
            changes,
        });
        if let Some(bench) = bench {
            graph.scroll_to = Some(Selection::Commit(bench.next_row(history.len())));
        }
        let started = Instant::now();
        let input = graph_view::GraphInput {
            repo: git,
            history,
            selected: *selected,
            wip,
            lit: lit_cache.as_ref().map(|(_, lit)| lit.as_slice()),
            descriptions: settings.show_descriptions,
        };
        let action = graph.ui(ui, input, avatars, |ui, selection, title| match selection {
            Selection::Wip => menus::wip(ui, commands),
            Selection::Commit(row) => {
                let id = history.id(row).to_string();
                let parents = history.parents(row).len();
                let can_rebase = head_reach.can_rebase_from(history, head_row, row);
                menus::commit(ui, &id, parents, title, &menu_ctx, can_rebase, commands)
            }
        });
        if let Some(bench) = bench {
            bench.record(ui.ctx(), started.elapsed());
        }
        if let Some(graph_view::Action::Select(selection)) = action {
            self.select(selection);
        }
    }

    fn search_bar(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let pos = egui::pos2(rect.right() - 420.0, rect.top() + 36.0);
        let mut close = false;
        let mut changed = false;
        let mut step = None;
        egui::Area::new(egui::Id::new("search-bar"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x23, 0x28, 0x33))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(0x3a, 0x42, 0x50)))
                    .corner_radius(8)
                    .inner_margin(Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_width(390.0);
                        ui.horizontal(|ui| {
                            let edit = ui.add(
                                egui::TextEdit::singleline(&mut self.search.query)
                                    .hint_text("Search messages, authors, hashes")
                                    .desired_width(220.0),
                            );
                            if self.search.searched.is_empty() && self.search.query.is_empty() {
                                edit.request_focus();
                            }
                            changed = edit.changed();
                            let (enter, shift, esc) = ui.input(|i| {
                                (
                                    i.key_pressed(Key::Enter),
                                    i.modifiers.shift,
                                    i.key_pressed(Key::Escape),
                                )
                            });
                            if edit.lost_focus() && enter {
                                step = Some(!shift);
                                edit.request_focus();
                            }
                            if esc {
                                close = true;
                            }
                            let status = if self.jobs.is_running("Searching") {
                                "…".to_string()
                            } else if self.search.searched.is_empty() {
                                String::new()
                            } else if self.search.rows.is_empty() {
                                "no matches".to_string()
                            } else {
                                format!("{} of {}", self.search.current + 1, self.search.rows.len())
                            };
                            ui.label(RichText::new(status).size(12.0).color(theme::TEXT_MUTED));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(egui::Button::new("×").frame(false))
                                        .on_hover_text("Close (Esc)")
                                        .clicked()
                                    {
                                        close = true;
                                    }
                                    if ui
                                        .add(egui::Button::new("↓").frame(false))
                                        .on_hover_text("Next (Enter)")
                                        .clicked()
                                    {
                                        step = Some(true);
                                    }
                                    if ui
                                        .add(egui::Button::new("↑").frame(false))
                                        .on_hover_text("Previous (Shift+Enter)")
                                        .clicked()
                                    {
                                        step = Some(false);
                                    }
                                },
                            );
                        });
                    });
            });
        if changed {
            self.run_search();
        }
        if let Some(forward) = step {
            self.step_search(forward);
        }
        if close {
            self.search = Search {
                generation: self.search.generation + 1,
                ..Search::default()
            };
        }
    }

    fn handle_keys(&mut self, ui: &egui::Ui) {
        if ui.input(|i| i.modifiers.command && i.key_pressed(Key::F)) {
            self.search.open = true;
            self.center = Center::Graph;
        }
        if ui.ctx().egui_wants_keyboard_input() || self.dialog.is_some() || self.history.is_empty()
        {
            return;
        }
        let (undo, redo) = ui.input(|i| {
            let z = i.modifiers.command && i.key_pressed(Key::Z);
            (z && !i.modifiers.shift, z && i.modifiers.shift)
        });
        if undo {
            self.undo_last();
        } else if redo {
            self.redo_last();
        }
        if !matches!(self.center, Center::Graph) {
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                self.center = Center::Graph;
            }
            return;
        }
        let (down, up) = ui.input(|i| {
            (
                i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::J),
                i.key_pressed(Key::ArrowUp) || i.key_pressed(Key::K),
            )
        });
        if !down && !up {
            return;
        }
        let has_wip = !self.wip.is_empty();
        let head = self.head_row().unwrap_or(0);
        let order = |s: Selection| -> usize {
            match s {
                Selection::Wip => head,
                Selection::Commit(r) if has_wip && r >= head => r + 1,
                Selection::Commit(r) => r,
            }
        };
        let from_order = |d: usize| -> Selection {
            match d {
                d if has_wip && d == head => Selection::Wip,
                d if has_wip && d > head => Selection::Commit(d - 1),
                d => Selection::Commit(d),
            }
        };
        let total = self.history.len() + has_wip as usize;
        let current = self.selected.map(order).unwrap_or(0);
        let next = if down {
            (current + 1).min(total - 1)
        } else {
            current.saturating_sub(1)
        };
        self.reveal(from_order(next));
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, commands: &mut Vec<Command>) {
        let branch = self.current_branch().map(str::to_string);
        let upstream = branch
            .as_deref()
            .and_then(|b| workspace::upstream_remote(&self.repo, b));
        let ahead_behind = branch
            .as_deref()
            .and_then(|b| self.workspace.ahead_behind.get(b))
            .copied();
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            picker(ui, "repository", &self.name());
            ui.label(RichText::new("›").color(Color32::from_rgb(0x4a, 0x50, 0x5c)));
            picker(ui, "branch", branch.as_deref().unwrap_or("detached"));
            let spare = (ui.available_width() - TOOLBAR_W) / 2.0;
            ui.add_space(spare.max(0.0));
            ui.spacing_mut().item_spacing.x = 4.0;

            let busy = |label: &str| self.jobs.running().any(|j| j.starts_with(label));
            if tool(ui, Icon::Undo, "Undo", self.can_undo(), &self.undo_hint()) {
                commands.push(Command::Undo);
            }
            divider(ui);
            if tool(
                ui,
                Icon::Fetch,
                "Fetch",
                !busy("Fetching"),
                "git fetch --all --prune",
            ) {
                commands.push(Command::Run(Op::Fetch));
            }
            let pull_hint = match ahead_behind {
                Some((_, behind)) if behind > 0 => format!("{behind} to pull"),
                _ => "git pull".into(),
            };
            let (ahead, behind) = ahead_behind.unwrap_or_default();
            let count = |n: usize, arrow: &str| (n > 0).then(|| format!("{arrow}{n}"));
            if tool_with_badge(
                ui,
                Icon::Pull,
                "Pull",
                branch.is_some() && !busy("Pulling"),
                &pull_hint,
                count(behind, "↓"),
            ) {
                commands.push(Command::Run(Op::Pull));
            }
            let push_hint = match (&upstream, ahead_behind) {
                (None, _) => "No upstream yet: the first push sets one".to_string(),
                (Some(_), Some((ahead, _))) if ahead > 0 => format!("{ahead} to push"),
                _ => "git push".into(),
            };
            if tool_with_badge(
                ui,
                Icon::Push,
                "Push",
                branch.is_some() && !busy("Pushing"),
                &push_hint,
                count(ahead, "↑"),
            ) && let Some(branch) = branch.clone()
            {
                commands.push(Command::Push(branch));
            }
            divider(ui);
            let start = self.selected_ref();
            if tool(
                ui,
                Icon::Branch,
                "Branch",
                true,
                "New branch from the selected commit",
            ) {
                commands.push(Command::Open(Dialog::NewBranch {
                    name: String::new(),
                    start_label: start.1.clone(),
                    start: start.0.clone(),
                    switch: true,
                }));
            }
            if tool(
                ui,
                Icon::Worktree,
                "Worktree",
                self.workdir.is_some(),
                "New worktree",
            ) {
                let ctx = self.menu_context();
                commands.push(Command::Open(Dialog::NewWorktree(NewWorktree::new(
                    ctx.repo_dir_name.to_string(),
                    start.0.clone(),
                    None,
                    ctx.local_branches,
                ))));
            }
            divider(ui);
            if tool(
                ui,
                Icon::Stash,
                "Stash",
                !self.wip.is_empty(),
                "Stash all changes",
            ) {
                commands.push(Command::Run(Op::StashPush));
            }
            if tool(
                ui,
                Icon::Pop,
                "Pop",
                !self.workspace.stashes.is_empty(),
                "Apply and drop the latest stash",
            ) {
                commands.push(Command::Run(Op::StashPop));
            }
        });
    }

    fn selected_ref(&self) -> (String, String) {
        match self.selected {
            Some(Selection::Commit(row)) => {
                let label = self
                    .history
                    .refs
                    .at_row(row)
                    .find(|l| l.kind == RefKind::Local)
                    .map(|l| l.name.clone());
                let id = self.history.id(row).to_string();
                match label {
                    Some(name) => (name.clone(), name),
                    None => (id.clone(), id[..7].to_string()),
                }
            }
            _ => {
                let head = self.current_branch().unwrap_or("HEAD").to_string();
                (head.clone(), head)
            }
        }
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            let branch = self.current_branch().unwrap_or("detached");
            let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
            ui.painter()
                .circle_filled(dot.center(), 3.5, theme::LANES[0]);
            ui.add_space(-12.0);
            ui.label(RichText::new(branch).size(11.0).color(theme::LANES[0]));
            if let Some((ahead, behind)) = self
                .current_branch()
                .and_then(|b| self.workspace.ahead_behind.get(b))
            {
                let sync = match (ahead, behind) {
                    (0, 0) => "in sync with upstream".to_string(),
                    (a, 0) => format!("{a} ahead"),
                    (0, b) => format!("{b} behind"),
                    (a, b) => format!("{a} ahead, {b} behind"),
                };
                ui.label(RichText::new(sync).size(11.0).color(theme::TEXT_MUTED));
            }
            ui.label(
                RichText::new(format!("{} commits", self.history.len()))
                    .size(11.0)
                    .color(theme::TEXT_MUTED),
            );
            ui.label(
                RichText::new(format!(
                    "{} worktrees",
                    self.workspace.worktrees.len().max(1)
                ))
                .size(11.0)
                .color(theme::TEXT_MUTED),
            );
            let time = ui.input(|i| i.time) as f32;
            let mut any_job = false;
            for job in self.jobs.running() {
                any_job = true;
                let (dot, _) = ui.allocate_exact_size(vec2(12.0, 14.0), Sense::hover());
                crate::mascot::bubbles(ui.painter(), dot.center(), time, theme::ACCENT);
                ui.add_space(-12.0);
                ui.label(RichText::new(job).size(11.0).color(theme::ACCENT));
            }
            if any_job {
                ui.ctx().request_repaint_after(Duration::from_millis(33));
            }
            let rect = ui.max_rect();
            ui.painter().text(
                rect.right_center(),
                Align2::RIGHT_CENTER,
                format!(
                    "Kelp {}  ·  loaded in {} ms",
                    env!("CARGO_PKG_VERSION"),
                    self.load_time.as_millis()
                ),
                FontId::proportional(11.0),
                theme::TEXT_FAINT,
            );
        });
    }

    fn toast(&mut self, ui: &mut egui::Ui) {
        let Some(toast) = &self.toast else { return };
        let age = toast.shown_at.elapsed().as_secs_f32();
        let life = if toast.error { 8.0 } else { 3.0 };
        if age > life {
            self.toast = None;
            return;
        }
        ui.ctx()
            .request_repaint_after(Duration::from_secs_f32(life - age));
        let color = if toast.error {
            theme::DELETED
        } else {
            theme::ADDED
        };
        egui::Area::new(egui::Id::new("toast"))
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -40.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x23, 0x28, 0x33))
                    .stroke(Stroke::new(1.0, theme::with_alpha(color, 0x99)))
                    .corner_radius(8)
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(560.0);
                        ui.label(RichText::new(&toast.text).color(theme::TEXT_STRONG));
                    });
            });
    }
}

const TOOLBAR_W: f32 = 8.0 * 60.0 + 3.0 * 16.0;

fn picker(ui: &mut egui::Ui, caption: &str, value: &str) {
    ui.vertical(|ui| {
        ui.add_space(9.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.label(RichText::new(caption).size(11.0).color(theme::TEXT_MUTED));
        ui.label(
            RichText::new(value)
                .size(15.0)
                .family(theme::semibold())
                .color(theme::TEXT_STRONG),
        );
    });
}

fn tool(ui: &mut egui::Ui, icon: Icon, label: &str, enabled: bool, hint: &str) -> bool {
    tool_with_badge(ui, icon, label, enabled, hint, None)
}

fn tool_with_badge(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    enabled: bool,
    hint: &str,
    badge: Option<String>,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        vec2(56.0, 48.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let painter = ui.painter_at(rect);
    if enabled && response.hovered() {
        painter.rect_filled(rect, 6.0, theme::with_alpha(Color32::WHITE, 0x0c));
    }
    let color = if enabled {
        Color32::from_rgb(0xc9, 0xcc, 0xd2)
    } else {
        Color32::from_rgb(0x5b, 0x61, 0x6d)
    };
    let icon_rect = icons::center_square(rect.translate(vec2(0.0, -7.0)), 18.0);
    icons::paint(&painter, icon_rect, icon, color);
    if let Some(text) = badge {
        let galley = painter.layout_no_wrap(
            text,
            FontId::new(10.0, theme::semibold()),
            Color32::from_rgb(0x10, 0x13, 0x1a),
        );
        let size = vec2((galley.size().x + 8.0).max(15.0), 15.0);
        let pill = egui::Rect::from_min_size(
            egui::pos2(icon_rect.right() - 4.0, icon_rect.top() - 6.0),
            size,
        );
        painter.rect_filled(pill, 7.5, theme::ACCENT);
        painter.galley(
            pill.center() - galley.size() / 2.0,
            galley,
            Color32::from_rgb(0x10, 0x13, 0x1a),
        );
    }
    painter.text(
        rect.center_bottom() - vec2(0.0, 9.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.0),
        color,
    );
    enabled && response.on_hover_text(hint).clicked()
}

fn divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(12.0, 30.0), Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        Stroke::new(1.0, Color32::from_rgb(0x2c, 0x31, 0x3b)),
    );
}

pub fn reveal_in_finder(path: &Path) -> std::io::Result<()> {
    let mut command = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg("-R").arg(path);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path.parent().unwrap_or(path));
        c
    };
    command.spawn().map(|_| ())
}

pub fn open_terminal(path: &Path) -> std::io::Result<()> {
    let mut command = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.args(["-a", "Terminal"]).arg(path);
        c
    } else {
        let mut c = std::process::Command::new("x-terminal-emulator");
        c.current_dir(path);
        c
    };
    command.spawn().map(|_| ())
}
