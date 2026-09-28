use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use eframe::egui::{self, Color32, Margin, RichText, Stroke, vec2};

use crate::theme;

#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub phase: String,
    pub percent: Option<f32>,
}

pub fn parse_progress(line: &str) -> Option<Progress> {
    let line = line.trim();
    let line = line.strip_prefix("remote:").map_or(line, str::trim);
    if line.starts_with("Cloning into") {
        return Some(Progress {
            phase: "Connecting".into(),
            percent: None,
        });
    }
    let (phase, rest) = line.split_once(':')?;
    let percent = rest
        .split_whitespace()
        .find_map(|word| word.strip_suffix('%'))
        .and_then(|n| n.parse::<f32>().ok())?;
    Some(Progress {
        phase: phase.trim().to_string(),
        percent: Some(percent / 100.0),
    })
}

pub fn progress_lines(buffer: &str) -> impl Iterator<Item = &str> {
    buffer.split(['\r', '\n']).filter(|l| !l.trim().is_empty())
}

pub fn repo_name(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    let last = trimmed.rsplit(['/', ':', '\\']).next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    (!name.is_empty() && name != "." && name != "..").then(|| name.to_string())
}

pub fn looks_like_git_url(text: &str) -> bool {
    let text = text.trim();
    !text.contains(char::is_whitespace)
        && (text.starts_with("https://")
            || text.starts_with("http://")
            || text.starts_with("ssh://")
            || text.starts_with("git://")
            || text.starts_with("file://")
            || (text.contains('@') && text.contains(':')))
        && repo_name(text).is_some()
}

pub fn explain_failure(stderr: &str) -> String {
    let last = stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty() && parse_progress(l).is_none())
        .unwrap_or("git clone failed");
    let auth = [
        "Permission denied",
        "Authentication failed",
        "could not read Username",
        "Repository not found",
        "Host key verification failed",
    ]
    .iter()
    .any(|needle| stderr.contains(needle));
    if auth {
        format!(
            "{last}\nKelp clones with your SSH keys and git credential helper. Check that the same URL clones in a terminal."
        )
    } else {
        last.to_string()
    }
}

pub fn default_parent(recent: Option<&Path>) -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    if let Some(parent) = recent.and_then(Path::parent).filter(|p| p.is_dir()) {
        return parent.to_path_buf();
    }
    let git = home.join("Git");
    if git.is_dir() { git } else { home }
}

pub enum Message {
    Progress(Progress),
    Done(Result<PathBuf, String>),
}

pub fn spawn_clone(
    url: String,
    dest: PathBuf,
    cancel: Arc<AtomicBool>,
    ctx: egui::Context,
) -> Receiver<Message> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = run_clone(&url, &dest, &cancel, &tx, &ctx);
        let _ = tx.send(Message::Done(result));
        ctx.request_repaint();
    });
    rx
}

pub fn run_clone(
    url: &str,
    dest: &Path,
    cancel: &AtomicBool,
    tx: &Sender<Message>,
    ctx: &egui::Context,
) -> Result<PathBuf, String> {
    if dest.exists() {
        return Err(format!("{} already exists", dest.display()));
    }
    let mut child = Command::new("git")
        .args(["clone", "--progress", url])
        .arg(dest)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start git: {e}"))?;
    let mut stderr = child.stderr.take().ok_or("git stderr unavailable")?;
    let reader_tx = tx.clone();
    let reader_ctx = ctx.clone();
    let reader = std::thread::spawn(move || {
        let mut all = String::new();
        let mut chunk = [0u8; 4096];
        while let Ok(n) = stderr.read(&mut chunk) {
            if n == 0 {
                break;
            }
            let text = String::from_utf8_lossy(&chunk[..n]);
            all.push_str(&text);
            if let Some(progress) = progress_lines(&text).filter_map(parse_progress).last() {
                let _ = reader_tx.send(Message::Progress(progress));
                reader_ctx.request_repaint();
            }
        }
        all
    });
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            let _ = std::fs::remove_dir_all(dest);
            return Err("Clone cancelled".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(80)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let stderr = reader.join().unwrap_or_default();
    if status.success() {
        Ok(dest.to_path_buf())
    } else {
        let _ = std::fs::remove_dir_all(dest);
        Err(explain_failure(&stderr))
    }
}

pub fn init_repo(dir: &Path) -> anyhow::Result<()> {
    if dir.join(".git").exists() {
        return Ok(());
    }
    kelp_core::git_cli::run(dir, &["init", "-b", "main"]).map(|_| ())
}

struct Running {
    rx: Receiver<Message>,
    cancel: Arc<AtomicBool>,
    progress: Progress,
}

pub struct CloneDialog {
    url: String,
    parent: PathBuf,
    name: String,
    name_edited: bool,
    running: Option<Running>,
    error: Option<String>,
    focus: bool,
}

pub enum Outcome {
    Keep,
    Close,
    Cloned(PathBuf),
}

impl CloneDialog {
    pub fn new(recent: Option<&Path>) -> Self {
        let mut dialog = Self {
            url: String::new(),
            parent: default_parent(recent),
            name: String::new(),
            name_edited: false,
            running: None,
            error: None,
            focus: true,
        };
        if let Some(pasted) = clipboard_text().filter(|t| looks_like_git_url(t)) {
            dialog.set_url(pasted.trim().to_string());
        }
        dialog
    }

    fn set_url(&mut self, url: String) {
        self.url = url;
        if !self.name_edited {
            self.name = repo_name(&self.url).unwrap_or_default();
        }
    }

    fn dest(&self) -> PathBuf {
        self.parent.join(self.name.trim())
    }

    fn poll(&mut self) -> Option<PathBuf> {
        let running = self.running.as_mut()?;
        let mut done = None;
        for message in running.rx.try_iter() {
            match message {
                Message::Progress(p) => running.progress = p,
                Message::Done(result) => done = Some(result),
            }
        }
        match done? {
            Ok(path) => {
                self.running = None;
                Some(path)
            }
            Err(e) => {
                self.running = None;
                self.error = Some(e);
                None
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome {
        if let Some(path) = self.poll() {
            return Outcome::Cloned(path);
        }
        let mut outcome = Outcome::Keep;
        let busy = self.running.is_some();
        let modal = egui::Modal::new(egui::Id::new("kelp-clone"))
            .frame(
                egui::Frame::new()
                    .fill(theme::POPUP)
                    .stroke(Stroke::new(1.0, theme::POPUP_BORDER))
                    .corner_radius(10)
                    .inner_margin(Margin::same(22)),
            )
            .show(ctx, |ui| {
                ui.set_width(460.0);
                ui.spacing_mut().item_spacing.y = 10.0;
                ui.label(
                    RichText::new("Clone a repository")
                        .size(17.0)
                        .family(theme::semibold())
                        .color(theme::TEXT_STRONG),
                );
                ui.add_enabled_ui(!busy, |ui| {
                    ui.label(RichText::new("URL").size(12.0).color(theme::TEXT_MUTED));
                    let mut url = self.url.clone();
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut url)
                            .hint_text(
                                "https://github.com/owner/repo.git or git@host:owner/repo.git",
                            )
                            .desired_width(f32::INFINITY),
                    );
                    if self.focus {
                        edit.request_focus();
                        self.focus = false;
                    }
                    if url != self.url {
                        self.set_url(url);
                        self.error = None;
                    }
                    ui.label(RichText::new("Folder").size(12.0).color(theme::TEXT_MUTED));
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(crate::recents::tilde(&self.parent)).color(theme::TEXT),
                        );
                        if ui.small_button("Change…").clicked()
                            && let Some(dir) = rfd::FileDialog::new()
                                .set_title("Clone into")
                                .set_directory(&self.parent)
                                .pick_folder()
                        {
                            self.parent = dir;
                        }
                    });
                    ui.label(RichText::new("Name").size(12.0).color(theme::TEXT_MUTED));
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.name).desired_width(f32::INFINITY),
                        )
                        .changed()
                    {
                        self.name_edited = true;
                    }
                });
                if let Some(running) = &self.running {
                    let bar = egui::ProgressBar::new(running.progress.percent.unwrap_or(0.0))
                        .desired_height(6.0)
                        .fill(theme::ACCENT)
                        .animate(running.progress.percent.is_none());
                    ui.add(bar);
                    let text = match running.progress.percent {
                        Some(p) => format!("{} {:.0}%", running.progress.phase, p * 100.0),
                        None => format!("{}…", running.progress.phase),
                    };
                    ui.label(RichText::new(text).size(12.0).color(theme::TEXT_MUTED));
                } else if let Some(error) = &self.error {
                    ui.label(RichText::new(error).size(12.0).color(theme::DELETED));
                } else if !self.url.trim().is_empty() {
                    ui.label(
                        RichText::new(format!(
                            "git clone {} {}",
                            self.url.trim(),
                            crate::recents::tilde(&self.dest())
                        ))
                        .size(11.5)
                        .monospace()
                        .color(theme::TEXT_FAINT),
                    );
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if busy {
                            if ui.button("Cancel clone").clicked()
                                && let Some(running) = &self.running
                            {
                                running.cancel.store(true, Ordering::Relaxed);
                            }
                            return;
                        }
                        let ready = !self.url.trim().is_empty() && !self.name.trim().is_empty();
                        let clone = egui::Button::new(
                            RichText::new("Clone")
                                .family(theme::semibold())
                                .color(Color32::from_rgb(0x10, 0x13, 0x1a)),
                        )
                        .fill(theme::ACCENT)
                        .min_size(vec2(90.0, 30.0));
                        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if ui.add_enabled(ready, clone).clicked() || (ready && enter) {
                            let cancel = Arc::new(AtomicBool::new(false));
                            let rx = spawn_clone(
                                self.url.trim().to_string(),
                                self.dest(),
                                cancel.clone(),
                                ui.ctx().clone(),
                            );
                            self.error = None;
                            self.running = Some(Running {
                                rx,
                                cancel,
                                progress: Progress {
                                    phase: "Starting".into(),
                                    percent: None,
                                },
                            });
                        }
                        if ui.button("Cancel").clicked() {
                            outcome = Outcome::Close;
                        }
                    });
                });
            });
        if modal.should_close() && !busy {
            outcome = Outcome::Close;
        }
        outcome
    }
}

#[cfg(target_os = "macos")]
fn clipboard_text() -> Option<String> {
    if crate::settings::is_dev_run() {
        return std::env::var("KELP_CLONE_URL").ok();
    }
    let out = Command::new("pbpaste").output().ok()?;
    String::from_utf8(out.stdout).ok()
}

#[cfg(not(target_os = "macos"))]
fn clipboard_text() -> Option<String> {
    std::env::var("KELP_CLONE_URL").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_from_urls() {
        let cases = [
            ("https://github.com/Hobo-Ware/kelp.git", "kelp"),
            ("https://github.com/Hobo-Ware/kelp", "kelp"),
            ("https://github.com/Hobo-Ware/kelp/", "kelp"),
            ("git@github.com:Hobo-Ware/kelp.git", "kelp"),
            ("git@example.com:kelp", "kelp"),
            ("ssh://git@host:2222/team/app.git", "app"),
            ("file:///tmp/remotes/demo.git", "demo"),
            ("  https://host/a/b.git  ", "b"),
        ];
        for (url, name) in cases {
            assert_eq!(repo_name(url).as_deref(), Some(name), "{url}");
        }
        assert_eq!(repo_name(""), None);
        assert_eq!(repo_name("https://host/"), Some("host".into()));
    }

    #[test]
    fn git_urls_are_recognized_on_paste() {
        assert!(looks_like_git_url("https://github.com/a/b.git"));
        assert!(looks_like_git_url("git@github.com:a/b.git\n"));
        assert!(!looks_like_git_url("hello world"));
        assert!(!looks_like_git_url("just some text"));
    }

    #[test]
    fn progress_from_real_git_output() {
        let stderr = "Cloning into 'git'...\nremote: Enumerating objects: 373521, done.\nremote: Counting objects:   1% (40/3999)\rremote: Counting objects:  52% (2080/3999)\rremote: Counting objects: 100% (3999/3999), done.\nReceiving objects:  45% (168085/373521), 71.20 MiB | 23.70 MiB/s\rReceiving objects:  46% (171820/373521), 72.51 MiB | 23.59 MiB/s\nResolving deltas:   7% (19771/282433)\rResolving deltas: 100% (282433/282433), done.\nUpdating files:  36% (1600/4444)\r";
        let parsed: Vec<Progress> = progress_lines(stderr).filter_map(parse_progress).collect();
        assert_eq!(parsed[0].phase, "Connecting");
        assert!(parsed.iter().all(|p| !p.phase.starts_with("remote")));
        let receiving: Vec<f32> = parsed
            .iter()
            .filter(|p| p.phase == "Receiving objects")
            .filter_map(|p| p.percent)
            .collect();
        assert_eq!(receiving, [0.45, 0.46]);
        let last = parsed.last().unwrap();
        assert_eq!(last.phase, "Updating files");
        assert!((last.percent.unwrap() - 0.36).abs() < 1e-6);
        assert_eq!(
            parse_progress("remote: Enumerating objects: 373521, done."),
            None
        );
    }

    #[test]
    fn auth_failures_get_a_hint() {
        let stderr = "Cloning into 'x'...\ngit@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository.\n";
        let message = explain_failure(stderr);
        assert!(message.starts_with("fatal: Could not read from remote repository."));
        assert!(message.contains("SSH keys"));
        assert_eq!(
            explain_failure("fatal: destination path 'x' already exists"),
            "fatal: destination path 'x' already exists"
        );
    }

    #[test]
    fn clones_a_local_repository_and_reports_progress() {
        let root = std::env::temp_dir().join(format!("kelp-clone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let source = root.join("source");
        std::fs::create_dir_all(&source).unwrap();
        let git = |dir: &Path, args: &[&str]| kelp_core::git_cli::run(dir, args).unwrap();
        git(&source, &["init", "-q", "-b", "main"]);
        git(&source, &["config", "user.email", "t@example.com"]);
        git(&source, &["config", "user.name", "T"]);
        std::fs::write(source.join("a.txt"), "one").unwrap();
        git(&source, &["add", "."]);
        git(&source, &["commit", "-q", "-m", "first"]);
        git(&root, &["clone", "-q", "--bare", "source", "remote.git"]);
        let url = format!("file://{}", root.join("remote.git").display());
        let dest = root.join(repo_name(&url).unwrap());
        let (tx, rx) = mpsc::channel();
        let cancel = AtomicBool::new(false);
        let cloned = run_clone(&url, &dest, &cancel, &tx, &egui::Context::default()).unwrap();
        assert_eq!(cloned, root.join("remote"));
        assert_eq!(
            std::fs::read_to_string(cloned.join("a.txt")).unwrap(),
            "one"
        );
        drop(tx);
        assert!(rx.try_iter().all(|m| matches!(m, Message::Progress(_))));
        let again = run_clone(
            &url,
            &dest,
            &cancel,
            &mpsc::channel().0,
            &egui::Context::default(),
        );
        assert!(again.unwrap_err().contains("already exists"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn init_makes_a_main_branch_repo() {
        let dir = std::env::temp_dir().join(format!("kelp-init-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        init_repo(&dir).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join(".git/HEAD"))
                .unwrap()
                .trim(),
            "ref: refs/heads/main"
        );
        init_repo(&dir).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
