use std::path::PathBuf;
use std::time::Duration;

use eframe::egui::{self, Margin, RichText, Stroke};
use serde::{Deserialize, Serialize};

use crate::columns::GraphColumns;
use crate::panels::{self, Panels};
use crate::recents::Recent;
use crate::theme;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub fn preference(self) -> egui::ThemePreference {
        match self {
            Appearance::System => egui::ThemePreference::System,
            Appearance::Light => egui::ThemePreference::Light,
            Appearance::Dark => egui::ThemePreference::Dark,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub appearance: Appearance,
    pub dim_outside_history: bool,
    pub load_avatars: bool,
    pub show_descriptions: bool,
    pub open_tabs: Vec<PathBuf>,
    pub check_updates: bool,
    pub auto_update: bool,
    pub auto_fetch: bool,
    pub fetch_minutes: u32,
    pub editor: String,
    pub window: Option<[f32; 4]>,
    pub graph_columns: GraphColumns,
    pub recent_actions: Vec<String>,
    pub recent_repos: Vec<Recent>,
    pub panels: Panels,
    pub zoom: f32,
    pub last_seen_version: Option<String>,
    #[serde(skip)]
    offline: bool,
    #[serde(skip)]
    fresh_install: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::System,
            dim_outside_history: false,
            load_avatars: true,
            show_descriptions: true,
            open_tabs: Vec::new(),
            check_updates: true,
            auto_update: true,
            auto_fetch: true,
            fetch_minutes: 5,
            editor: String::new(),
            window: None,
            graph_columns: GraphColumns::default(),
            recent_actions: Vec::new(),
            recent_repos: Vec::new(),
            panels: Panels::default(),
            zoom: 1.0,
            last_seen_version: None,
            offline: false,
            fresh_install: false,
        }
    }
}

pub fn is_dev_run() -> bool {
    std::env::var_os("KELP_SCREENSHOT").is_some() || std::env::var_os("KELP_BENCH_SCROLL").is_some()
}

impl Settings {
    fn file() -> PathBuf {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let base = if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"))
        };
        base.join("kelp").join("settings.json")
    }

    pub fn load() -> Self {
        let file = Self::file();
        let mut settings: Settings = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        settings.fresh_install = !file.exists();
        settings.offline = std::env::var_os("KELP_OFFLINE").is_some();
        if let Ok(names) = std::env::var("KELP_COLUMNS") {
            settings.graph_columns = GraphColumns::from_names(&names);
        }
        if is_dev_run() {
            settings.recent_repos = crate::recents::from_env().unwrap_or_default();
            settings.zoom = std::env::var("KELP_ZOOM")
                .ok()
                .and_then(|z| z.parse().ok())
                .unwrap_or(1.0);
            settings.panels = Panels::default().collapsed_by_env();
        }
        settings.zoom = panels::clamp_zoom(settings.zoom);
        settings
    }

    pub fn theme_preference(&self) -> egui::ThemePreference {
        static FORCED: std::sync::OnceLock<Option<egui::ThemePreference>> =
            std::sync::OnceLock::new();
        let forced = FORCED.get_or_init(|| match std::env::var("KELP_THEME").as_deref() {
            Ok("light") => Some(egui::ThemePreference::Light),
            Ok("dark") => Some(egui::ThemePreference::Dark),
            _ => None,
        });
        forced.unwrap_or_else(|| self.appearance.preference())
    }

    pub fn is_fresh_install(&self) -> bool {
        self.fresh_install
    }

    pub fn avatars_enabled(&self) -> bool {
        self.load_avatars && !self.offline
    }

    pub fn save(&self) {
        if is_dev_run() {
            return;
        }
        let file = Self::file();
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(file, json);
        }
    }

    pub fn fetch_interval(&self) -> Option<Duration> {
        (self.auto_fetch && !self.offline)
            .then(|| Duration::from_secs(u64::from(self.fetch_minutes.max(1)) * 60))
    }

    pub fn updates_enabled(&self) -> bool {
        self.check_updates && !self.offline
    }

    pub fn window(
        &mut self,
        ctx: &egui::Context,
        open: &mut bool,
        updater: &mut crate::updater::Updater,
        signing: &mut crate::signing_panel::SigningPanel,
        repo: Option<&std::path::Path>,
    ) -> bool {
        let before = self.clone();
        let mut whats_new = false;
        let modal = egui::Modal::new(egui::Id::new("kelp-settings")).backdrop_color(theme::backdrop())
            .frame(
                egui::Frame::new()
                    .fill(theme::modal())
                    .stroke(Stroke::new(1.0, theme::modal_border()))
                    .corner_radius(10)
                    .inner_margin(Margin::same(22)),
            )
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.spacing_mut().item_spacing.y = 12.0;
                ui.label(
                    RichText::new("Settings")
                        .size(17.0)
                        .family(theme::semibold())
                        .color(theme::text_strong()),
                );
                let max_height = ui.ctx().content_rect().height() * 0.72;
                egui::ScrollArea::vertical()
                    .max_height(max_height)
                    .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 12.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Appearance").color(theme::text_strong()));
                    ui.add_space(8.0);
                    crate::widgets::segmented(
                        ui,
                        &mut self.appearance,
                        &[
                            (Appearance::System, "System"),
                            (Appearance::Light, "Light"),
                            (Appearance::Dark, "Dark"),
                        ],
                    );
                });
                option(
                    ui,
                    &mut self.show_descriptions,
                    "Show commit descriptions in the graph",
                    "The grey text after each commit title.",
                );
                option(
                    ui,
                    &mut self.dim_outside_history,
                    "Fade commits outside the selected history",
                    "Makes it easy to see which commits are part of the selected commit.",
                );
                option(
                    ui,
                    &mut self.load_avatars,
                    "Load avatars from GitHub and Gravatar",
                    "Off means generated initials only, and no network requests.",
                );
                option(
                    ui,
                    &mut self.auto_fetch,
                    "Fetch from remotes in the background",
                    "Keeps remote branches current. Local changes show up on their own.",
                );
                ui.add_enabled_ui(self.auto_fetch, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(24.0);
                        ui.label(RichText::new("Every").color(theme::text()));
                        egui::ComboBox::from_id_salt("fetch-minutes")
                            .selected_text(format!("{} min", self.fetch_minutes))
                            .show_ui(ui, |ui| {
                                for minutes in [1, 5, 15, 30] {
                                    ui.selectable_value(
                                        &mut self.fetch_minutes,
                                        minutes,
                                        format!("{minutes} min"),
                                    );
                                }
                            });
                    });
                });
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.label(RichText::new("Open files with").color(theme::text_strong()));
                    let detected = crate::open_with::editor_app("");
                    let hint = detected
                        .as_deref()
                        .map_or("System default app".to_string(), |app| app.to_string());
                    ui.add(
                        egui::TextEdit::singleline(&mut self.editor)
                            .hint_text(hint)
                            .desired_width(260.0),
                    );
                    ui.label(
                        RichText::new("An app name, like Visual Studio Code. Empty uses the first installed of Cursor, VS Code, Zed and Sublime Text.")
                            .size(12.0)
                            .color(theme::text_faint()),
                    );
                });
                ui.separator();
                signing.ui(ui, repo);
                ui.separator();
                updater.settings_section(ui, &mut self.auto_update, &mut self.check_updates);
                ui.separator();
                whats_new = help_section(ui);
                if std::env::var("KELP_OPEN_SETTINGS").as_deref() == Ok("help") {
                    ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                }
                    });
                ui.add_space(4.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new("Done")
                                .corner_radius(6)
                                .min_size(egui::vec2(80.0, 32.0)),
                        )
                        .clicked()
                    {
                        *open = false;
                    }
                });
            });
        if modal.should_close() {
            *open = false;
        }
        if *self != before {
            self.save();
        }
        whats_new
    }
}

fn help_section(ui: &mut egui::Ui) -> bool {
    let mut whats_new = false;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.label(RichText::new("Help").color(theme::text_strong()));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            for page in crate::help::Page::IN_SETTINGS {
                if ui.link(page.title()).clicked() {
                    let _ = page.open();
                }
            }
            whats_new = ui.link("What's new").clicked();
        });
    });
    whats_new
}

fn option(ui: &mut egui::Ui, value: &mut bool, label: &str, hint: &str) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.checkbox(value, RichText::new(label).color(theme::text_strong()));
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.label(RichText::new(hint).size(12.0).color(theme::text_faint()));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recents_panels_and_zoom_survive_a_round_trip() {
        let mut settings = Settings::default();
        settings.recent_repos.push(Recent {
            path: PathBuf::from("/nope/kelp"),
            opened: 42,
        });
        settings.panels.sidebar_open = false;
        settings.panels.details_width = 410.0;
        settings.zoom = 1.25;
        let json = serde_json::to_string(&settings).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.recent_repos, settings.recent_repos);
        assert_eq!(back.panels, settings.panels);
        assert_eq!(back.zoom, 1.25);
    }

    #[test]
    fn appearance_survives_a_round_trip() {
        for appearance in [Appearance::System, Appearance::Light, Appearance::Dark] {
            let settings = Settings {
                appearance,
                ..Settings::default()
            };
            let json = serde_json::to_string(&settings).unwrap();
            let back: Settings = serde_json::from_str(&json).unwrap();
            assert_eq!(back.appearance, appearance);
        }
        assert_eq!(Appearance::Light.preference(), egui::ThemePreference::Light);
    }

    #[test]
    fn settings_from_older_versions_get_defaults() {
        let back: Settings = serde_json::from_str(r#"{"load_avatars": false}"#).unwrap();
        assert_eq!(back.appearance, Appearance::System);
        assert!(!back.load_avatars);
        assert!(back.recent_repos.is_empty());
        assert_eq!(back.panels, Panels::default());
        assert_eq!(back.zoom, 1.0);
    }
}
