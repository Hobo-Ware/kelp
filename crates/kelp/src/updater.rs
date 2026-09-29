use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui::{self, RichText, ViewportCommand};
use kelp_core::update::{self, Release};

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

pub struct Updater {
    pub status: Status,
    next_check: Instant,
    last_check: Option<Instant>,
    focused: bool,
    tx: Sender<Message>,
    rx: Receiver<Message>,
    ctx: egui::Context,
}

impl Updater {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let status = match std::env::var("KELP_FAKE_UPDATE") {
            Ok(version) => Status::Available(Release {
                version,
                url: String::new(),
            }),
            Err(_) => Status::Idle,
        };
        Self {
            status,
            next_check: Instant::now() + FIRST_CHECK_AFTER,
            last_check: None,
            focused: true,
            tx,
            rx,
            ctx,
        }
    }

    pub fn current() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    pub fn brew_install() -> bool {
        update::installed_with_brew()
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
        std::thread::spawn(move || {
            let installed = update::installed_version()
                .filter(|version| update::is_newer(version, Self::current()));
            let message = match installed {
                Some(version) => Message::AlreadyInstalled(Release {
                    version,
                    url: String::new(),
                }),
                None => Message::Checked {
                    result: update::latest_release(),
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
        std::thread::spawn(move || {
            let result = update::brew_upgrade();
            let _ = tx.send(Message::Installed(release, result));
            ctx.request_repaint();
        });
    }

    pub fn tick(&mut self, enabled: bool, auto_install: bool) {
        while let Ok(message) = self.rx.try_recv() {
            self.status = match message {
                Message::Checked {
                    result: Ok(release),
                    ..
                } if update::is_newer(&release.version, Self::current()) => {
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
                && Self::brew_install()
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
            Status::Available(r) if Self::brew_install() => {
                (format!("Update to {}", r.version), Some(r.clone()))
            }
            Status::Available(r) => (format!("Kelp {} is out", r.version), None),
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
        let status = match &self.status {
            Status::Idle => format!("Kelp {}", Self::current()),
            Status::Checking => "Checking for updates…".into(),
            Status::UpToDate => format!("Kelp {} is the latest version", Self::current()),
            Status::Available(r) => format!(
                "Kelp {} is available (you have {})",
                r.version,
                Self::current()
            ),
            Status::Installing(r) => format!("Installing Kelp {}…", r.version),
            Status::Ready(r) => format!("Kelp {} is installed. Restart to use it.", r.version),
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
        let brew = Self::brew_install();
        ui.add_enabled_ui(brew, |ui| {
            ui.checkbox(
                auto_install,
                RichText::new("Install updates in the background").color(theme::text_strong()),
            );
        });
        let hint = if brew {
            "Uses brew upgrade. Restart when it's done."
        } else {
            "Needs Kelp installed with Homebrew."
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

    use super::{Message, RETRY_AFTER, Release, Status, Updater};

    fn updater() -> Updater {
        let mut updater = Updater::new(eframe::egui::Context::default());
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
