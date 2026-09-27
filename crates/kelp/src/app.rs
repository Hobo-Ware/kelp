use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, FontId, Margin, RichText, Sense, Stroke, vec2};
use kelp_core::history::History;

use crate::dev_screenshot::DevScreenshot;
use crate::repo_view::Repo;
use crate::settings::Settings;
use crate::theme;

type Loaded = anyhow::Result<(gix::Repository, History, Duration)>;

pub struct KelpApp {
    tabs: Vec<Tab>,
    active: usize,
    screenshot: Option<DevScreenshot>,
    settings: Settings,
    show_settings: bool,
}

struct Tab {
    path: PathBuf,
    state: State,
}

enum State {
    Loading(Receiver<Loaded>),
    Failed(String),
    Ready(Box<Repo>),
}

impl Tab {
    fn open(ctx: &egui::Context, path: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        let load_path = path.clone();
        let ctx = ctx.clone();
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
        }
    }

    fn title(&self) -> String {
        match &self.state {
            State::Ready(repo) => repo.name(),
            _ => self
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| self.path.display().to_string()),
        }
    }

    fn same_repo(&self, path: &Path) -> bool {
        let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let target = canonical(path);
        match &self.state {
            State::Ready(repo) => canonical(&repo.dir) == target,
            _ => canonical(&self.path) == target,
        }
    }
}

impl KelpApp {
    pub fn open(ctx: egui::Context, paths: Vec<PathBuf>) -> Self {
        let tabs = paths.into_iter().map(|p| Tab::open(&ctx, p)).collect();
        Self {
            tabs,
            active: 0,
            screenshot: DevScreenshot::from_env(),
            settings: Settings::load(),
            show_settings: std::env::var_os("KELP_OPEN_SETTINGS").is_some(),
        }
    }

    fn remember_tabs(&mut self) {
        let tabs: Vec<PathBuf> = self
            .tabs
            .iter()
            .map(|t| match &t.state {
                State::Ready(repo) => repo.dir.clone(),
                _ => t.path.clone(),
            })
            .collect();
        if tabs != self.settings.open_tabs && self.screenshot.is_none() {
            self.settings.open_tabs = tabs;
            self.settings.save();
        }
    }

    fn open_tab(&mut self, ctx: &egui::Context, path: PathBuf) {
        if let Some(i) = self.tabs.iter().position(|t| t.same_repo(&path)) {
            self.active = i;
            return;
        }
        self.tabs.push(Tab::open(ctx, path));
        self.active = self.tabs.len() - 1;
    }

    fn tab_strip(&mut self, ui: &mut egui::Ui) {
        let mut close = None;
        let mut pick_folder = false;
        egui::Panel::top("tabs")
            .exact_size(36.0)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x0f, 0x11, 0x15))
                    .inner_margin(Margin::symmetric(10, 0)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    kelp_mark(ui);
                    ui.add_space(10.0);
                    for (i, tab) in self.tabs.iter().enumerate() {
                        let active = i == self.active;
                        let title = tab.title();
                        let galley = ui.painter().layout_no_wrap(
                            title,
                            FontId::proportional(13.0),
                            theme::TEXT,
                        );
                        let w = galley.size().x + 50.0;
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(w, 30.0), Sense::click());
                        let painter = ui.painter_at(rect);
                        if active {
                            painter.rect_filled(
                                rect,
                                egui::CornerRadius {
                                    nw: 6,
                                    ne: 6,
                                    sw: 0,
                                    se: 0,
                                },
                                theme::PANEL,
                            );
                        } else if response.hovered() {
                            painter.rect_filled(rect, 6.0, theme::with_alpha(Color32::WHITE, 0x08));
                        }
                        let color = if active {
                            theme::TEXT_STRONG
                        } else {
                            theme::TEXT_MUTED
                        };
                        painter.galley(
                            egui::pos2(rect.left() + 14.0, rect.center().y - galley.size().y / 2.0),
                            galley,
                            color,
                        );
                        let x_rect = egui::Rect::from_center_size(
                            egui::pos2(rect.right() - 16.0, rect.center().y),
                            vec2(18.0, 18.0),
                        );
                        let x_response =
                            ui.interact(x_rect, ui.id().with(("close-tab", i)), Sense::click());
                        if x_response.hovered() {
                            painter.rect_filled(
                                x_rect,
                                4.0,
                                theme::with_alpha(Color32::WHITE, 0x14),
                            );
                        }
                        painter.text(
                            x_rect.center(),
                            Align2::CENTER_CENTER,
                            "×",
                            FontId::proportional(14.0),
                            theme::TEXT_FAINT,
                        );
                        if x_response.clicked() {
                            close = Some(i);
                        } else if response.clicked() {
                            self.active = i;
                        }
                        response.on_hover_text(tab.path.display().to_string());
                    }
                    ui.add_space(6.0);
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("+").size(16.0).color(theme::TEXT_MUTED),
                            )
                            .frame(false),
                        )
                        .on_hover_text("Open a repository")
                        .clicked()
                    {
                        pick_folder = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let settings = egui::Button::new(
                            RichText::new("Settings")
                                .size(12.0)
                                .color(theme::TEXT_MUTED),
                        )
                        .frame(false);
                        if ui.add(settings).clicked() {
                            self.show_settings = true;
                        }
                    });
                });
            });
        if let Some(i) = close {
            self.tabs.remove(i);
            if self.active >= self.tabs.len() {
                self.active = self.tabs.len().saturating_sub(1);
            } else if i < self.active {
                self.active -= 1;
            }
        }
        if pick_folder
            && let Some(folder) = rfd::FileDialog::new()
                .set_title("Open a repository")
                .pick_folder()
        {
            self.open_tab(ui.ctx(), folder);
        }
    }
}

fn kelp_mark(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
    let p = |x: f32, y: f32| rect.min + vec2(x, y) * (18.0 / 56.0);
    let stroke = Stroke::new(2.0, theme::ACCENT);
    ui.painter()
        .line_segment([p(18.0, 8.0), p(18.0, 48.0)], stroke);
    ui.painter()
        .line_segment([p(38.0, 16.0), p(38.0, 26.0)], stroke);
    let curve: Vec<egui::Pos2> = (0..=10)
        .map(|i| {
            let t = i as f32 / 10.0;
            let mt = 1.0 - t;
            let x = mt.powi(3) * 38.0
                + 3.0 * mt * mt * t * 38.0
                + 3.0 * mt * t * t * 18.0
                + t.powi(3) * 18.0;
            let y = mt.powi(3) * 26.0
                + 3.0 * mt * mt * t * 34.0
                + 3.0 * mt * t * t * 32.0
                + t.powi(3) * 40.0;
            p(x, y)
        })
        .collect();
    ui.painter().add(egui::Shape::line(curve, stroke));
    ui.add_space(4.0);
    ui.label(
        RichText::new("Kelp")
            .size(14.0)
            .family(theme::semibold())
            .color(theme::TEXT_STRONG),
    );
}

impl eframe::App for KelpApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        for tab in &mut self.tabs {
            if let State::Loading(rx) = &tab.state
                && let Ok(result) = rx.try_recv()
            {
                tab.state = match result {
                    Ok((repo, history, load_time)) => {
                        State::Ready(Box::new(Repo::new(&ctx, repo, history, load_time)))
                    }
                    Err(e) => State::Failed(format!("{e:#}")),
                };
            }
        }
        let active_ready = self
            .tabs
            .get(self.active)
            .is_some_and(|t| matches!(t.state, State::Ready(_)));
        if let Some(shot) = &mut self.screenshot {
            shot.tick(&ctx, active_ready);
        }

        self.tab_strip(ui);

        let mut open = Vec::new();
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            if let State::Ready(repo) = &mut tab.state {
                repo.avatars.enabled = self.settings.load_avatars;
                repo.poll(&ctx);
                if i != self.active {
                    open.append(&mut repo.outbox);
                }
            }
        }
        match self
            .tabs
            .get_mut(self.active)
            .map(|t| (&t.path, &mut t.state))
        {
            None => empty(ui),
            Some((path, State::Loading(_))) => centered(
                ui,
                &format!("Loading {}…", path.display()),
                theme::TEXT_MUTED,
            ),
            Some((path, State::Failed(err))) => centered(
                ui,
                &format!("Could not open {}: {err}", path.display()),
                theme::DELETED,
            ),
            Some((_, State::Ready(repo))) => {
                repo.ui(ui, &self.settings);
                open.append(&mut repo.outbox);
            }
        }
        for path in open {
            self.open_tab(&ctx, path);
        }
        self.remember_tabs();
        if self.show_settings {
            self.settings.window(&ctx, &mut self.show_settings);
        }
    }
}

fn centered(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.centered_and_justified(|ui| ui.label(RichText::new(text).color(color)));
    });
}

fn empty(ui: &mut egui::Ui) {
    centered(
        ui,
        "Open a repository with the + button, or run: kelp <path>",
        theme::TEXT_MUTED,
    );
}
