use eframe::egui::{self, Color32, RichText, Sense, Ui, vec2};
use kelp_core::refs::RefKind;

use crate::app::{Repo, Selection};
use crate::{graph_view, theme};

pub fn ui(ui: &mut Ui, repo: &mut Repo) {
    let mut reveal = None;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(8.0);
            for (title, kind) in [
                ("LOCAL", RefKind::Local),
                ("REMOTE", RefKind::Remote),
                ("TAGS", RefKind::Tag),
            ] {
                let labels: Vec<_> = repo.history.refs.of_kind(kind).collect();
                let id = ui.make_persistent_id(title);
                let open_by_default = kind != RefKind::Tag;
                let header = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    id,
                    open_by_default,
                );
                header
                    .show_header(ui, |ui| {
                        ui.label(
                            RichText::new(title)
                                .size(11.0)
                                .strong()
                                .color(theme::TEXT_MUTED),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(12.0);
                            ui.label(
                                RichText::new(labels.len().to_string())
                                    .size(11.0)
                                    .color(theme::TEXT_MUTED),
                            );
                        });
                    })
                    .body(|ui| {
                        for label in labels {
                            let selected = label.row.is_some()
                                && label.row.map(|r| Selection::Commit(r as usize))
                                    == repo.selected;
                            let dot = label
                                .row
                                .map(|r| theme::lane(repo.history.layout.node_color(r as usize)))
                                .unwrap_or(theme::TEXT_FAINT);
                            if branch_row(ui, &label.name, dot, label.is_head, selected).clicked()
                                && let Some(row) = label.row
                            {
                                reveal = Some(row as usize);
                            }
                        }
                    });
                ui.add_space(6.0);
            }
        });
    if let Some(row) = reveal {
        repo.reveal(Selection::Commit(row));
    }
}

fn branch_row(
    ui: &mut Ui,
    name: &str,
    dot: Color32,
    is_head: bool,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 0.0, theme::SIDEBAR_SELECTED);
    } else if response.hovered() {
        painter.rect_filled(rect, 0.0, theme::with_alpha(Color32::WHITE, 0x08));
    }
    let y = rect.center().y;
    painter.circle_filled(egui::pos2(rect.left() + 12.0, y), 4.0, dot);
    let color = if selected || is_head {
        theme::TEXT_STRONG
    } else {
        Color32::from_rgb(0xd5, 0xd7, 0xdc)
    };
    let font = if is_head {
        egui::FontId::proportional(13.5)
    } else {
        egui::FontId::proportional(13.0)
    };
    let reserved = if is_head { 56.0 } else { 12.0 };
    let max_width = (rect.width() - 24.0 - reserved).max(0.0);
    let galley = graph_view::truncated(&painter, name.to_string(), font, color, max_width);
    painter.galley(
        egui::pos2(rect.left() + 24.0, y - galley.size().y / 2.0),
        galley,
        color,
    );
    if is_head {
        painter.text(
            egui::pos2(rect.right() - 12.0, y),
            egui::Align2::RIGHT_CENTER,
            "HEAD",
            egui::FontId::proportional(11.0),
            dot,
        );
    }
    response
}
