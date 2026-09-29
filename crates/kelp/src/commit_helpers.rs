use std::path::Path;

use eframe::egui::{
    self, CornerRadius, CursorIcon, FontId, Id, Response, RichText, Sense, Stroke, Ui, WidgetInfo,
    WidgetType, pos2, vec2,
};
use kelp_core::commit_message::{self, TYPES};

use crate::repo_view::Repo;
use crate::theme;

const PEOPLE_SHOWN: usize = 8;

pub fn conventional_toggle_id(dir: &Path) -> Id {
    Id::new(("commit-conventional", dir))
}

pub fn type_button_id(dir: &Path) -> Id {
    Id::new(("commit-type", dir))
}

pub fn type_item_id(dir: &Path, kind: &str) -> Id {
    Id::new(("commit-type-item", dir, kind))
}

pub fn more_button_id(dir: &Path) -> Id {
    Id::new(("commit-more", dir))
}

pub fn conventional_toggle(ui: &mut Ui, repo: &mut Repo) {
    let on = repo.uses_conventional();
    let response =
        pill(ui, conventional_toggle_id(&repo.dir), "Conventional", on).on_hover_text(if on {
            "Hide the Conventional Commit type picker"
        } else {
            "Show a Conventional Commit type picker"
        });
    if response.clicked() {
        repo.set_conventional(!on);
    }
}

pub fn type_row(ui: &mut Ui, repo: &mut Repo) {
    if !repo.uses_conventional() {
        return;
    }
    let (prefix, _) = commit_message::split_prefix(&repo.commit_summary);
    let mut next = prefix.clone().unwrap_or_default();
    let dir = repo.dir.clone();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let label = if next.kind.is_empty() {
            "type"
        } else {
            next.kind.as_str()
        };
        let button = pill_with_caret(ui, type_button_id(&dir), label, !next.kind.is_empty(), true)
            .on_hover_text("Conventional Commit type");
        egui::Popup::menu(&button).show(|ui| {
            ui.set_min_width(120.0);
            for kind in std::iter::once("").chain(TYPES) {
                let text = if kind.is_empty() { "none" } else { kind };
                if pill(ui, type_item_id(&dir, kind), text, next.kind == kind).clicked() {
                    next.kind = kind.to_string();
                    ui.close();
                }
            }
        });
        let mut scope = next.scope.clone();
        let edit = ui.add_enabled(
            !next.kind.is_empty(),
            egui::TextEdit::singleline(&mut scope)
                .hint_text("scope")
                .desired_width(90.0),
        );
        if edit.changed() {
            next.scope = scope.chars().filter(|c| !"() ".contains(*c)).collect();
        }
        let breaking = pill(ui, Id::new(("commit-breaking", &dir)), "!", next.breaking)
            .on_hover_text("Breaking change");
        if breaking.clicked() && !next.kind.is_empty() {
            next.breaking = !next.breaking;
        }
    });
    if prefix.unwrap_or_default() != next {
        repo.commit_summary = commit_message::with_prefix(&repo.commit_summary, Some(&next));
    }
}

pub fn co_author_row(ui: &mut Ui, repo: &mut Repo) {
    let authors = commit_message::co_authors(&repo.commit_body);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for person in &authors {
            if chip(ui, &person.name, &person.email) {
                repo.commit_body =
                    commit_message::remove_co_author(&repo.commit_body, &person.email);
            }
        }
        let add = pill(
            ui,
            Id::new(("commit-co-author", &repo.dir)),
            "+ Co-author",
            false,
        )
        .on_hover_text("Credit someone from this repository's history");
        egui::Popup::menu(&add)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                if let Some(person) = people_menu(ui, repo, &authors) {
                    repo.commit_body = commit_message::add_co_author(&repo.commit_body, &person);
                    ui.close();
                }
            });
    });
}

fn people_menu(
    ui: &mut Ui,
    repo: &mut Repo,
    added: &[commit_message::Person],
) -> Option<commit_message::Person> {
    ui.set_min_width(240.0);
    let filter_id = Id::new(("commit-co-author-filter", &repo.dir));
    let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
    ui.add(
        egui::TextEdit::singleline(&mut filter)
            .hint_text("Name or email")
            .desired_width(f32::INFINITY),
    )
    .request_focus();
    let needle = filter.trim().to_lowercase();
    ui.data_mut(|d| d.insert_temp(filter_id, filter));
    let matches: Vec<commit_message::Person> = repo
        .people()
        .iter()
        .filter(|p| !added.iter().any(|a| a.email.eq_ignore_ascii_case(&p.email)))
        .filter(|p| {
            needle.is_empty()
                || p.name.to_lowercase().contains(&needle)
                || p.email.to_lowercase().contains(&needle)
        })
        .take(PEOPLE_SHOWN)
        .cloned()
        .collect();
    if matches.is_empty() {
        ui.label(
            RichText::new("No other authors in the history")
                .size(12.0)
                .color(theme::text_faint()),
        );
    }
    let mut picked = None;
    for person in matches {
        let text = format!("{}  <{}>", person.name, person.email);
        if ui.selectable_label(false, text).clicked() {
            picked = Some(person);
        }
    }
    picked
}

fn chip(ui: &mut Ui, name: &str, email: &str) -> bool {
    let mut removed = false;
    egui::Frame::new()
        .fill(theme::control())
        .corner_radius(10)
        .inner_margin(egui::Margin {
            left: 9,
            right: 3,
            top: 1,
            bottom: 1,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.label(RichText::new(name).size(12.0).color(theme::text()))
                .on_hover_text(email);
            removed = crate::widgets::text_button(ui, "×")
                .on_hover_text(format!("Remove {name}"))
                .clicked();
        });
    removed
}

pub fn more_button(ui: &mut Ui, id: Id, enabled: bool) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(rect, id, sense);
    response
        .widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, "More commit options"));
    let fill = if !enabled {
        theme::card_hover()
    } else if response.hovered() {
        theme::with_alpha(theme::accent(), 0xdd)
    } else {
        theme::accent()
    };
    let ink = if enabled {
        theme::on_accent()
    } else {
        theme::text_faint()
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(6), fill);
    let c = rect.center();
    painter.add(egui::Shape::convex_polygon(
        vec![
            c + vec2(-4.5, -2.0),
            c + vec2(4.5, -2.0),
            c + vec2(0.0, 3.0),
        ],
        ink,
        Stroke::NONE,
    ));
    if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

fn pill(ui: &mut Ui, id: Id, text: &str, selected: bool) -> Response {
    pill_with_caret(ui, id, text, selected, false)
}

fn pill_with_caret(ui: &mut Ui, id: Id, text: &str, selected: bool, caret: bool) -> Response {
    let font = FontId::proportional(12.0);
    let width = ui
        .painter()
        .layout_no_wrap(text.to_string(), font.clone(), theme::text())
        .size()
        .x;
    let caret_w = if caret { 12.0 } else { 0.0 };
    let size = vec2(width + 16.0 + caret_w, 22.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    response.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, selected, text));
    let (fill, ink) = match (selected, response.hovered()) {
        (true, _) => (theme::with_alpha(theme::accent(), 0x30), theme::accent()),
        (false, true) => (theme::control_hover(), theme::text_strong()),
        (false, false) => (theme::control(), theme::text_muted()),
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(5), fill);
    painter.text(
        pos2(rect.left() + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        font,
        ink,
    );
    if caret {
        let c = pos2(rect.right() - 11.0, rect.center().y);
        painter.add(egui::Shape::convex_polygon(
            vec![
                c + vec2(-3.5, -1.5),
                c + vec2(3.5, -1.5),
                c + vec2(0.0, 2.5),
            ],
            ink,
            Stroke::NONE,
        ));
    }
    response
}
