use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};

use eframe::egui::{self, Align2, FontId, Rect, RichText, Sense, Ui, pos2, vec2};
use gix::ObjectId;
use kelp_core::blame::{self, Blame};

use crate::avatars::AvatarStore;
use crate::graph_view::{self, TooltipCommit};
use crate::menus;
use crate::theme;

const LINE_H: f32 = 22.0;
const GUTTER_W: f32 = 250.0;
const NUM_W: f32 = 46.0;
const AVATAR_R: f32 = 8.0;

struct Target {
    rev: Option<ObjectId>,
    path: String,
}

enum State {
    Loading(Receiver<Result<Option<Blame>, String>>),
    Ready(Loaded),
    Failed(String),
}

struct Loaded {
    blame: Blame,
    lines: Vec<String>,
    band: Vec<bool>,
}

pub struct BlameView {
    ctx: egui::Context,
    dir: PathBuf,
    target: Target,
    earlier: Vec<Target>,
    state: State,
    cancel: Arc<AtomicBool>,
}

pub enum Event {
    None,
    Reveal(ObjectId),
}

enum Click {
    Reveal(ObjectId),
    Before(ObjectId, String),
    Copy(ObjectId),
}

impl BlameView {
    pub fn new(ctx: &egui::Context, dir: PathBuf, rev: Option<ObjectId>, path: &str) -> Self {
        let mut view = Self {
            ctx: ctx.clone(),
            dir,
            target: Target {
                rev,
                path: path.to_string(),
            },
            earlier: Vec::new(),
            state: State::Failed(String::new()),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        view.start();
        view
    }

    pub fn restart(&mut self) {
        self.start();
    }

    fn start(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (dir, rev, path) = (self.dir.clone(), self.target.rev, self.target.path.clone());
        let cancel = self.cancel.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let result = blame::run(&dir, rev, &path, &cancel).map_err(|e| format!("{e:#}"));
            if tx.send(result).is_ok() {
                ctx.request_repaint();
            }
        });
        self.state = State::Loading(rx);
    }

    fn poll(&mut self) {
        let State::Loading(rx) = &self.state else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(Some(blame))) => self.state = State::Ready(Loaded::new(blame)),
            Ok(Ok(None)) => {}
            Ok(Err(e)) => self.state = State::Failed(e),
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.state = State::Failed("blame stopped".into())
            }
        }
    }

    fn step_back(&mut self, before: ObjectId, path: String) {
        let current = std::mem::replace(
            &mut self.target,
            Target {
                rev: Some(before),
                path,
            },
        );
        self.earlier.push(current);
        self.start();
    }

    fn step_forward(&mut self) {
        if let Some(target) = self.earlier.pop() {
            self.target = target;
            self.start();
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, avatars: &mut AvatarStore) -> Event {
        self.poll();
        if !self.earlier.is_empty() {
            self.history_bar(ui);
        }
        let loaded = match &self.state {
            State::Loading(_) => return notice(ui, "Working out who wrote each line…"),
            State::Failed(e) => return notice(ui, &format!("Could not blame this file: {e}")),
            State::Ready(loaded) => loaded,
        };
        let now = now();
        let mut click = None;
        let font = FontId::monospace(12.5);
        egui::ScrollArea::both().auto_shrink(false).show_rows(
            ui,
            LINE_H,
            loaded.lines.len(),
            |ui, rows| {
                let width = ui.available_width().max(900.0);
                let (area, _) =
                    ui.allocate_exact_size(vec2(width, LINE_H * rows.len() as f32), Sense::hover());
                let painter = ui.painter_at(area);
                for (i, line) in rows.clone().enumerate() {
                    let top = area.top() + i as f32 * LINE_H;
                    let row = Rect::from_min_size(pos2(area.left(), top), vec2(width, LINE_H));
                    let gutter = Rect::from_min_size(row.min, vec2(GUTTER_W, LINE_H));
                    let Some(origin) = loaded.blame.origin(line) else {
                        continue;
                    };
                    if loaded.band[line] {
                        painter.rect_filled(gutter, 0.0, theme::band());
                    }
                    let response = ui
                        .interact(gutter, ui.id().with(("blame", line)), Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if loaded.blame.starts_block(line) || line == rows.start {
                        paint_origin(&painter, gutter, origin, avatars, now);
                    }
                    if !origin.is_uncommitted() {
                        if response.clicked() {
                            click = Some(Click::Reveal(origin.id));
                        }
                        let tooltip = TooltipCommit {
                            title: &origin.summary,
                            body: "",
                            author: &origin.author,
                            email: &origin.email,
                            time: origin.time,
                            id: origin.id,
                        };
                        let response = response.clone().on_hover_ui_at_pointer(|ui| {
                            graph_view::commit_tooltip(ui, &tooltip, now)
                        });
                        crate::menus::context_menu(&response, |ui| {
                            ui.set_min_width(230.0);
                            ui.spacing_mut().item_spacing.y = 0.0;
                            if let Some((before, path)) = &origin.previous
                                && menus::row(ui, None, "Blame before this commit", None, false)
                            {
                                click = Some(Click::Before(*before, path.clone()));
                                ui.close();
                            }
                            if menus::row(ui, None, "Show in the graph", None, false) {
                                click = Some(Click::Reveal(origin.id));
                                ui.close();
                            }
                            if menus::row(ui, None, "Copy commit hash", None, false) {
                                click = Some(Click::Copy(origin.id));
                                ui.close();
                            }
                        });
                    }
                    painter.text(
                        pos2(gutter.right() + NUM_W - 8.0, row.center().y),
                        Align2::RIGHT_CENTER,
                        (line + 1).to_string(),
                        font.clone(),
                        theme::line_number(),
                    );
                    painter.text(
                        pos2(gutter.right() + NUM_W + 12.0, row.center().y),
                        Align2::LEFT_CENTER,
                        &loaded.lines[line],
                        font.clone(),
                        theme::text_body(),
                    );
                }
            },
        );
        match click {
            Some(Click::Reveal(id)) => return Event::Reveal(id),
            Some(Click::Before(before, path)) => self.step_back(before, path),
            Some(Click::Copy(id)) => ui.ctx().copy_text(id.to_string()),
            None => {}
        }
        Event::None
    }

    fn history_bar(&mut self, ui: &mut Ui) {
        let mut forward = false;
        egui::Frame::new()
            .fill(theme::panel())
            .inner_margin(egui::Margin::symmetric(16, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let at = self.target.rev.map_or("the work tree".to_string(), |id| {
                        id.to_hex_with_len(7).to_string()
                    });
                    ui.label(
                        RichText::new(format!("Blame before {at} · {}", self.target.path))
                            .size(12.0)
                            .color(theme::text_muted()),
                    );
                    if ui.button(RichText::new("Back").size(12.0)).clicked() {
                        forward = true;
                    }
                });
            });
        if forward {
            self.step_forward();
        }
    }
}

impl Drop for BlameView {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Loaded {
    fn new(blame: Blame) -> Self {
        let lines = blame
            .lines
            .iter()
            .map(|l| l.text.replace('\t', "    "))
            .collect();
        let mut band = Vec::with_capacity(blame.lines.len());
        let mut on = false;
        for i in 0..blame.lines.len() {
            if i > 0 && blame.starts_block(i) {
                on = !on;
            }
            band.push(on);
        }
        Self { blame, lines, band }
    }
}

fn paint_origin(
    painter: &egui::Painter,
    gutter: Rect,
    origin: &blame::Origin,
    avatars: &mut AvatarStore,
    now: i64,
) {
    let mid = gutter.center().y;
    let (texture, hash, color) = if origin.is_uncommitted() {
        (None, "not committed".to_string(), theme::modified())
    } else {
        (
            avatars.texture(&origin.email, origin.id),
            origin.id.to_hex_with_len(7).to_string(),
            theme::text_muted(),
        )
    };
    graph_view::draw_avatar(
        painter,
        pos2(gutter.left() + 18.0, mid),
        AVATAR_R,
        &origin.author,
        &origin.email,
        texture,
        theme::border(),
        false,
    );
    let when = kelp_core::commit::relative_time(origin.time, now);
    let when = painter.layout_no_wrap(when, FontId::proportional(11.5), theme::text_faint());
    let when_w = when.size().x;
    painter.galley(
        pos2(gutter.right() - 10.0 - when_w, mid - when.size().y / 2.0),
        when,
        theme::text_faint(),
    );
    let hash = painter.layout_no_wrap(hash, FontId::monospace(11.5), color);
    let hash_w = hash.size().x;
    painter.galley(
        pos2(gutter.left() + 34.0, mid - hash.size().y / 2.0),
        hash,
        color,
    );
    let author = graph_view::truncated(
        painter,
        origin.author.clone(),
        FontId::proportional(12.0),
        theme::text(),
        (gutter.width() - 34.0 - hash_w - 10.0 - when_w - 22.0).max(0.0),
    );
    painter.galley(
        pos2(
            gutter.left() + 34.0 + hash_w + 8.0,
            mid - author.size().y / 2.0,
        ),
        author,
        theme::text(),
    );
}

fn notice(ui: &mut Ui, text: &str) -> Event {
    ui.centered_and_justified(|ui| ui.label(RichText::new(text).color(theme::text_muted())));
    Event::None
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use eframe::egui::{Event as Input, PointerButton, Pos2, RawInput};

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kelp-blame-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| kelp_core::git_cli::run(&dir, args).unwrap();
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test"]);
        std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "first"]);
        dir
    }

    fn frame(
        ctx: &egui::Context,
        view: &mut BlameView,
        avatars: &mut AvatarStore,
        events: Vec<Input>,
    ) -> Event {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 300.0))),
            events,
            ..Default::default()
        };
        let mut out = Event::None;
        let _ = ctx.run_ui(input, |ui| out = view.ui(ui, avatars));
        out
    }

    #[test]
    fn clicking_the_gutter_reveals_that_lines_commit() {
        let dir = scratch("reveal");
        let head = gix::open(&dir).unwrap().head_id().unwrap().detach();
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut avatars = AvatarStore::new(ctx.clone(), None);
        avatars.enabled = false;
        let mut view = BlameView::new(&ctx, dir.clone(), None, "a.txt");
        for _ in 0..300 {
            frame(&ctx, &mut view, &mut avatars, vec![]);
            if matches!(view.state, State::Ready(_)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(matches!(view.state, State::Ready(_)), "blame never loaded");
        let at = pos2(40.0, LINE_H / 2.0);
        let press = |pressed| Input::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(&ctx, &mut view, &mut avatars, vec![Input::PointerMoved(at)]);
        frame(&ctx, &mut view, &mut avatars, vec![press(true)]);
        let revealed = frame(&ctx, &mut view, &mut avatars, vec![press(false)]);
        assert!(matches!(revealed, Event::Reveal(id) if id == head));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
