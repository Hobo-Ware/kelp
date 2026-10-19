use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use egui::{self, RichText, ViewportCommand};
use kelp_core::update::{self, Release, Source, UpgradeError};

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
    Stuck(PathBuf),
    Failed(String),
}

enum Message {
    Checked {
        result: anyhow::Result<Release>,
        asked: bool,
    },
    AlreadyInstalled(Release),
    Stuck(PathBuf),
    Installed(Release, Result<(), UpgradeError>),
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
    /// Where the repair helper writes its log.
    pub state_dir: PathBuf,
}

/// Checks for new releases in the background and installs them with `brew upgrade`.
pub struct Updater {
    pub status: Status,
    active: bool,
    config: UpdateConfig,
    next_check: Instant,
    last_check: Option<Instant>,
    focused: bool,
    quit_requested: bool,
    tx: Sender<Message>,
    rx: Receiver<Message>,
    ctx: egui::Context,
}

impl Updater {
    pub fn new(ctx: egui::Context, config: UpdateConfig) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            status: Status::Idle,
            active: true,
            config,
            next_check: Instant::now() + FIRST_CHECK_AFTER,
            last_check: None,
            focused: true,
            quit_requested: false,
            tx,
            rx,
            ctx,
        }
    }

    /// Turns the updater off for good: no checks, no pill, no settings section. For platforms
    /// the app has no installer for.
    pub fn disable(&mut self) {
        self.active = false;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn current(&self) -> &str {
        &self.config.version
    }

    pub fn brew_install(&self) -> bool {
        update::installed_with_brew(&self.config.source)
    }

    pub fn check_now(&mut self) {
        if self.active {
            self.check(true);
        }
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
            let message = match (update::stuck_upgrade(&source), installed) {
                (Some(path), _) => Message::Stuck(path),
                (None, Some(version)) => Message::AlreadyInstalled(Release {
                    version,
                    url: String::new(),
                }),
                (None, None) => Message::Checked {
                    result: update::latest_release(&source),
                    asked,
                },
            };
            let _ = tx.send(message);
            ctx.request_repaint();
        });
    }

    pub fn install(&mut self, release: Release) {
        if let Some(path) = update::stuck_upgrade(&self.config.source) {
            self.status = Status::Stuck(path);
            return;
        }
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
                Message::Stuck(path) | Message::Installed(_, Err(UpgradeError::Stuck(path))) => {
                    Status::Stuck(path)
                }
                Message::Installed(_, Err(e)) => Status::Failed(e.to_string()),
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
        let enabled = enabled && self.active;
        if enabled && (Instant::now() >= self.next_check || (regained_focus && !checked_lately)) {
            self.check(false);
        }
        if enabled {
            self.ctx
                .request_repaint_after(self.next_check.saturating_duration_since(Instant::now()));
        }
    }

    /// The commands the repair helper runs, or `None` outside an app bundle.
    pub fn repair_script(&self) -> Option<String> {
        let bundle = update::app_bundle()?;
        Some(update::repair_command(
            &self.config.source,
            &bundle,
            std::process::id(),
        ))
    }

    /// Starts the repair helper detached. On success the host should quit once `take_quit_request` says so.
    pub fn fix(&mut self) {
        if !self.active || matches!(self.status, Status::Installing(_)) {
            return;
        }
        let Some(bundle) = update::app_bundle() else {
            self.status = Status::Failed("Not running from an app bundle".into());
            return;
        };
        let log = self.config.state_dir.join("update-repair.log");
        match update::start_repair(&self.config.source, &bundle, std::process::id(), &log) {
            Ok(()) => self.quit_requested = true,
            Err(e) => self.status = Status::Failed(format!("Could not start the repair: {e}")),
        }
    }

    /// True once after `fix` started the helper: the host must now quit the app.
    pub fn take_quit_request(&mut self) -> bool {
        std::mem::take(&mut self.quit_requested)
    }

    pub fn can_restart(&self) -> bool {
        !matches!(self.status, Status::Installing(_))
    }

    /// Opens a fresh copy and closes this window. Refused while an update is installing.
    pub fn restart(&self, ctx: &egui::Context) -> bool {
        if !self.can_restart() {
            return false;
        }
        restart(ctx);
        true
    }

    pub fn has_news(&self) -> bool {
        self.active && matches!(self.status, Status::Available(_) | Status::Ready(_))
    }

    pub fn pill(&mut self, ui: &mut egui::Ui) {
        if !self.active {
            return;
        }
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
            Status::Stuck(_) => (STUCK_LABEL.to_string(), None),
            Status::Failed(_) => (FAILED_LABEL.to_string(), None),
            _ => return,
        };
        let trouble = matches!(self.status, Status::Stuck(_) | Status::Failed(_));
        let (fill, ink) = if trouble {
            (theme::with_alpha(theme::modified(), 40), theme::modified())
        } else {
            (theme::accent(), theme::on_accent())
        };
        let button = egui::Button::new(
            RichText::new(label)
                .size(12.0)
                .family(theme::semibold())
                .color(ink),
        )
        .fill(fill)
        .corner_radius(12);
        let mut response = ui.add(button);
        if let Some(tip) = self.pill_tooltip() {
            response = response.on_hover_text(tip);
        }
        if response.clicked() {
            match &self.status {
                Status::Ready(_) => {
                    self.restart(ui.ctx());
                }
                Status::Stuck(_) => self.fix(),
                Status::Failed(_) => self.check_now(),
                Status::Available(release) => match action {
                    Some(release) => self.install(release),
                    None => ui.ctx().open_url(egui::OpenUrl::new_tab(&release.url)),
                },
                _ => {}
            }
        }
    }

    fn pill_tooltip(&self) -> Option<String> {
        match &self.status {
            Status::Failed(error) => Some(format!("{error}\nClick to try again.")),
            Status::Stuck(path) => Some(stuck_explanation(&self.config.app_name, path)),
            _ => None,
        }
    }

    pub fn settings_section(
        &mut self,
        ui: &mut egui::Ui,
        auto_install: &mut bool,
        enabled: &mut bool,
    ) {
        if !self.active {
            return;
        }
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
            Status::Failed(e) => format!("Update failed: {e}"),
            Status::Stuck(path) => stuck_explanation(&app, path),
        };
        ui.label(RichText::new(status).size(12.0).color(theme::text_muted()));
        ui.horizontal(|ui| {
            if ui.button("Check now").clicked() {
                self.check_now();
            }
            self.pill(ui);
        });
        if let Some(script) = self.repair_script()
            && matches!(self.status, Status::Stuck(_))
        {
            ui.label(
                RichText::new("Fix runs:")
                    .size(12.0)
                    .color(theme::text_faint()),
            );
            ui.label(
                RichText::new(script)
                    .size(11.0)
                    .monospace()
                    .color(theme::text_muted()),
            );
        }
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

const STUCK_LABEL: &str = "Update stuck · Fix";
const FAILED_LABEL: &str = "Update failed";

fn stuck_explanation(app: &str, path: &std::path::Path) -> String {
    format!(
        "A Homebrew upgrade of {app} did not finish and left {}. Fix quits {app}, reinstalls it with Homebrew and opens it again.",
        path.display()
    )
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

    use std::path::PathBuf;

    use kelp_core::update::{Source, UpgradeError};

    use super::{Message, RETRY_AFTER, Release, Status, UpdateConfig, Updater};

    fn release() -> Release {
        Release {
            version: "99.0.0".into(),
            url: String::new(),
        }
    }

    fn updater() -> Updater {
        let config = UpdateConfig {
            source: Source::new("Hobo-Ware/kelp", "hobo-ware/tap/kelp"),
            app_name: "Kelp".into(),
            version: "0.8.19".into(),
            state_dir: std::env::temp_dir().join("kelp-updater-test"),
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
        assert_eq!(
            updater.pill_tooltip().as_deref(),
            Some("offline\nClick to try again.")
        );
    }

    #[test]
    fn a_failed_install_keeps_the_brew_error() {
        let mut updater = updater();
        let error = UpgradeError::Failed("brew upgrade failed: no".into());
        updater
            .tx
            .send(Message::Installed(release(), Err(error)))
            .unwrap();
        updater.tick(false, false);
        assert_eq!(
            updater.status,
            Status::Failed("brew upgrade failed: no".into())
        );
    }

    #[test]
    fn a_stuck_upgrade_shows_in_the_status() {
        let path = PathBuf::from("/tmp/x/kelp/1.0.upgrading");
        for message in [
            Message::Stuck(path.clone()),
            Message::Installed(release(), Err(UpgradeError::Stuck(path.clone()))),
        ] {
            let mut updater = updater();
            updater.tx.send(message).unwrap();
            updater.tick(false, false);
            assert_eq!(updater.status, Status::Stuck(path.clone()));
        }
        let tip = updater().pill_tooltip();
        assert_eq!(tip, None);
        let mut stuck = updater();
        stuck.status = Status::Stuck(path);
        let tip = stuck.pill_tooltip().unwrap();
        assert!(tip.contains("/tmp/x/kelp/1.0.upgrading"));
        assert!(tip.contains("Fix quits Kelp"));
    }

    #[test]
    fn a_restart_is_refused_while_installing() {
        let mut updater = updater();
        assert!(updater.can_restart());
        updater.status = Status::Installing(release());
        assert!(!updater.can_restart());
        assert!(!updater.restart(&egui::Context::default()));
    }

    #[test]
    fn fixing_while_installing_does_nothing() {
        let mut updater = updater();
        updater.status = Status::Installing(release());
        updater.fix();
        assert!(!updater.take_quit_request());
        assert_eq!(updater.status, Status::Installing(release()));
    }

    #[test]
    fn the_quit_request_is_handed_over_once() {
        let mut updater = updater();
        updater.quit_requested = true;
        assert!(updater.take_quit_request());
        assert!(!updater.take_quit_request());
    }

    #[test]
    fn fixing_outside_an_app_bundle_says_so() {
        let mut updater = updater();
        updater.fix();
        assert_eq!(
            updater.status,
            Status::Failed("Not running from an app bundle".into())
        );
        assert!(!updater.take_quit_request());
    }

    #[test]
    fn a_disabled_updater_stays_silent() {
        let mut updater = updater();
        updater.disable();
        updater.status = Status::Available(Release {
            version: "99.0.0".into(),
            url: String::new(),
        });
        updater.next_check = Instant::now();
        updater.tick(true, true);
        updater.check_now();
        assert!(!updater.is_active());
        assert!(!updater.has_news());
        assert!(updater.last_check.is_none());
        assert!(matches!(updater.status, Status::Available(_)));
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
