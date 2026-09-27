use std::path::PathBuf;

use eframe::egui::{self, Color32, Margin, RichText, Stroke};
use serde::{Deserialize, Serialize};

use crate::theme;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub dim_outside_history: bool,
    pub load_avatars: bool,
    pub show_descriptions: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dim_outside_history: false,
            load_avatars: true,
            show_descriptions: true,
        }
    }
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
        let mut settings: Settings = std::fs::read(Self::file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if std::env::var_os("KELP_OFFLINE").is_some() {
            settings.load_avatars = false;
        }
        settings
    }

    pub fn save(&self) {
        let file = Self::file();
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(file, json);
        }
    }

    pub fn window(&mut self, ctx: &egui::Context, open: &mut bool) {
        let before = self.clone();
        let modal = egui::Modal::new(egui::Id::new("kelp-settings"))
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x1f, 0x24, 0x2d))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(0x3a, 0x42, 0x50)))
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
                        .color(theme::TEXT_STRONG),
                );
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
    }
}

fn option(ui: &mut egui::Ui, value: &mut bool, label: &str, hint: &str) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.checkbox(value, RichText::new(label).color(theme::TEXT_STRONG));
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.label(RichText::new(hint).size(12.0).color(theme::TEXT_FAINT));
        });
    });
}
