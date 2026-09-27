use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, FontId, Key, Margin, RichText, Sense, Stroke, vec2};
use kelp_core::commit::{self, Details, FileChange};
use kelp_core::history::History;
use kelp_core::ops::Op;
use kelp_core::refs::RefKind;
use kelp_core::review::{self, Review};
use kelp_core::workspace::{self, Stash, Worktree};
use kelp_core::{git_cli, status};

use crate::avatars::AvatarStore;
use crate::commands::Command;
use crate::dev_bench::ScrollBench;
use crate::dialogs::{self, Dialog, NewWorktree, Outcome};
use crate::diff_view::{self, DiffSource, DiffView};
use crate::graph_view::{self, GraphView};
use crate::icons::{self, Icon};
use crate::jobs::Jobs;
use crate::menus::{self, MenuContext};
use crate::settings::Settings;
use crate::{details, sidebar, theme, worktrees_view};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Wip,
    Commit(usize),
}

pub enum JobOutput {
    Reloaded(anyhow::Result<Box<History>>),
    Status(anyhow::Result<Vec<FileChange>>),
    CommitGraph(anyhow::Result<()>),
    Workspace(Box<WorkspaceInfo>),
    Op {
        label: String,
        result: anyhow::Result<String>,
    },
    Search {
        query: String,
        rows: anyhow::Result<Vec<usize>>,
    },
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
    Worktrees,
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
        let author = review::author_name(&repo);
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
            bench: ScrollBench::from_env(),
        };
        if let Some(row) = ready.head_row() {
            ready.select(Selection::Commit(row));
        }
        ready.refresh_status();
        ready.refresh_workspace();
        if std::env::var_os("KELP_OPEN_DIFF").is_some()
            && let Some(path) = ready
                .details
                .as_ref()
                .and_then(|d| d.changes.first())
                .map(|c| c.path.clone())
        {
            ready.open_diff(&path);
            if std::env::var("KELP_OPEN_DIFF").as_deref() == Ok("split")
                && let Center::Diff(view) = &mut ready.center
            {
                view.show_split();
            }
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
            Some(Selection::Wip) => DiffSource::Working,
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
            Ok(view) => self.center = Center::Diff(Box::new(view)),
            Err(e) => self.notify(format!("Could not open file: {e:#}"), true),
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
            upstream: self
                .current_branch()
                .and_then(|b| workspace::upstream_remote(&self.repo, b)),
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
                Command::Open(dialog) => self.dialog = Some(dialog),
                Command::Copy(text) => {
                    ctx.copy_text(text.clone());
                    self.notify(format!("Copied {text}"), false);
                }
                Command::Reveal(selection) => self.reveal(selection),
                Command::ShowWorktrees => self.center = Center::Worktrees,
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
            }
        }
    }

    pub fn run_op(&mut self, op: Op) {
        let label = op.label();
        let dir = self.dir.clone();
        let job_label = label.clone();
        self.jobs.spawn(label, move || JobOutput::Op {
            label: job_label,
            result: op.run(&dir),
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
            return;
        }
        self.jobs.spawn("Checking changes", move || {
            JobOutput::Status(status::working_changes(&workdir))
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
            return;
        }
        let dir = self.dir.clone();
        self.jobs.spawn("Reloading", move || {
            JobOutput::Reloaded(History::open(&dir).map(|(_, history)| Box::new(history)))
        });
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        self.avatars.poll();
        for output in self.jobs.finished() {
            match output {
                JobOutput::Status(Ok(changes)) => {
                    self.wip = changes;
                    if self.wip.is_empty() && self.selected == Some(Selection::Wip) {
                        self.selected = None;
                        if let Some(row) = self.head_row() {
                            self.select(Selection::Commit(row));
                        }
                    }
                }
                JobOutput::Status(Err(e)) => self.notify(format!("{e:#}"), true),
                JobOutput::Reloaded(Ok(history)) => {
                    self.replace_history(*history);
                    self.refresh_workspace();
                }
                JobOutput::Reloaded(Err(e)) => self.notify(format!("Reload failed: {e:#}"), true),
                JobOutput::CommitGraph(Ok(())) => {}
                JobOutput::CommitGraph(Err(e)) => {
                    self.notify(format!("Could not write commit-graph: {e:#}"), true)
                }
                JobOutput::Workspace(info) => self.workspace = *info,
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
                JobOutput::Op { label, result } => {
                    match result {
                        Ok(_) => {
                            self.notify(format!("{} done", label), false);
                            self.outbox.append(&mut self.open_after_ops);
                        }
                        Err(e) => {
                            self.open_after_ops.clear();
                            self.notify(format!("{e:#}"), true);
                        }
                    }
                    self.reload();
                    self.refresh_status();
                }
            }
        }
        self.watch_focus(ctx);
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

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BG))
            .show(ui, |ui| match &mut self.center {
                Center::Diff(view) => match view.ui(ui, &mut self.review, &self.author) {
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
                    diff_view::Event::None => {}
                },
                Center::Worktrees => worktrees_view::ui(ui, self, &mut commands),
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
                menus::commit(ui, &history.id(row).to_string(), title, &menu_ctx, commands)
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
            if tool(
                ui,
                Icon::Pull,
                "Pull",
                branch.is_some() && !busy("Pulling"),
                &pull_hint,
            ) {
                commands.push(Command::Run(Op::Pull));
            }
            let push_hint = match (&upstream, ahead_behind) {
                (None, _) => "No upstream yet: pushes to origin and sets it".to_string(),
                (Some(_), Some((ahead, _))) if ahead > 0 => format!("{ahead} to push"),
                _ => "git push".into(),
            };
            if tool(
                ui,
                Icon::Push,
                "Push",
                branch.is_some() && !busy("Pushing"),
                &push_hint,
            ) && let Some(branch) = branch.clone()
            {
                commands.push(Command::Run(Op::Push {
                    branch,
                    remote: upstream.clone(),
                }));
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
            for job in self.jobs.running() {
                ui.label(
                    RichText::new(format!("{job}…"))
                        .size(11.0)
                        .color(theme::ACCENT),
                );
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

const TOOLBAR_W: f32 = 7.0 * 60.0 + 2.0 * 16.0;

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
