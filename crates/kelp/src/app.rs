use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, FontId, Key, Margin, RichText, Stroke};
use kelp_core::commit::{self, Details};
use kelp_core::history::History;

use crate::dev_bench::ScrollBench;
use crate::dev_screenshot::DevScreenshot;
use crate::graph_view::{self, GraphView};
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

pub struct Repo {
    pub repo: gix::Repository,
    pub history: History,
    pub load_time: Duration,
    pub selected: Option<usize>,
    pub details: Option<Details>,
    pub graph: GraphView,
    pub bench: Option<ScrollBench>,
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
    pub fn select(&mut self, row: usize) {
        if self.selected == Some(row) {
            return;
        }
        self.selected = Some(row);
        self.details = commit::details(&self.repo, self.history.id(row)).ok();
    }

    pub fn reveal(&mut self, row: usize) {
        self.select(row);
        self.graph.scroll_to = Some(row);
    }
}

impl eframe::App for KelpApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let State::Loading(rx) = &self.state
            && let Ok(result) = rx.try_recv()
        {
            self.state = match result {
                Ok((repo, history, load_time)) => {
                    let mut ready = Repo {
                        repo,
                        history,
                        load_time,
                        selected: None,
                        details: None,
                        graph: GraphView::new(),
                        bench: ScrollBench::from_env(),
                    };
                    if let Some(row) = ready.history.refs.head.and_then(|h| ready.history.row(&h)) {
                        ready.select(row);
                    }
                    State::Ready(Box::new(ready))
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
            State::Ready(repo) => ready_ui(ui, repo, &self.path),
        }
    }
}

fn centered(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.centered_and_justified(|ui| ui.label(RichText::new(text).color(color)));
    });
}

fn ready_ui(ui: &mut egui::Ui, repo: &mut Repo, path: &std::path::Path) {
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
        .show(ui, |ui| {
            let Repo {
                repo: git,
                history,
                selected,
                graph,
                bench,
                ..
            } = repo;
            if let Some(bench) = bench {
                graph.scroll_to = Some(bench.next_row(history.len()));
            }
            let started = Instant::now();
            let action = graph.ui(ui, git, history, *selected);
            if let Some(bench) = bench {
                bench.record(ui.ctx(), started.elapsed());
            }
            if let Some(graph_view::Action::Select(row)) = action {
                repo.select(row);
            }
        });
}

fn handle_keys(ui: &egui::Ui, repo: &mut Repo) {
    if ui.ctx().egui_wants_keyboard_input() || repo.history.is_empty() {
        return;
    }
    let (down, up) = ui.input(|i| {
        (
            i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::J),
            i.key_pressed(Key::ArrowUp) || i.key_pressed(Key::K),
        )
    });
    let current = repo.selected.unwrap_or(0);
    let last = repo.history.len() - 1;
    if down {
        repo.reveal((current + 1).min(last));
    } else if up {
        repo.reveal(current.saturating_sub(1));
    }
}

fn toolbar(ui: &mut egui::Ui, repo: &Repo, path: &std::path::Path) {
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
