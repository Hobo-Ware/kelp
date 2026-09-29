use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, CornerRadius, FontId, Rect, RichText, Sense, Ui, pos2, vec2};

use crate::icons::Icon;
use crate::recents::{self, Recent};
use crate::{menus, theme};

const MASCOT: f32 = 132.0;
const LIST_W: f32 = 560.0;
const ROW_H: f32 = 50.0;
const BUTTON_H: f32 = 34.0;

pub enum Action {
    Open(PathBuf),
    PickFolder,
    Clone,
    Init,
    Forget(PathBuf),
    Reveal(PathBuf),
}

struct Info {
    branch: Option<String>,
    exists: bool,
}

#[derive(Default)]
pub struct Welcome {
    filter: String,
    info: HashMap<PathBuf, Info>,
    focus_filter: bool,
    selected: Option<PathBuf>,
    reveal_selected: bool,
}

impl Welcome {
    pub fn reopen(&mut self) {
        self.filter.clear();
        self.info.clear();
        self.focus_filter = true;
        self.selected = None;
    }

    fn keys(&mut self, ctx: &egui::Context, recents: &[Recent]) -> Option<Action> {
        let in_filter = crate::list_keys::is_typing(ctx);
        if !in_filter && !crate::list_keys::nothing_focused(ctx) {
            return None;
        }
        let shown: Vec<&Recent> = filtered(recents, &self.filter).collect();
        let at = self
            .selected
            .as_ref()
            .and_then(|p| shown.iter().position(|r| &r.path == p));
        let step = crate::list_keys::arrow_step(ctx, !in_filter);
        if let Some(next) = step.and_then(|s| crate::list_keys::target(s, at, shown.len())) {
            self.selected = Some(shown[next].path.clone());
            self.reveal_selected = true;
            return None;
        }
        let open = !in_filter && crate::list_keys::pressed(ctx, egui::Key::Enter);
        at.filter(|_| open)
            .map(|at| Action::Open(shown[at].path.clone()))
    }

    fn info(&mut self, path: &Path) -> &Info {
        self.info.entry(path.to_path_buf()).or_insert_with(|| Info {
            branch: recents::branch_of(path),
            exists: path.exists(),
        })
    }

    pub fn ui(&mut self, ui: &mut Ui, recents: &[Recent], waving: bool) -> Option<Action> {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::bg()))
            .show(ui, |ui| {
                let full = ui.max_rect();
                let column = Rect::from_center_size(
                    pos2(full.center().x, full.center().y),
                    vec2(LIST_W.min(full.width() - 48.0), full.height()),
                );
                let top = (full.top() + (full.height() * 0.12).min(90.0)).max(full.top() + 24.0);
                let art = Rect::from_center_size(
                    pos2(full.center().x, top + MASCOT / 2.0),
                    vec2(MASCOT, MASCOT),
                );
                let animate = waving || ui.rect_contains_pointer(art);
                crate::mascot::paint(ui.painter(), art, ui.input(|i| i.time) as f32, animate);
                if animate {
                    ui.ctx().request_repaint();
                }
                let title = ui.painter().layout_no_wrap(
                    "Welcome to Kelp".into(),
                    FontId::new(22.0, theme::semibold()),
                    theme::text_strong(),
                );
                let title_pos = pos2(full.center().x - title.size().x / 2.0, art.bottom() + 12.0);
                ui.painter()
                    .galley(title_pos, title.clone(), theme::text_strong());
                let buttons_top = title_pos.y + title.size().y + 20.0;
                let mut action = self.keys(ui.ctx(), recents);
                action = action.or(self.buttons(ui, column, buttons_top));
                let filter_top = buttons_top + BUTTON_H + 26.0;
                let list_top = filter_top + 44.0;
                if !recents.is_empty() {
                    action = action.or(self.filter_box(ui, column, filter_top, recents));
                }
                let list = Rect::from_min_max(
                    pos2(column.left(), list_top),
                    pos2(column.right(), full.bottom() - 24.0),
                );
                action.or(self.list(ui, list, recents))
            })
            .inner
    }

    fn buttons(&self, ui: &mut Ui, column: Rect, top: f32) -> Option<Action> {
        let widths = [150.0, 110.0, 160.0];
        let gap = 10.0;
        let total: f32 = widths.iter().sum::<f32>() + gap * 2.0;
        let mut x = column.center().x - total / 2.0;
        let mut picked = None;
        let labels = [
            ("Open folder…", Action::PickFolder, true),
            ("Clone…", Action::Clone, false),
            ("New repository…", Action::Init, false),
        ];
        for ((label, act, primary), w) in labels.into_iter().zip(widths) {
            let rect = Rect::from_min_size(pos2(x, top), vec2(w, BUTTON_H));
            x += w + gap;
            let text = if primary {
                RichText::new(label)
                    .family(theme::semibold())
                    .color(theme::on_accent())
            } else {
                RichText::new(label).color(theme::text())
            };
            let mut button = egui::Button::new(text).corner_radius(8);
            if primary {
                button = button.fill(theme::accent());
            }
            if ui.put(rect, button).clicked() {
                picked = Some(act);
            }
        }
        picked
    }

    fn filter_box(
        &mut self,
        ui: &mut Ui,
        column: Rect,
        top: f32,
        recents: &[Recent],
    ) -> Option<Action> {
        let rect = Rect::from_min_size(pos2(column.left(), top), vec2(column.width(), 30.0));
        let edit = ui.put(
            rect,
            egui::TextEdit::singleline(&mut self.filter)
                .hint_text("Filter recent repositories")
                .margin(egui::Margin::symmetric(10, 6)),
        );
        if std::mem::take(&mut self.focus_filter) {
            edit.request_focus();
        }
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if !enter {
            return None;
        }
        let shown: Vec<&Recent> = filtered(recents, &self.filter).collect();
        shown
            .iter()
            .find(|r| self.selected.as_ref() == Some(&r.path))
            .or(shown.first())
            .map(|r| Action::Open(r.path.clone()))
    }

    fn list(&mut self, ui: &mut Ui, area: Rect, recents: &[Recent]) -> Option<Action> {
        if recents.is_empty() {
            ui.painter().text(
                pos2(area.center().x, area.top() + 4.0),
                Align2::CENTER_TOP,
                "Repositories you open show up here.",
                FontId::proportional(13.0),
                theme::text_faint(),
            );
            return None;
        }
        let shown: Vec<Recent> = filtered(recents, &self.filter).cloned().collect();
        if shown.is_empty() {
            ui.painter().text(
                pos2(area.center().x, area.top() + 4.0),
                Align2::CENTER_TOP,
                "No recent repository matches.",
                FontId::proportional(13.0),
                theme::text_faint(),
            );
            return None;
        }
        let now = recents::now();
        let mut action = None;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(&mut child, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for recent in &shown {
                    let info = self.info(&recent.path);
                    let (branch, exists) = (info.branch.clone(), info.exists);
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                    let selected = self.selected.as_ref() == Some(&recent.path);
                    if selected && std::mem::take(&mut self.reveal_selected) {
                        ui.scroll_to_rect(rect, None);
                    }
                    paint_row(
                        ui,
                        rect,
                        recent,
                        branch.as_deref(),
                        exists,
                        response.hovered() || selected,
                        now,
                    );
                    crate::widgets::focus_ring(ui, &response, 8.0);
                    if response.clicked() {
                        action = Some(Action::Open(recent.path.clone()));
                    }
                    crate::menus::context_menu(&response, |ui| {
                        crate::menus::menu_width(ui, 200.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if menus::row(ui, Some(Icon::Folder), "Reveal in Finder", None, false) {
                            action = Some(Action::Reveal(recent.path.clone()));
                        }
                        menus::separator(ui);
                        if menus::row(ui, Some(Icon::Trash), "Remove from list", None, true) {
                            action = Some(Action::Forget(recent.path.clone()));
                        }
                    });
                }
            });
        action
    }
}

fn filtered<'a>(recents: &'a [Recent], filter: &str) -> impl Iterator<Item = &'a Recent> {
    let needle = filter.trim().to_lowercase();
    recents.iter().filter(move |r| {
        needle.is_empty() || r.path.to_string_lossy().to_lowercase().contains(&needle)
    })
}

fn paint_row(
    ui: &Ui,
    rect: Rect,
    recent: &Recent,
    branch: Option<&str>,
    exists: bool,
    hovered: bool,
    now: i64,
) {
    let painter = ui.painter_at(rect);
    if hovered {
        painter.rect_filled(rect, CornerRadius::same(8), theme::control());
    }
    let name = recent.path.file_name().map_or_else(
        || recent.path.display().to_string(),
        |n| n.to_string_lossy().to_string(),
    );
    let strong = if exists {
        theme::text_strong()
    } else {
        theme::text_faint()
    };
    painter.text(
        pos2(rect.left() + 14.0, rect.top() + 9.0),
        Align2::LEFT_TOP,
        name,
        FontId::new(14.0, theme::semibold()),
        strong,
    );
    let path = if exists {
        recents::tilde(&recent.path)
    } else {
        format!("{}  ·  missing", recents::tilde(&recent.path))
    };
    let when = kelp_core::commit::relative_time(recent.opened, now);
    let when = painter.layout_no_wrap(when, FontId::proportional(11.5), theme::text_faint());
    let when_w = when.size().x;
    painter.galley(
        pos2(
            rect.right() - 14.0 - when_w,
            rect.bottom() - 9.0 - when.size().y,
        ),
        when,
        theme::text_faint(),
    );
    let path = crate::graph_view::truncated(
        &painter,
        path,
        FontId::proportional(12.0),
        theme::text_faint(),
        (rect.width() - 28.0 - when_w - 16.0).max(0.0),
    );
    painter.galley(
        pos2(rect.left() + 14.0, rect.bottom() - 9.0 - path.size().y),
        path,
        theme::text_faint(),
    );
    if let Some(branch) = branch {
        let branch = crate::graph_view::truncated(
            &painter,
            branch.to_string(),
            FontId::proportional(12.0),
            theme::lanes()[0],
            rect.width() * 0.45,
        );
        painter.galley(
            pos2(rect.right() - 14.0 - branch.size().x, rect.top() + 10.0),
            branch,
            theme::lanes()[0],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(
        ctx: &egui::Context,
        welcome: &mut Welcome,
        recents: &[Recent],
        key: Option<egui::Key>,
    ) -> Option<Action> {
        let events = key
            .map(|key| {
                [true, false]
                    .map(|pressed| egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    })
                    .to_vec()
            })
            .unwrap_or_default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 900.0))),
            events,
            ..Default::default()
        };
        let mut action = None;
        let _ = ctx.run_ui(input, |ui| action = welcome.ui(ui, recents, false));
        action
    }

    fn opened(action: Option<Action>) -> Option<PathBuf> {
        match action {
            Some(Action::Open(path)) => Some(path),
            _ => None,
        }
    }

    #[test]
    fn arrows_pick_a_recent_repository_and_enter_opens_it() {
        let recents: Vec<Recent> = ["/r/one", "/r/two", "/r/three"]
            .iter()
            .map(|p| Recent {
                path: PathBuf::from(p),
                opened: 0,
            })
            .collect();
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut welcome = Welcome::default();
        welcome.reopen();
        press(&ctx, &mut welcome, &recents, None);
        press(&ctx, &mut welcome, &recents, None);
        assert!(
            crate::list_keys::is_typing(&ctx),
            "the filter has focus on open"
        );

        press(&ctx, &mut welcome, &recents, Some(egui::Key::ArrowDown));
        press(&ctx, &mut welcome, &recents, Some(egui::Key::ArrowDown));
        assert_eq!(welcome.selected, Some(PathBuf::from("/r/two")));
        let from_filter = press(&ctx, &mut welcome, &recents, Some(egui::Key::Enter));
        assert_eq!(opened(from_filter), Some(PathBuf::from("/r/two")));

        press(&ctx, &mut welcome, &recents, Some(egui::Key::End));
        assert_eq!(welcome.selected, Some(PathBuf::from("/r/three")));
        let from_list = press(&ctx, &mut welcome, &recents, Some(egui::Key::Enter));
        assert_eq!(opened(from_list), Some(PathBuf::from("/r/three")));
    }

    #[test]
    fn filter_matches_any_part_of_the_path_ignoring_case() {
        let recents: Vec<Recent> = ["/Users/v/Git/Hoboware/kelp", "/Users/v/Git/trakt-web"]
            .iter()
            .map(|p| Recent {
                path: PathBuf::from(p),
                opened: 0,
            })
            .collect();
        let names = |f: &str| filtered(&recents, f).count();
        assert_eq!(names(""), 2);
        assert_eq!(names("TRAKT"), 1);
        assert_eq!(names("hobo"), 1);
        assert_eq!(names("git"), 2);
        assert_eq!(names("nope"), 0);
    }
}
