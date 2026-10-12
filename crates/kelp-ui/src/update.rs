use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use egui::{self, RichText, ViewportCommand};
use kelp_core::update::{self, Release, Source};

use crate::theme;

const FIRST_CHECK_AFTER: Duration = Duration::from_secs(5);
const CHECK_EVERY: Duration = Duration::from_secs(3600);
const RETRY_AFTER: Duration = Duration::from_secs(15 * 60);
const FOCUS_CHECK_GAP: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Installing(Release),
    Ready(Release),
    Failed(String),
}

enum Message {
    Checked {
        result: anyhow::Result<Release>,
        asked: bool,
    },
    AlreadyInstalled(Release),
    Installed(Release, anyhow::Result<()>),
}

/// What the updater needs to know about the app it updates.
#[derive(Debug, Clone)]
pub struct UpdateConfig {
    /// The GitHub repository and the Homebrew cask.
    pub source: Source,
    /// Shown in the updater's texts.
    pub app_name: String,
    /// The running version, compared with the latest release.
    pub version: String,
}

/// Checks for new releases in the background and installs them with `brew upgrade`.
pub struct Updater {
    pub status: Status,
    config: UpdateConfig,
    next_check: Instant,
    last_check: Option<Instant>,
    focused: bool,
    tx: Sender<Message>,
    rx: Receiver<Message>,
    ctx: egui::Context,
}

impl Updater {
    pub fn new(ctx: egui::Context, config: UpdateConfig) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            status: Status::Idle,
            config,
            next_check: Instant::now() + FIRST_CHECK_AFTER,
            last_check: None,
            focused: true,
            tx,
            rx,
            ctx,
        }
    }

    pub fn current(&self) -> &str {
        &self.config.version
    }

    pub fn brew_install(&self) -> bool {
        update::installed_with_brew(&self.config.source)
    }

    pub fn check_now(&mut self) {
        self.check(true);
    }

    fn check(&mut self, asked: bool) {
        if matches!(
            self.status,
            Status::Checking | Status::Installing(_) | Status::Ready(_)
        ) {
            return;
        }
        if asked {
            self.status = Status::Checking;
        }
        self.next_check = Instant::now() + CHECK_EVERY;
        self.last_check = Some(Instant::now());
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let source = self.config.source.clone();
        let current = self.config.version.clone();
        std::thread::spawn(move || {
            let installed =
                update::installed_version().filter(|version| update::is_newer(version, &current));
            let message = match installed {
                Some(version) => Message::AlreadyInstalled(Release {
                    version,
                    url: String::new(),
                }),
                None => Message::Checked {
                    result: update::latest_release(&source),
                    asked,
                },
            };
            let _ = tx.send(message);
            ctx.request_repaint();
        });
    }

    pub fn install(&mut self, release: Release) {
        self.status = Status::Installing(release.clone());
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let source = self.config.source.clone();
        std::thread::spawn(move || {
            let result = update::brew_upgrade(&source);
            let _ = tx.send(Message::Installed(release, result));
            ctx.request_repaint();
        });
    }

    /// Call once per frame; collects results and starts the periodic check when `enabled`.
    pub fn tick(&mut self, enabled: bool, auto_install: bool) {
        while let Ok(message) = self.rx.try_recv() {
            self.status = match message {
                Message::Checked {
                    result: Ok(release),
                    ..
                } if update::is_newer(&release.version, &self.config.version) => {
                    Status::Available(release)
                }
                Message::Checked { result: Ok(_), .. } => Status::UpToDate,
                Message::Checked {
                    result: Err(e),
                    asked: true,
                } => Status::Failed(format!("{e:#}")),
                Message::Checked {
                    result: Err(_),
                    asked: false,
                } => {
                    self.next_check = Instant::now() + RETRY_AFTER;
                    match &self.status {
                        Status::Checking => Status::Idle,
                        other => other.clone(),
                    }
                }
                Message::AlreadyInstalled(release) => Status::Ready(release),
                Message::Installed(release, Ok(())) => Status::Ready(release),
                Message::Installed(_, Err(e)) => Status::Failed(format!("{e:#}")),
            };
            if let Status::Available(release) = &self.status
                && auto_install
                && self.brew_install()
            {
                let release = release.clone();
                self.install(release);
            }
        }
        let focused = self.ctx.input(|i| i.focused);
        let regained_focus = focused && !self.focused;
        self.focused = focused;
        let checked_lately = self
            .last_check
            .is_some_and(|at| at.elapsed() < FOCUS_CHECK_GAP);
        if enabled && (Instant::now() >= self.next_check || (regained_focus && !checked_lately)) {
            self.check(false);
        }
        if enabled {
            self.ctx
                .request_repaint_after(self.next_check.saturating_duration_since(Instant::now()));
        }
    }

    pub fn has_news(&self) -> bool {
        matches!(self.status, Status::Available(_) | Status::Ready(_))
    }

    pub fn pill(&mut self, ui: &mut egui::Ui) {
        let (label, action) = match &self.status {
            Status::Available(r) if self.brew_install() => {
                (format!("Update to {}", r.version), Some(r.clone()))
            }
            Status::Available(r) => (
                format!("{} {} is out", self.config.app_name, r.version),
                None,
            ),
            Status::Installing(r) => (format!("Installing {}…", r.version), None),
            Status::Ready(r) => (format!("Restart to update to {}", r.version), None),
            _ => return,
        };
        let ready = matches!(self.status, Status::Ready(_));
        let button = egui::Button::new(
            RichText::new(label)
                .size(12.0)
                .family(theme::semibold())
                .color(theme::on_accent()),
        )
        .fill(theme::accent())
        .corner_radius(12);
        let response = ui.add(button);
        if response.clicked() {
            if ready {
                restart(ui.ctx());
            } else if let Some(release) = action {
                self.install(release);
            } else if let Status::Available(release) = &self.status {
                ui.ctx().open_url(egui::OpenUrl::new_tab(&release.url));
            }
        }
    }

    pub fn settings_section(
        &mut self,
        ui: &mut egui::Ui,
        auto_install: &mut bool,
        enabled: &mut bool,
    ) {
        ui.label(
            RichText::new("Updates")
                .family(theme::semibold())
                .color(theme::text_strong()),
        );
        let app = self.config.app_name.clone();
        let status = match &self.status {
            Status::Idle => format!("{app} {}", self.current()),
            Status::Checking => "Checking for updates…".into(),
            Status::UpToDate => format!("{app} {} is the latest version", self.current()),
            Status::Available(r) => format!(
                "{app} {} is available (you have {})",
                r.version,
                self.current()
            ),
            Status::Installing(r) => format!("Installing {app} {}…", r.version),
            Status::Ready(r) => format!("{app} {} is installed. Restart to use it.", r.version),
            Status::Failed(e) => format!("Update check failed: {e}"),
        };
        ui.label(RichText::new(status).size(12.0).color(theme::text_muted()));
        ui.horizontal(|ui| {
            if ui.button("Check now").clicked() {
                self.check_now();
            }
            self.pill(ui);
        });
        ui.checkbox(
            enabled,
            RichText::new("Check for updates automatically").color(theme::text_strong()),
        );
        let brew = self.brew_install();
        ui.add_enabled_ui(brew, |ui| {
            ui.checkbox(
                auto_install,
                RichText::new("Install updates in the background").color(theme::text_strong()),
            );
        });
        let hint = if brew {
            "Uses brew upgrade. Restart when it's done.".to_string()
        } else {
            format!("Needs {app} installed with Homebrew.")
        };
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.label(RichText::new(hint).size(12.0).color(theme::text_faint()));
        });
    }
}

fn restart(ctx: &egui::Context) {
    if let Some(bundle) = update::app_bundle() {
        let _ = std::process::Command::new("open")
            .arg("-n")
            .arg(bundle)
            .spawn();
    }
    ctx.send_viewport_cmd(ViewportCommand::Close);
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use kelp_core::update::Source;

    use super::{Message, RETRY_AFTER, Release, Status, UpdateConfig, Updater};

    fn updater() -> Updater {
        let config = UpdateConfig {
            source: Source::new("Hobo-Ware/kelp", "hobo-ware/tap/kelp"),
            app_name: "Kelp".into(),
            version: "0.8.19".into(),
        };
        let mut updater = Updater::new(egui::Context::default(), config);
        updater.status = Status::UpToDate;
        updater
    }

    #[test]
    fn a_failed_background_check_stays_quiet_and_retries_soon() {
        let mut updater = updater();
        let result = Err(anyhow::anyhow!("403"));
        updater
            .tx
            .send(Message::Checked {
                result,
                asked: false,
            })
            .unwrap();
        updater.tick(false, false);
        assert_eq!(updater.status, Status::UpToDate);
        let wait = updater.next_check.saturating_duration_since(Instant::now());
        assert!(wait <= RETRY_AFTER && wait > RETRY_AFTER - Duration::from_secs(5));
    }

    #[test]
    fn a_failed_check_you_asked_for_says_why() {
        let mut updater = updater();
        let result = Err(anyhow::anyhow!("offline"));
        updater
            .tx
            .send(Message::Checked {
                result,
                asked: true,
            })
            .unwrap();
        updater.tick(false, false);
        assert_eq!(updater.status, Status::Failed("offline".into()));
    }

    #[test]
    fn a_newer_copy_on_disk_asks_for_a_restart() {
        let mut updater = updater();
        let release = Release {
            version: "99.0.0".into(),
            url: String::new(),
        };
        updater
            .tx
            .send(Message::AlreadyInstalled(release.clone()))
            .unwrap();
        updater.tick(false, false);
        assert_eq!(updater.status, Status::Ready(release));
        assert!(updater.has_news());
    }
}
