use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use eframe::egui::{self, RichText, Ui};
use kelp_core::signing::{self, Config, Format, Key, Scope};

use crate::{theme, widgets};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Off,
    Gpg,
    Ssh,
}

struct Loaded {
    config: Config,
    keys: Vec<Key>,
}

pub struct SigningPanel {
    loaded_for: Option<PathBuf>,
    loading: Option<Receiver<Loaded>>,
    saving: Option<Receiver<Result<String, String>>>,
    mode: Mode,
    key: String,
    keys: Vec<Key>,
    scope: Scope,
    status: Option<(String, bool)>,
}

impl Default for SigningPanel {
    fn default() -> Self {
        Self {
            loaded_for: None,
            loading: None,
            saving: None,
            mode: Mode::Off,
            key: String::new(),
            keys: Vec::new(),
            scope: Scope::Global,
            status: None,
        }
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

impl SigningPanel {
    fn load(&mut self, ctx: &egui::Context, dir: PathBuf) {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let reading = dir.clone();
        std::thread::spawn(move || {
            let loaded = Loaded {
                config: signing::current(&reading),
                keys: signing::detect_keys(&home()),
            };
            if tx.send(loaded).is_ok() {
                ctx.request_repaint();
            }
        });
        self.loading = Some(rx);
        self.loaded_for = Some(dir);
    }

    fn poll(&mut self) {
        if let Some(loaded) = self.loading.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.loading = None;
            self.mode = match (loaded.config.sign, loaded.config.format) {
                (false, _) => Mode::Off,
                (true, Format::Gpg) => Mode::Gpg,
                (true, Format::Ssh) => Mode::Ssh,
            };
            self.key = loaded.config.key;
            self.keys = loaded.keys;
        }
        if let Some(result) = self.saving.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.saving = None;
            self.status = Some(match result {
                Ok(text) => (text, false),
                Err(text) => (text, true),
            });
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, repo: Option<&Path>) {
        let dir = repo.map(Path::to_path_buf).unwrap_or_else(home);
        if self.loaded_for.as_ref() != Some(&dir) {
            if repo.is_some() {
                self.scope = Scope::Repo;
            }
            self.load(ui.ctx(), dir.clone());
        }
        self.poll();
        ui.label(
            RichText::new("Commit signing")
                .family(theme::semibold())
                .color(theme::TEXT_STRONG),
        );
        if self.loading.is_some() {
            ui.label(RichText::new("Reading your git config…").color(theme::TEXT_FAINT));
            return;
        }
        widgets::segmented(
            ui,
            &mut self.mode,
            &[(Mode::Off, "Off"), (Mode::Gpg, "GPG"), (Mode::Ssh, "SSH")],
        );
        if self.mode != Mode::Off {
            let format = if self.mode == Mode::Ssh {
                Format::Ssh
            } else {
                Format::Gpg
            };
            let choices: Vec<&Key> = self.keys.iter().filter(|k| k.format == format).collect();
            let selected = choices.iter().find(|k| k.id == self.key).map_or_else(
                || {
                    if self.key.is_empty() {
                        "Choose a key".to_string()
                    } else {
                        self.key.clone()
                    }
                },
                |k| k.label.clone(),
            );
            egui::ComboBox::from_id_salt("signing-key")
                .width(300.0)
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for key in &choices {
                        ui.selectable_value(&mut self.key, key.id.clone(), &key.label);
                    }
                    if choices.is_empty() {
                        ui.label(
                            RichText::new(if format == Format::Ssh {
                                "No keys in ~/.ssh"
                            } else {
                                "No GPG secret keys found"
                            })
                            .color(theme::TEXT_FAINT),
                        );
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.key)
                    .hint_text("or paste a key id or public key path")
                    .desired_width(300.0),
            );
        }
        ui.horizontal(|ui| {
            ui.add_enabled_ui(repo.is_some(), |ui| {
                ui.radio_value(&mut self.scope, Scope::Repo, "This repository");
            });
            ui.radio_value(&mut self.scope, Scope::Global, "All repositories");
        });
        ui.horizontal(|ui| {
            let busy = self.saving.is_some();
            if ui
                .add_enabled(!busy, egui::Button::new("Apply").corner_radius(6))
                .clicked()
            {
                self.apply(ui.ctx(), &dir);
            }
            if let Some((text, error)) = &self.status {
                let color = if *error {
                    theme::DELETED
                } else {
                    theme::TEXT_FAINT
                };
                ui.label(RichText::new(text).size(12.0).color(color));
            }
        });
        ui.label(
            RichText::new(
                "Kelp signs through git, so commits from the terminal sign the same way.",
            )
            .size(12.0)
            .color(theme::TEXT_FAINT),
        );
    }

    fn apply(&mut self, ctx: &egui::Context, dir: &Path) {
        let config = Config {
            sign: self.mode != Mode::Off,
            format: if self.mode == Mode::Ssh {
                Format::Ssh
            } else {
                Format::Gpg
            },
            key: self.key.clone(),
        };
        let scope = self.scope;
        let dir = dir.to_path_buf();
        let ctx = ctx.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = signing::apply(&dir, scope, &config)
                .map(|()| match scope {
                    Scope::Repo => "Saved to this repository's git config".to_string(),
                    Scope::Global => "Saved to your global git config".to_string(),
                })
                .map_err(|e| format!("{e:#}"));
            if tx.send(result).is_ok() {
                ctx.request_repaint();
            }
        });
        self.saving = Some(rx);
        self.status = None;
    }
}
