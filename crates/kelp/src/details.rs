use eframe::egui::{self, Align2, FontId, RichText, Sense, Stroke, Ui, pos2, vec2};
use kelp_core::avatar;
use kelp_core::commit::{self, ChangeKind};

use crate::app::Repo;
use crate::theme;

pub fn ui(ui: &mut Ui, repo: &mut Repo) {
    let Some(details) = repo.details.clone() else {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new("Select a commit").color(theme::TEXT_MUTED))
        });
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let lane = repo
        .history
        .row(&details.id)
        .map(|r| theme::lane(repo.history.layout.node_color(r)))
        .unwrap_or(theme::ACCENT);

    let mut reveal = None;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 10.0;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("commit").color(theme::TEXT_MUTED));
                        let short = details.id.to_hex_with_len(7).to_string();
                        if ui
                            .add(
                                egui::Label::new(
                                    RichText::new(&short).monospace().color(theme::TEXT_STRONG),
                                )
                                .sense(Sense::click()),
                            )
                            .on_hover_text("Copy full hash")
                            .clicked()
                        {
                            ui.ctx().copy_text(details.id.to_string());
                        }
                    });
                    ui.label(
                        RichText::new(&details.title)
                            .size(17.0)
                            .strong()
                            .color(theme::TEXT_STRONG),
                    );
                    if !details.body.is_empty() {
                        ui.label(
                            RichText::new(&details.body)
                                .color(egui::Color32::from_rgb(0xb4, 0xb9, 0xc2)),
                        );
                    }
                    ui.separator();

                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(vec2(38.0, 38.0), Sense::hover());
                        let fill = theme::AVATARS
                            [avatar::color_index(&details.email, theme::AVATARS.len())];
                        ui.painter()
                            .circle(rect.center(), 18.0, fill, Stroke::new(2.0, lane));
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            avatar::initials(&details.author),
                            FontId::proportional(13.0),
                            theme::AVATAR_INK,
                        );
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.label(RichText::new(&details.author).color(theme::TEXT_STRONG));
                            ui.label(
                                RichText::new(format!(
                                    "authored {}",
                                    commit::relative_time(details.time, now)
                                ))
                                .size(12.0)
                                .color(theme::TEXT_MUTED),
                            );
                            if details.committer != details.author {
                                ui.label(
                                    RichText::new(format!("committed by {}", details.committer))
                                        .size(12.0)
                                        .color(theme::TEXT_MUTED),
                                );
                            }
                        });
                    });
                    if !details.parents.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(if details.parents.len() > 1 {
                                    "parents"
                                } else {
                                    "parent"
                                })
                                .size(12.0)
                                .color(theme::TEXT_MUTED),
                            );
                            for parent in &details.parents {
                                let short = parent.to_hex_with_len(7).to_string();
                                if ui
                                    .link(
                                        RichText::new(short)
                                            .monospace()
                                            .size(12.0)
                                            .color(theme::ACCENT),
                                    )
                                    .clicked()
                                {
                                    reveal = repo.history.row(parent);
                                }
                            }
                        });
                    }
                    ui.separator();

                    let count =
                        |k: ChangeKind| details.changes.iter().filter(|c| c.kind == k).count();
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 14.0;
                        ui.label(
                            RichText::new(format!(
                                "{} modified",
                                count(ChangeKind::Modified) + count(ChangeKind::Renamed)
                            ))
                            .size(12.0)
                            .color(theme::MODIFIED),
                        );
                        ui.label(
                            RichText::new(format!("{} added", count(ChangeKind::Added)))
                                .size(12.0)
                                .color(theme::ADDED),
                        );
                        ui.label(
                            RichText::new(format!("{} deleted", count(ChangeKind::Deleted)))
                                .size(12.0)
                                .color(theme::DELETED),
                        );
                    });
                });
            ui.spacing_mut().item_spacing.y = 0.0;
            for change in &details.changes {
                file_row(ui, change.kind, &change.path);
            }
        });
    if let Some(row) = reveal {
        repo.reveal(row);
    }
}

fn file_row(ui: &mut Ui, kind: ChangeKind, path: &str) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
    let painter = ui.painter_at(rect);
    if response.hovered() {
        painter.rect_filled(rect, 0.0, theme::with_alpha(egui::Color32::WHITE, 0x08));
    }
    let (mark, color) = match kind {
        ChangeKind::Added => ("A", theme::ADDED),
        ChangeKind::Deleted => ("D", theme::DELETED),
        ChangeKind::Modified => ("M", theme::MODIFIED),
        ChangeKind::Renamed => ("R", theme::MODIFIED),
    };
    let y = rect.center().y;
    let font = FontId::monospace(12.0);
    painter.text(
        pos2(rect.left() + 20.0, y),
        Align2::CENTER_CENTER,
        mark,
        font.clone(),
        color,
    );
    let (dir, name) = path.rsplit_once('/').map_or(("", path), |(d, n)| (d, n));
    let left = rect.left() + 36.0;
    let name_galley = painter.layout_no_wrap(name.to_string(), font.clone(), theme::TEXT);
    let name_w = name_galley.size().x;
    let dir_space = (rect.right() - 12.0 - left - name_w).max(0.0);
    let mut x = left;
    if !dir.is_empty() {
        let dir_galley = painter.layout_no_wrap(
            elide_end(&painter, &format!("{dir}/"), &font, dir_space),
            font.clone(),
            theme::TEXT_FAINT,
        );
        painter.galley(
            pos2(x, y - dir_galley.size().y / 2.0),
            dir_galley.clone(),
            theme::TEXT_FAINT,
        );
        x += dir_galley.size().x;
    }
    painter.galley(
        pos2(x, y - name_galley.size().y / 2.0),
        name_galley,
        theme::TEXT,
    );
    response.on_hover_text(path);
}

fn elide_end(painter: &egui::Painter, text: &str, font: &FontId, max_width: f32) -> String {
    let width = |t: &str| {
        painter
            .layout_no_wrap(t.to_string(), font.clone(), theme::TEXT)
            .size()
            .x
    };
    if width(text) <= max_width {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect::<String>() + "…/";
        if width(&candidate) <= max_width {
            lo = mid
        } else {
            hi = mid - 1
        }
    }
    if lo == 0 {
        String::new()
    } else {
        chars[..lo].iter().collect::<String>() + "…/"
    }
}
