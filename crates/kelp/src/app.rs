use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, FontId, Margin, RichText, Sense, Stroke, vec2};
use kelp_core::history::History;

use crate::clone::{self, CloneDialog};
use crate::dev_screenshot::DevScreenshot;
use crate::menus::{self, TabAction};
use crate::palette::Palette;
use crate::panels::{self, Side};
use crate::recents;
use crate::repo_view::{self, Repo};
use crate::settings::{self, Settings};
use crate::welcome::{self, Welcome};
use crate::{theme, window};

mod palette_glue;

type Loaded = anyhow::Result<(gix::Repository, History, Duration)>;

pub struct KelpApp {
    tabs: Vec<Tab>,
    active: usize,
    screenshot: Option<DevScreenshot>,
    settings: Settings,
    show_settings: bool,
    titlebar_unified: bool,
    settings_unsaved: bool,
    home: bool,
    welcome: Welcome,
    clone: Option<CloneDialog>,
    notice: Option<(String, Instant)>,
    window: window::Tracker,
    started: Instant,
    ctx: egui::Context,
    updater: crate::updater::Updater,
    palette: Palette,
    shortcuts_open: bool,
    signing: crate::signing_panel::SigningPanel,
    instance: Option<crate::instance::Listener>,
    applied_theme: Option<egui::ThemePreference>,
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

    fn dir(&self) -> PathBuf {
        match &self.state {
            State::Ready(repo) => repo.dir.clone(),
            _ => self.path.clone(),
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
        let settings = Settings::load();
        let palette = palette_from_env(&settings);
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        ctx.set_zoom_factor(settings.zoom);
        let mut welcome = Welcome::default();
        welcome.reopen();
        let clone = (std::env::var("KELP_OPEN_DIALOG").as_deref() == Ok("clone"))
            .then(|| CloneDialog::new(settings.recent_repos.first().map(|r| r.path.as_path())));
        Self {
            tabs,
            active: 0,
            screenshot: DevScreenshot::from_env(),
            show_settings: std::env::var_os("KELP_OPEN_SETTINGS").is_some(),
            titlebar_unified: false,
            settings_unsaved: false,
            home: std::env::var_os("KELP_OPEN_WELCOME").is_some(),
            welcome,
            clone,
            notice: None,
            settings,
            window: window::Tracker::default(),
            started: Instant::now(),
            updater: crate::updater::Updater::new(ctx.clone()),
            palette,
            shortcuts_open: std::env::var_os("KELP_OPEN_SHORTCUTS").is_some(),
            signing: crate::signing_panel::SigningPanel::default(),
            instance: (!crate::settings::is_dev_run())
                .then(|| {
                    crate::instance::Listener::start(&crate::instance::socket_path(), ctx.clone())
                })
                .flatten(),
            applied_theme: None,
            ctx: ctx.clone(),
        }
    }

    fn remember_tabs(&mut self) {
        let tabs: Vec<PathBuf> = self.tabs.iter().map(Tab::dir).collect();
        if tabs != self.settings.open_tabs && !settings::is_dev_run() {
            self.settings.open_tabs = tabs;
            self.settings.save();
        }
    }

    fn remember_window(&mut self, ctx: &egui::Context) {
        if settings::is_dev_run() {
            return;
        }
        if let Some(geometry) = self.window.settled_change(ctx, self.settings.window) {
            self.settings.window = Some(geometry);
            self.settings.save();
        }
    }

    fn save_when_settled(&mut self, ctx: &egui::Context) {
        if !self.settings_unsaved || ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        self.settings_unsaved = false;
        self.settings.save();
    }

    fn remember_recent(&mut self, dir: &Path) {
        recents::remember(&mut self.settings.recent_repos, dir, recents::now());
        self.settings.save();
    }

    fn show_home(&mut self) {
        self.home = true;
        self.welcome.reopen();
    }

    fn notify(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), Instant::now()));
    }

    fn set_zoom(&mut self, ctx: &egui::Context, zoom: f32) {
        let zoom = panels::clamp_zoom(zoom);
        ctx.set_zoom_factor(zoom);
        self.settings.zoom = zoom;
        self.settings.save();
        self.notify(format!("Zoom {:.0}%", zoom * 100.0));
    }

    fn toggle_panel(&mut self, side: Side) {
        self.settings.panels.toggle(side);
        self.settings.save();
    }

    fn run_welcome(&mut self, ctx: &egui::Context, action: welcome::Action) {
        match action {
            welcome::Action::Open(path) => self.open_tab(ctx, path),
            welcome::Action::PickFolder => self.pick_folder(),
            welcome::Action::Clone => {
                let recent = self.settings.recent_repos.first().map(|r| r.path.clone());
                self.clone = Some(CloneDialog::new(recent.as_deref()));
            }
            welcome::Action::Init => self.init_repository(ctx),
            welcome::Action::Forget(path) => {
                recents::forget(&mut self.settings.recent_repos, &path);
                self.settings.save();
            }
            welcome::Action::Reveal(path) => {
                if let Err(e) = repo_view::reveal_in_finder(&path) {
                    self.notify(format!("Could not reveal {}: {e}", path.display()));
                }
            }
        }
    }

    fn init_repository(&mut self, ctx: &egui::Context) {
        let Some(dir) = rfd::FileDialog::new()
            .set_title("Choose a folder for the new repository")
            .pick_folder()
        else {
            return;
        };
        match clone::init_repo(&dir) {
            Ok(()) => self.open_tab(ctx, dir),
            Err(e) => self.notify(format!("Could not create a repository: {e:#}")),
        }
    }

    fn paint_notice(&mut self, ctx: &egui::Context) {
        let Some((text, shown)) = &self.notice else {
            return;
        };
        let left = NOTICE_FOR.saturating_sub(shown.elapsed());
        if left.is_zero() {
            self.notice = None;
            return;
        }
        ctx.request_repaint_after(left);
        egui::Area::new(egui::Id::new("app-notice"))
            .anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -40.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::toast())
                    .stroke(Stroke::new(1.0, theme::popup_border()))
                    .corner_radius(8)
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.label(RichText::new(text.as_str()).color(theme::text_strong()));
                    });
            });
    }

    fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        self.tabs.remove(i);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        } else if i < self.active {
            self.active -= 1;
        }
    }

    fn close_other_tabs(&mut self, keep: usize) {
        if keep < self.tabs.len() {
            let tab = self.tabs.swap_remove(keep);
            self.tabs = vec![tab];
            self.active = 0;
        }
    }

    fn run_tab_action(&mut self, i: usize, action: TabAction) {
        let Some(dir) = self.tabs.get(i).map(Tab::dir) else {
            return;
        };
        match action {
            TabAction::Reveal => {
                let _ = repo_view::reveal_in_finder(&dir);
            }
            TabAction::CopyPath => self.ctx.copy_text(dir.display().to_string()),
            TabAction::Close => self.close_tab(i),
            TabAction::CloseOthers => self.close_other_tabs(i),
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, KeyboardShortcut, Modifiers};
        const DIGITS: [Key; 9] = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
            Key::Num9,
        ];
        let cmd = |key| KeyboardShortcut::new(Modifiers::COMMAND, key);
        let back = KeyboardShortcut::new(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab);
        let forward = KeyboardShortcut::new(Modifiers::CTRL, Key::Tab);
        let cmd_shift = |key| KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, key);
        let (palette, sheet, settings) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&cmd(Key::K)) | i.consume_shortcut(&cmd_shift(Key::P)),
                i.consume_shortcut(&cmd(Key::Slash)),
                i.consume_shortcut(&cmd(Key::Comma)),
            )
        });
        if palette {
            self.palette.show("");
        }
        if sheet {
            self.shortcuts_open = !self.shortcuts_open;
        }
        if settings {
            self.show_settings = true;
        }
        let cmd_alt = |key| KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::ALT, key);
        let (panel, zoom) = ctx.input_mut(|i| {
            let panel = if i.consume_shortcut(&cmd_alt(Key::S)) {
                Some(Side::Sidebar)
            } else if i.consume_shortcut(&cmd_alt(Key::D)) {
                Some(Side::Details)
            } else {
                None
            };
            let zoom =
                if i.consume_shortcut(&cmd(Key::Plus)) | i.consume_shortcut(&cmd(Key::Equals)) {
                    Some(1)
                } else if i.consume_shortcut(&cmd(Key::Minus)) {
                    Some(-1)
                } else if i.consume_shortcut(&cmd(Key::Num0)) {
                    Some(0)
                } else {
                    None
                };
            (panel, zoom)
        });
        if let Some(side) = panel {
            self.toggle_panel(side);
        }
        if let Some(direction) = zoom {
            let next = panels::zoom_step(self.settings.zoom, direction);
            self.set_zoom(ctx, next);
        }
        let (new_tab, open, close, refresh, step, number) = ctx.input_mut(|i| {
            let new_tab = i.consume_shortcut(&cmd(Key::T));
            let open = i.consume_shortcut(&cmd(Key::O));
            let close = i.consume_shortcut(&cmd(Key::W));
            let refresh = i.consume_shortcut(&cmd(Key::R));
            let step = if i.consume_shortcut(&back) {
                -1
            } else if i.consume_shortcut(&forward) {
                1
            } else {
                0
            };
            let number = DIGITS
                .iter()
                .position(|&key| i.consume_shortcut(&cmd(key)))
                .map(|n| n + 1);
            (new_tab, open, close, refresh, step, number)
        });
        if let Some(i) = number.and_then(|n| tab_for_number(n, self.tabs.len())) {
            self.active = i;
            self.home = false;
        }
        if step != 0 {
            self.active = cycled(self.active, self.tabs.len(), step);
            self.home = false;
        }
        if close {
            if self.home && !self.tabs.is_empty() {
                self.home = false;
            } else {
                self.close_tab(self.active);
            }
        }
        if new_tab {
            self.show_home();
        }
        if self.home
            && !self.tabs.is_empty()
            && self.clone.is_none()
            && ctx.input(|i| i.key_pressed(Key::Escape))
        {
            self.home = false;
        }
        if refresh
            && let Some(Tab {
                state: State::Ready(repo),
                ..
            }) = self.tabs.get_mut(self.active)
        {
            repo.refresh_status();
            repo.reload();
            repo.refresh_workspace();
        }
        if open {
            self.pick_folder();
        }
        self.run_global_shortcuts(ctx);
    }

    fn run_global_shortcuts(&mut self, ctx: &egui::Context) {
        if self.palette.open || self.home || self.show_settings {
            return;
        }
        let pressed: Vec<crate::actions::RepoAction> = ctx.input_mut(|i| {
            crate::actions::GLOBAL_SHORTCUTS
                .iter()
                .filter_map(|id| crate::actions::ACTIONS.iter().find(|a| a.id == *id))
                .filter_map(|action| {
                    let shortcut = crate::actions::parse_shortcut(action.shortcut?)?;
                    match action.run {
                        crate::actions::Run::Repo(repo) if i.consume_shortcut(&shortcut) => {
                            Some(repo)
                        }
                        _ => None,
                    }
                })
                .collect()
        });
        let Some(Tab {
            state: State::Ready(repo),
            ..
        }) = self.tabs.get_mut(self.active)
        else {
            return;
        };
        for action in pressed {
            let allowed = repo
                .action_state()
                .allows(crate::actions::Run::Repo(action));
            if allowed && repo.dialog.is_none() {
                repo.run_action(ctx, action);
            }
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        for path in dropped {
            if let Some(target) = drop_target(&path) {
                self.open_tab(ctx, target);
            }
        }
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            paint_drop_hint(ctx);
        }
    }

    fn follow_appearance(&mut self, ctx: &egui::Context) {
        let preference = self.settings.theme_preference();
        if self.applied_theme != Some(preference) {
            self.applied_theme = Some(preference);
            ctx.set_theme(preference);
            let window = match preference {
                egui::ThemePreference::System => egui::SystemTheme::SystemDefault,
                egui::ThemePreference::Light => egui::SystemTheme::Light,
                egui::ThemePreference::Dark => egui::SystemTheme::Dark,
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(window));
        }
        crate::theme::sync(ctx);
    }

    fn take_handoffs(&mut self, ctx: &egui::Context) {
        let Some(instance) = &self.instance else {
            return;
        };
        let requests = instance.take();
        if requests.is_empty() {
            return;
        }
        for path in requests.into_iter().flatten() {
            self.open_tab(ctx, path);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn open_tab(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.home = false;
        if let Some(i) = self.tabs.iter().position(|t| t.same_repo(&path)) {
            self.active = i;
            return;
        }
        self.tabs.push(Tab::open(ctx, path));
        self.active = self.tabs.len() - 1;
    }

    fn tab_strip(&mut self, ui: &mut egui::Ui) {
        let mut close = None;
        let mut tab_action = None;
        let mut new_tab = false;
        let mut leave_home = false;
        let native = 1.0 / ui.ctx().zoom_factor();
        let tab_h = tab_height(ui.ctx());
        egui::Panel::top("tabs")
            .exact_size(TAB_STRIP_H * native)
            .frame(
                egui::Frame::new()
                    .fill(theme::chrome())
                    .inner_margin(Margin::symmetric(10, 0)),
            )
            .show(ui, |ui| {
                window_drag_area(ui);
                if self.screenshot.is_some() && cfg!(target_os = "macos") {
                    paint_window_buttons(ui);
                }
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let fullscreen = ui.input(|i| i.viewport().fullscreen.unwrap_or(false));
                    if cfg!(target_os = "macos") && !fullscreen {
                        ui.add_space((crate::macos::TRAFFIC_LIGHTS_W * native - 10.0).max(0.0));
                    }
                    kelp_mark(ui);
                    ui.add_space(12.0);
                    let mut centers = Vec::with_capacity(self.tabs.len());
                    let mut dragged = None;
                    let has_others = self.tabs.len() > 1;
                    for (i, tab) in self.tabs.iter().enumerate() {
                        let active = i == self.active && !self.home;
                        let title = tab.title();
                        let galley = ui.painter().layout_no_wrap(
                            title.clone(),
                            FontId::proportional(13.0),
                            theme::text(),
                        );
                        let w = galley.size().x + 50.0;
                        let (rect, _) = ui.allocate_exact_size(vec2(w, tab_h), Sense::hover());
                        let response = ui.interact(
                            rect,
                            egui::Id::new(("tab", &tab.path)),
                            Sense::click_and_drag(),
                        );
                        centers.push(rect.center().x);
                        if response.drag_started() {
                            self.active = i;
                            leave_home = true;
                        }
                        if response.dragged()
                            && let Some(pointer) = response.interact_pointer_pos()
                        {
                            dragged = Some((i, pointer.x));
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                        }
                        let painter = ui.painter_at(rect);
                        if active {
                            painter.rect_filled(rect, 6.0, theme::panel());
                        } else if response.hovered() {
                            painter.rect_filled(rect, 6.0, theme::overlay(0x08));
                        }
                        let color = if active {
                            theme::text_strong()
                        } else {
                            theme::text_muted()
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
                        let x_response = ui.interact(
                            x_rect,
                            egui::Id::new(("close-tab", &tab.path)),
                            Sense::click(),
                        );
                        if x_response.hovered() {
                            painter.rect_filled(x_rect, 4.0, theme::overlay(0x14));
                        }
                        paint_cross(&painter, x_rect.center(), 3.5, theme::text_faint());
                        crate::widgets::focus_ring(ui, &response, 6.0);
                        crate::widgets::describe_selected(
                            &response,
                            format!("tab {title}"),
                            active,
                        );
                        crate::widgets::describe(
                            &x_response,
                            egui::WidgetType::Button,
                            format!("Close {title}"),
                        );
                        if x_response.clicked() || response.middle_clicked() {
                            close = Some(i);
                        } else if response.clicked() {
                            self.active = i;
                            leave_home = true;
                        }
                        let forced_menu = active
                            && self.screenshot.is_some()
                            && std::env::var("KELP_OPEN_MENU").as_deref() == Ok("tab");
                        let mut show_menu = |ui: &mut egui::Ui| {
                            if let Some(action) = menus::tab(ui, &title, has_others) {
                                tab_action = Some((i, action));
                            }
                        };
                        if forced_menu {
                            egui::Popup::from_response(&response)
                                .open(true)
                                .show(&mut show_menu);
                        } else {
                            crate::menus::context_menu(&response, &mut show_menu);
                        }
                        response.on_hover_text(tab.path.display().to_string());
                    }
                    if let Some((from, x)) = dragged {
                        let to = drop_index(&centers, from, x);
                        if to != from {
                            let tab = self.tabs.remove(from);
                            self.tabs.insert(to, tab);
                            self.active = moved_index(self.active, from, to);
                        }
                    }
                    if self.home && !self.tabs.is_empty() && home_tab(ui) {
                        leave_home = true;
                    }
                    ui.add_space(4.0);
                    if new_tab_button(ui).on_hover_text("New tab (⌘T)").clicked() {
                        new_tab = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let settings = egui::Button::new(
                            RichText::new("Settings")
                                .size(12.0)
                                .color(theme::text_muted()),
                        )
                        .frame(false);
                        let response = ui.add(settings);
                        if self.updater.has_news() {
                            let dot =
                                egui::pos2(response.rect.right() - 2.0, response.rect.top() + 6.0);
                            ui.painter().circle_filled(dot, 4.0, theme::accent());
                        }
                        if response.clicked() {
                            self.show_settings = true;
                        }
                        ui.add_space(8.0);
                        self.updater.pill(ui);
                    });
                });
            });
        if let Some(i) = close {
            self.close_tab(i);
        }
        if let Some((i, action)) = tab_action {
            self.run_tab_action(i, action);
        }
        if leave_home {
            self.home = false;
        }
        if new_tab {
            self.show_home();
        }
    }

    fn pick_folder(&mut self) {
        if let Some(folder) = rfd::FileDialog::new()
            .set_title("Open a repository")
            .pick_folder()
        {
            let ctx = self.ctx.clone();
            self.open_tab(&ctx, folder);
        }
    }
}

fn palette_from_env(settings: &Settings) -> Palette {
    let mut palette = Palette::new(settings.recent_actions.clone());
    if let Ok(query) = std::env::var("KELP_OPEN_PALETTE") {
        palette.show(&query);
    }
    palette
}

fn tab_for_number(number: usize, count: usize) -> Option<usize> {
    match number {
        _ if count == 0 => None,
        9 => Some(count - 1),
        n if (1..=count).contains(&n) => Some(n - 1),
        _ => None,
    }
}

fn cycled(active: usize, count: usize, step: isize) -> usize {
    if count == 0 {
        return 0;
    }
    (active as isize + step).rem_euclid(count as isize) as usize
}

fn drop_target(path: &Path) -> Option<PathBuf> {
    let repo_root = |dir: &Path| {
        let repo = gix::discover(dir).ok()?;
        Some(
            repo.workdir()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| repo.path().to_path_buf()),
        )
    };
    if path.is_dir() {
        return Some(repo_root(path).unwrap_or_else(|| path.to_path_buf()));
    }
    repo_root(path.parent()?)
}

fn paint_drop_hint(ctx: &egui::Context) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("drop-hint"),
    ));
    let screen = ctx.content_rect();
    painter.rect_filled(screen, 0.0, theme::with_alpha(theme::bg(), 0xeb));
    painter.rect_stroke(
        screen.shrink(18.0),
        12.0,
        Stroke::new(1.5, theme::with_alpha(theme::accent(), 0x99)),
        egui::StrokeKind::Inside,
    );
    let center = screen.center();
    painter.rect(
        egui::Rect::from_center_size(center + vec2(0.0, 2.0), vec2(380.0, 96.0)),
        12.0,
        theme::popup(),
        Stroke::new(1.0, theme::popup_border()),
        egui::StrokeKind::Inside,
    );
    painter.text(
        center - vec2(0.0, 12.0),
        egui::Align2::CENTER_CENTER,
        "Drop a folder to open it",
        FontId::new(18.0, theme::semibold()),
        theme::text_strong(),
    );
    painter.text(
        center + vec2(0.0, 16.0),
        egui::Align2::CENTER_CENTER,
        "A file opens the repository it belongs to.",
        FontId::proportional(13.0),
        theme::text_muted(),
    );
}

fn drop_index(centers: &[f32], from: usize, pointer_x: f32) -> usize {
    centers
        .iter()
        .enumerate()
        .filter(|&(i, &center)| i != from && center < pointer_x)
        .count()
}

fn moved_index(index: usize, from: usize, to: usize) -> usize {
    if index == from {
        to
    } else if from < index && index <= to {
        index - 1
    } else if to <= index && index < from {
        index + 1
    } else {
        index
    }
}

fn paint_window_buttons(ui: &egui::Ui) {
    let colors = [
        Color32::from_rgb(0xff, 0x5f, 0x57),
        Color32::from_rgb(0xfe, 0xbc, 0x2e),
        Color32::from_rgb(0x28, 0xc8, 0x40),
    ];
    let native = 1.0 / ui.ctx().zoom_factor();
    let top = ui.max_rect().top();
    let left = ui.max_rect().left() - 10.0;
    for (i, color) in colors.into_iter().enumerate() {
        let center = egui::pos2(
            left + (19.0 + i as f32 * 20.0) * native,
            top + TAB_STRIP_H / 2.0 * native,
        );
        ui.painter().circle(
            center,
            6.0 * native,
            color,
            Stroke::new(0.5, Color32::from_black_alpha(60)),
        );
    }
}

fn window_drag_area(ui: &egui::Ui) {
    let bar = ui.interact(
        ui.max_rect(),
        egui::Id::new("window-drag"),
        Sense::click_and_drag(),
    );
    if bar.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if bar.double_clicked() {
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
}

fn kelp_mark(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
    let c = rect.center();
    let edge = |i: usize, side: f32| {
        let t = i as f32 / 24.0;
        let half = 5.0 * (std::f32::consts::PI * t).sin();
        c + vec2(side * half, -8.0 + 13.0 * t)
    };
    let outline: Vec<egui::Pos2> = (0..=24)
        .map(|i| edge(i, 1.0))
        .chain((0..=24).rev().map(|i| edge(i, -1.0)))
        .collect();
    let stroke = Stroke::new(1.4, theme::accent());
    let painter = ui.painter();
    painter.add(egui::Shape::convex_polygon(
        outline,
        theme::with_alpha(theme::accent(), 0x2e),
        stroke,
    ));
    painter.line_segment([c + vec2(0.0, -5.0), c + vec2(0.0, 8.0)], stroke);
}

fn paint_cross(painter: &egui::Painter, center: egui::Pos2, d: f32, color: Color32) {
    let stroke = Stroke::new(1.3, color);
    painter.line_segment([center + vec2(-d, -d), center + vec2(d, d)], stroke);
    painter.line_segment([center + vec2(-d, d), center + vec2(d, -d)], stroke);
}

fn home_tab(ui: &mut egui::Ui) -> bool {
    let galley = ui.painter().layout_no_wrap(
        "New tab".into(),
        FontId::proportional(13.0),
        theme::text_strong(),
    );
    let (rect, _) = ui.allocate_exact_size(
        vec2(galley.size().x + 50.0, tab_height(ui.ctx())),
        Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, theme::panel());
    painter.galley(
        egui::pos2(rect.left() + 14.0, rect.center().y - galley.size().y / 2.0),
        galley,
        theme::text_strong(),
    );
    let x_rect = egui::Rect::from_center_size(
        egui::pos2(rect.right() - 16.0, rect.center().y),
        vec2(18.0, 18.0),
    );
    let close = ui.interact(x_rect, egui::Id::new("close-home-tab"), Sense::click());
    if close.hovered() {
        painter.rect_filled(x_rect, 4.0, theme::overlay(0x14));
    }
    paint_cross(&painter, x_rect.center(), 3.5, theme::text_faint());
    close.clicked()
}

fn tab_height(ctx: &egui::Context) -> f32 {
    TAB_H.min(TAB_STRIP_H / ctx.zoom_factor() - 4.0)
}

fn new_tab_button(ui: &mut egui::Ui) -> egui::Response {
    let side = tab_height(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter().rect_filled(rect, 6.0, theme::overlay(0x08));
    }
    let color = if hovered {
        theme::text_strong()
    } else {
        theme::text_muted()
    };
    let c = rect.center();
    let stroke = Stroke::new(1.4, color);
    ui.painter()
        .line_segment([c + vec2(-5.0, 0.0), c + vec2(5.0, 0.0)], stroke);
    ui.painter()
        .line_segment([c + vec2(0.0, -5.0), c + vec2(0.0, 5.0)], stroke);
    crate::widgets::focus_ring(ui, &response, 6.0);
    crate::widgets::describe(&response, egui::WidgetType::Button, "New tab");
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl eframe::App for KelpApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let geometry = window::current(&self.ctx);
        if !settings::is_dev_run() && geometry.is_some() && geometry != self.settings.window {
            self.settings.window = geometry;
            self.settings.save();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.follow_appearance(&ctx);
        crate::widgets::track_input_mode(&ctx);
        if !self.titlebar_unified {
            crate::macos::unify_titlebar(frame);
            self.titlebar_unified = true;
        }
        self.take_handoffs(&ctx);
        let mut loaded = Vec::new();
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
                if let State::Ready(repo) = &tab.state {
                    loaded.push(repo.dir.clone());
                }
            }
        }
        for dir in loaded {
            self.remember_recent(&dir);
        }
        let active_ready = self.tabs.is_empty()
            || self
                .tabs
                .get(self.active)
                .is_some_and(|t| !matches!(t.state, State::Loading(_)));
        if let Some(shot) = &mut self.screenshot {
            shot.tick(&ctx, active_ready);
        }

        self.updater
            .tick(self.settings.updates_enabled(), self.settings.auto_update);
        self.handle_shortcuts(&ctx);
        self.palette_frame(&ctx);
        self.handle_drops(&ctx);
        self.tab_strip(ui);

        let mut open = Vec::new();
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            if let State::Ready(repo) = &mut tab.state {
                repo.avatars.enabled = self.settings.avatars_enabled();
                repo.editor.clone_from(&self.settings.editor);
                repo.poll(&ctx, self.settings.fetch_interval());
                if i != self.active {
                    open.append(&mut repo.outbox);
                }
            }
        }
        let showing_home = self.home || self.tabs.is_empty();
        let mut welcome_action = None;
        match self
            .tabs
            .get_mut(self.active)
            .filter(|_| !showing_home)
            .map(|t| (&t.path, &mut t.state))
        {
            None => {
                let waving = self.started.elapsed().as_secs_f32() < HELLO_SECONDS;
                welcome_action = self.welcome.ui(ui, &self.settings.recent_repos, waving);
            }
            Some((path, State::Loading(_))) => {
                let screen = MascotScreen {
                    title: "Loading history",
                    subtitle: path.display().to_string(),
                    color: theme::text_muted(),
                    animate: true,
                };
                screen.show(ui);
            }
            Some((path, State::Failed(err))) => {
                let screen = MascotScreen {
                    title: "Could not open this repository",
                    subtitle: format!("{}: {err}", path.display()),
                    color: theme::deleted(),
                    animate: false,
                };
                screen.show(ui);
            }
            Some((_, State::Ready(repo))) => {
                repo.ui(ui, &self.settings);
                open.append(&mut repo.outbox);
                if let Some(columns) = repo.columns_changed.take() {
                    self.settings.graph_columns = columns;
                    self.settings_unsaved = true;
                }
                if let Some(layout) = repo.panels_changed.take() {
                    self.settings.panels = layout;
                    self.settings_unsaved = true;
                }
            }
        }
        if let Some(action) = welcome_action {
            self.run_welcome(&ctx, action);
        }
        if let Some(dialog) = &mut self.clone {
            match dialog.show(&ctx) {
                clone::Outcome::Keep => {}
                clone::Outcome::Close => self.clone = None,
                clone::Outcome::Cloned(path) => {
                    self.clone = None;
                    self.open_tab(&ctx, path);
                }
            }
        }
        for path in open {
            self.open_tab(&ctx, path);
        }
        self.remember_tabs();
        self.remember_window(&ctx);
        self.save_when_settled(&ctx);
        self.paint_notice(&ctx);
        if self.show_settings {
            let repo = self.tabs.get(self.active).map(|t| t.path.clone());
            self.settings.window(
                &ctx,
                &mut self.show_settings,
                &mut self.updater,
                &mut self.signing,
                repo.as_deref(),
            );
        }
    }
}

const HELLO_SECONDS: f32 = 4.0;
const NOTICE_FOR: Duration = Duration::from_millis(1600);
const TAB_STRIP_H: f32 = 40.0;
const TAB_H: f32 = 28.0;
const MASCOT_SIZE: f32 = 220.0;

struct MascotScreen<'a> {
    title: &'a str,
    subtitle: String,
    color: Color32,
    animate: bool,
}

impl MascotScreen<'_> {
    fn show(self, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::bg()))
            .show(ui, |ui| {
                let full = ui.max_rect();
                let top = full.center().y - MASCOT_SIZE * 0.75;
                let art = egui::Rect::from_center_size(
                    egui::pos2(full.center().x, top + MASCOT_SIZE / 2.0),
                    vec2(MASCOT_SIZE, MASCOT_SIZE),
                );
                let hovered = ui.rect_contains_pointer(art);
                let animate = self.animate || hovered;
                let time = ui.input(|i| i.time) as f32;
                crate::mascot::paint(ui.painter(), art, time, animate);
                if animate {
                    ui.ctx().request_repaint();
                }
                let mut y = art.bottom() + 18.0;
                let painter = ui.painter().clone();
                let title = painter.layout_no_wrap(
                    self.title.to_string(),
                    FontId::new(22.0, theme::semibold()),
                    theme::text_strong(),
                );
                painter.galley(
                    egui::pos2(full.center().x - title.size().x / 2.0, y),
                    title.clone(),
                    theme::text_strong(),
                );
                y += title.size().y + 8.0;
                let sub = painter.layout(
                    self.subtitle,
                    FontId::proportional(13.5),
                    self.color,
                    full.width().min(560.0),
                );
                painter.galley(
                    egui::pos2(full.center().x - sub.size().x / 2.0, y),
                    sub.clone(),
                    self.color,
                );
            });
        if self.animate {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{cycled, drop_index, drop_target, moved_index, tab_for_number};

    #[test]
    fn dragging_past_a_neighbor_center_swaps() {
        let centers = [50.0, 150.0, 250.0];
        assert_eq!(drop_index(&centers, 0, 90.0), 0);
        assert_eq!(drop_index(&centers, 0, 160.0), 1);
        assert_eq!(drop_index(&centers, 0, 400.0), 2);
        assert_eq!(drop_index(&centers, 2, 10.0), 0);
        assert_eq!(drop_index(&centers, 2, 200.0), 2);
        assert_eq!(drop_index(&centers, 2, 120.0), 1);
    }

    #[test]
    fn other_tabs_shift_around_the_moved_one() {
        let order = |from, to| {
            let mut tabs = vec!['a', 'b', 'c', 'd'];
            let t = tabs.remove(from);
            tabs.insert(to, t);
            tabs
        };
        for (from, to) in [(0, 2), (3, 1), (1, 1), (2, 0)] {
            let after = order(from, to);
            for (index, name) in ['a', 'b', 'c', 'd'].into_iter().enumerate() {
                assert_eq!(after[moved_index(index, from, to)], name, "{from}->{to}");
            }
        }
    }

    #[test]
    fn number_shortcuts_pick_a_tab_and_nine_is_last() {
        assert_eq!(tab_for_number(1, 3), Some(0));
        assert_eq!(tab_for_number(3, 3), Some(2));
        assert_eq!(tab_for_number(4, 3), None);
        assert_eq!(tab_for_number(9, 3), Some(2));
        assert_eq!(tab_for_number(9, 12), Some(11));
        assert_eq!(tab_for_number(1, 0), None);
    }

    #[test]
    fn ctrl_tab_wraps_around() {
        assert_eq!(cycled(0, 3, 1), 1);
        assert_eq!(cycled(2, 3, 1), 0);
        assert_eq!(cycled(0, 3, -1), 2);
        assert_eq!(cycled(0, 0, 1), 0);
    }

    #[test]
    fn dropped_paths_open_their_repository() {
        let root = std::env::temp_dir().join(format!("kelp-drop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let plain = root.join("plain");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::write(repo.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(plain.join("notes.txt"), "hi").unwrap();
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&repo)
            .status()
            .unwrap();
        assert!(status.success());
        let real = |p: std::path::PathBuf| std::fs::canonicalize(p).unwrap();
        let repo = real(repo);
        assert_eq!(
            drop_target(&repo.join("src/main.rs")).map(real),
            Some(repo.clone())
        );
        assert_eq!(drop_target(&repo.join("src")).map(real), Some(repo.clone()));
        assert_eq!(drop_target(&plain).map(real), Some(real(plain.clone())));
        assert_eq!(drop_target(&plain.join("notes.txt")), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
