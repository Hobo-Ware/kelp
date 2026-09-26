use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, FontId, Key, Margin, RichText, Stroke};
use kelp_core::commit::{self, Details, FileChange};
use kelp_core::history::History;
use kelp_core::{git_cli, status};

use crate::avatars::AvatarStore;
use crate::dev_bench::ScrollBench;
use crate::dev_screenshot::DevScreenshot;
use crate::diff_view::{DiffSource, DiffView};
use crate::graph_view::{self, GraphView};
use crate::jobs::Jobs;
use crate::{details, sidebar, theme};

type Loaded = anyhow::Result<(gix::Repository, History, Duration)>;

pub struct KelpApp {
    path: PathBuf,
    state: State,
    screenshot: Option<DevScreenshot>,
}

enum State {
    Loading(Receiver<Loaded>),
    Failed(String),
    Ready(Box<Repo>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Wip,
    Commit(usize),
}

pub enum JobOutput {
    Reloaded(anyhow::Result<Box<History>>),
    Status(anyhow::Result<Vec<FileChange>>),
    CommitGraph(anyhow::Result<()>),
}

pub enum Center {
    Graph,
    Diff(Box<DiffView>),
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
    pub graph: GraphView,
    pub center: Center,
    pub jobs: Jobs<JobOutput>,
    pub toast: Option<Toast>,
    pub avatars: AvatarStore,
    was_focused: Option<bool>,
    bench: Option<ScrollBench>,
}

impl KelpApp {
    pub fn open(ctx: egui::Context, path: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        let load_path = path.clone();
        std::thread::spawn(move || {
            let started = Instant::now();
            let result =
                History::open(&load_path).map(|(repo, history)| (repo, history, started.elapsed()));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        Self {
            path,
            state: State::Loading(rx),
            screenshot: DevScreenshot::from_env(),
        }
    }
}

impl Repo {
    fn new(
        ctx: &egui::Context,
        repo: gix::Repository,
        history: History,
        load_time: Duration,
    ) -> Self {
        let workdir = repo.workdir().map(Path::to_path_buf);
        let dir = workdir.clone().unwrap_or_else(|| repo.path().to_path_buf());
        let avatars =
            AvatarStore::new(ctx.clone(), kelp_core::avatar::GitHubRepo::from_repo(&repo));
        let mut ready = Self {
            avatars,
            dir,
            workdir,
            repo,
            history,
            load_time,
            selected: None,
            details: None,
            wip: Vec::new(),
            graph: GraphView::new(),
            center: Center::Graph,
            jobs: Jobs::new(ctx.clone()),
            toast: None,
            was_focused: None,
            bench: ScrollBench::from_env(),
        };
        if let Some(row) = ready.head_row() {
            ready.select(Selection::Commit(row));
        }
        ready.refresh_status();
        if std::env::var_os("KELP_OPEN_DIFF").is_some()
            && let Some(path) = ready
                .details
                .as_ref()
                .and_then(|d| d.changes.first())
                .map(|c| c.path.clone())
        {
            ready.open_diff(&path);
        }
        if !git_cli::has_commit_graph(&ready.repo) {
            let dir = ready.repo.path().to_path_buf();
            ready.jobs.spawn("Speeding up history", move || {
                JobOutput::CommitGraph(git_cli::write_commit_graph(&dir))
            });
        }
        ready
    }

    pub fn head_row(&self) -> Option<usize> {
        self.history.refs.head.and_then(|h| self.history.row(&h))
    }

    pub fn select(&mut self, selection: Selection) {
        if self.selected == Some(selection) {
            return;
        }
        self.selected = Some(selection);
        self.details = match selection {
            Selection::Commit(row) => commit::details(&self.repo, self.history.id(row)).ok(),
            Selection::Wip => None,
        };
    }

    pub fn reveal(&mut self, selection: Selection) {
        self.select(selection);
        self.graph.scroll_to = Some(selection);
    }

    pub fn open_diff(&mut self, path: &str) {
        let source = match self.selected {
            Some(Selection::Wip) => DiffSource::Working,
            Some(Selection::Commit(row)) => DiffSource::Commit(self.history.id(row)),
            None => return,
        };
        match DiffView::load(&self.repo, self.workdir.as_deref(), source, path) {
            Ok(view) => self.center = Center::Diff(Box::new(view)),
            Err(e) => self.notify(format!("Could not open diff: {e:#}"), true),
        }
    }

    pub fn notify(&mut self, text: impl Into<String>, error: bool) {
        self.toast = Some(Toast {
            text: text.into(),
            error,
            shown_at: Instant::now(),
        });
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

    pub fn reload(&mut self) {
        if self.jobs.is_running("Reloading") {
            return;
        }
        let dir = self.dir.clone();
        self.jobs.spawn("Reloading", move || {
            JobOutput::Reloaded(History::open(&dir).map(|(_, history)| Box::new(history)))
        });
    }

    fn poll_jobs(&mut self) {
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
                JobOutput::Reloaded(Ok(history)) => self.replace_history(*history),
                JobOutput::Reloaded(Err(e)) => self.notify(format!("Reload failed: {e:#}"), true),
                JobOutput::CommitGraph(Ok(())) => {}
                JobOutput::CommitGraph(Err(e)) => {
                    self.notify(format!("Could not write commit-graph: {e:#}"), true)
                }
            }
        }
    }

    fn replace_history(&mut self, history: History) {
        let selected_id = match self.selected {
            Some(Selection::Commit(row)) => Some(self.history.id(row)),
            _ => None,
        };
        self.history = history;
        self.graph.clear_cache();
        let keep = self.selected == Some(Selection::Wip);
        self.selected = None;
        if keep {
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
}

impl eframe::App for KelpApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let State::Loading(rx) = &self.state
            && let Ok(result) = rx.try_recv()
        {
            self.state = match result {
                Ok((repo, history, load_time)) => {
                    State::Ready(Box::new(Repo::new(ui.ctx(), repo, history, load_time)))
                }
                Err(e) => State::Failed(format!("{e:#}")),
            };
        }

        if let Some(shot) = &mut self.screenshot {
            shot.tick(ui.ctx(), matches!(self.state, State::Ready(_)));
        }

        match &mut self.state {
            State::Loading(_) => centered(
                ui,
                &format!("Loading {}…", self.path.display()),
                theme::TEXT_MUTED,
            ),
            State::Failed(err) => centered(
                ui,
                &format!("Could not open {}: {err}", self.path.display()),
                theme::DELETED,
            ),
            State::Ready(repo) => {
                repo.poll_jobs();
                repo.watch_focus(ui.ctx());
                ready_ui(ui, repo, &self.path);
            }
        }
    }
}

fn centered(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.centered_and_justified(|ui| ui.label(RichText::new(text).color(color)));
    });
}

fn ready_ui(ui: &mut egui::Ui, repo: &mut Repo, path: &Path) {
    handle_keys(ui, repo);

    egui::Panel::top("toolbar")
        .exact_size(52.0)
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .inner_margin(Margin::symmetric(16, 0))
                .stroke(Stroke::new(1.0, theme::BORDER)),
        )
        .show(ui, |ui| toolbar(ui, repo, path));

    egui::Panel::bottom("status")
        .exact_size(26.0)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(0x0f, 0x11, 0x15))
                .inner_margin(Margin::symmetric(14, 0)),
        )
        .show(ui, |ui| status_bar(ui, repo));

    egui::Panel::left("sidebar")
        .default_size(250.0)
        .min_size(180.0)
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .stroke(Stroke::new(1.0, theme::BORDER)),
        )
        .show(ui, |ui| sidebar::ui(ui, repo));

    egui::Panel::right("details")
        .default_size(370.0)
        .min_size(260.0)
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .stroke(Stroke::new(1.0, theme::BORDER)),
        )
        .show(ui, |ui| details::ui(ui, repo));

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BG))
        .show(ui, |ui| match &mut repo.center {
            Center::Diff(view) => {
                if view.ui(ui) {
                    repo.center = Center::Graph;
                }
            }
            Center::Graph => graph_center(ui, repo),
        });

    toast(ui, repo);
}

fn graph_center(ui: &mut egui::Ui, repo: &mut Repo) {
    let head_row = repo.head_row();
    let wip = (!repo.wip.is_empty()).then(|| graph_view::Wip {
        head_row: head_row.unwrap_or(0),
        changes: &repo.wip,
    });
    let Repo {
        repo: git,
        history,
        selected,
        graph,
        bench,
        avatars,
        ..
    } = repo;
    if let Some(bench) = bench {
        graph.scroll_to = Some(Selection::Commit(bench.next_row(history.len())));
    }
    let started = Instant::now();
    let action = graph.ui(ui, git, history, *selected, wip, avatars);
    if let Some(bench) = bench {
        bench.record(ui.ctx(), started.elapsed());
    }
    if let Some(graph_view::Action::Select(selection)) = action {
        repo.select(selection);
    }
}

fn handle_keys(ui: &egui::Ui, repo: &mut Repo) {
    if ui.ctx().egui_wants_keyboard_input() || repo.history.is_empty() {
        return;
    }
    if matches!(repo.center, Center::Diff(_)) {
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            repo.center = Center::Graph;
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
    let has_wip = !repo.wip.is_empty();
    let head = repo.head_row().unwrap_or(0);
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
    let total = repo.history.len() + has_wip as usize;
    let current = repo.selected.map(order).unwrap_or(0);
    let next = if down {
        (current + 1).min(total - 1)
    } else {
        current.saturating_sub(1)
    };
    repo.reveal(from_order(next));
}

fn toolbar(ui: &mut egui::Ui, repo: &Repo, path: &Path) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let branch = repo
        .history
        .refs
        .head_branch
        .clone()
        .unwrap_or_else(|| "detached".into());
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        picker(ui, "repository", &name);
        ui.label(RichText::new("›").color(egui::Color32::from_rgb(0x4a, 0x50, 0x5c)));
        picker(ui, "branch", &branch);
    });
}

fn picker(ui: &mut egui::Ui, caption: &str, value: &str) {
    ui.vertical(|ui| {
        ui.add_space(8.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.label(RichText::new(caption).size(11.0).color(theme::TEXT_MUTED));
        ui.label(
            RichText::new(value)
                .size(15.0)
                .strong()
                .color(theme::TEXT_STRONG),
        );
    });
}

fn status_bar(ui: &mut egui::Ui, repo: &Repo) {
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        let branch = repo
            .history
            .refs
            .head_branch
            .as_deref()
            .unwrap_or("detached");
        let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        ui.painter()
            .circle_filled(dot.center(), 3.5, theme::LANES[0]);
        ui.add_space(-12.0);
        ui.label(RichText::new(branch).size(11.0).color(theme::LANES[0]));
        ui.label(
            RichText::new(format!("{} commits", repo.history.len()))
                .size(11.0)
                .color(theme::TEXT_MUTED),
        );
        ui.label(
            RichText::new(format!("loaded in {} ms", repo.load_time.as_millis()))
                .size(11.0)
                .color(theme::TEXT_MUTED),
        );
        for job in repo.jobs.running() {
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
            format!("Kelp {}", env!("CARGO_PKG_VERSION")),
            FontId::proportional(11.0),
            theme::ACCENT,
        );
    });
}

const TOAST_SECONDS: f32 = 5.0;

fn toast(ui: &mut egui::Ui, repo: &mut Repo) {
    let Some(toast) = &repo.toast else { return };
    let age = toast.shown_at.elapsed().as_secs_f32();
    if age > TOAST_SECONDS {
        repo.toast = None;
        return;
    }
    ui.ctx()
        .request_repaint_after(Duration::from_secs_f32(TOAST_SECONDS - age));
    let color = if toast.error {
        theme::DELETED
    } else {
        theme::ADDED
    };
    egui::Area::new(egui::Id::new("toast"))
        .anchor(Align2::CENTER_BOTTOM, egui::vec2(0.0, -40.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(0x23, 0x28, 0x33))
                .stroke(Stroke::new(1.0, theme::with_alpha(color, 0x99)))
                .corner_radius(8)
                .inner_margin(Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.set_max_width(520.0);
                    ui.label(RichText::new(&toast.text).color(theme::TEXT_STRONG));
                });
        });
}
