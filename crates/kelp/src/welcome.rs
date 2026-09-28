use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Ui, pos2, vec2,
};

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
}

impl Welcome {
    pub fn reopen(&mut self) {
        self.filter.clear();
        self.info.clear();
        self.focus_filter = true;
    }

    fn info(&mut self, path: &Path) -> &Info {
        self.info.entry(path.to_path_buf()).or_insert_with(|| Info {
            branch: recents::branch_of(path),
            exists: path.exists(),
        })
    }

    pub fn ui(&mut self, ui: &mut Ui, recents: &[Recent], waving: bool) -> Option<Action> {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BG))
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
                    theme::TEXT_STRONG,
                );
                let title_pos = pos2(full.center().x - title.size().x / 2.0, art.bottom() + 12.0);
                ui.painter()
                    .galley(title_pos, title.clone(), theme::TEXT_STRONG);
                let buttons_top = title_pos.y + title.size().y + 20.0;
                let mut action = self.buttons(ui, column, buttons_top);
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
                    .color(Color32::from_rgb(0x10, 0x13, 0x1a))
            } else {
                RichText::new(label).color(theme::TEXT)
            };
            let mut button = egui::Button::new(text).corner_radius(8);
            if primary {
                button = button.fill(theme::ACCENT);
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
        enter
            .then(|| filtered(recents, &self.filter).next())
            .flatten()
            .map(|r| Action::Open(r.path.clone()))
    }

    fn list(&mut self, ui: &mut Ui, area: Rect, recents: &[Recent]) -> Option<Action> {
        if recents.is_empty() {
            ui.painter().text(
                pos2(area.center().x, area.top() + 4.0),
                Align2::CENTER_TOP,
                "Repositories you open show up here.",
                FontId::proportional(13.0),
                theme::TEXT_FAINT,
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
                theme::TEXT_FAINT,
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
                    paint_row(
                        ui,
                        rect,
                        recent,
                        branch.as_deref(),
                        exists,
                        response.hovered(),
                        now,
                    );
                    if response.clicked() {
                        action = Some(Action::Open(recent.path.clone()));
                    }
                    response.context_menu(|ui| {
                        ui.set_min_width(200.0);
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
        painter.rect_filled(rect, CornerRadius::same(8), theme::CONTROL);
    }
    let name = recent.path.file_name().map_or_else(
        || recent.path.display().to_string(),
        |n| n.to_string_lossy().to_string(),
    );
    let strong = if exists {
        theme::TEXT_STRONG
    } else {
        theme::TEXT_FAINT
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
    painter.text(
        pos2(rect.left() + 14.0, rect.bottom() - 9.0),
        Align2::LEFT_BOTTOM,
        path,
        FontId::proportional(12.0),
        theme::TEXT_FAINT,
    );
    let when = kelp_core::commit::relative_time(recent.opened, now);
    painter.text(
        pos2(rect.right() - 14.0, rect.bottom() - 9.0),
        Align2::RIGHT_BOTTOM,
        when,
        FontId::proportional(11.5),
        theme::TEXT_FAINT,
    );
    if let Some(branch) = branch {
        painter.text(
            pos2(rect.right() - 14.0, rect.top() + 10.0),
            Align2::RIGHT_TOP,
            branch,
            FontId::proportional(12.0),
            theme::LANES[0],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
