use std::path::Path;

use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Stroke, Ui, pos2, vec2};
use gix::ObjectId;
use kelp_core::diff::{self, Body, FileDiff, Line, LineKind};

use crate::theme;

const LINE_H: f32 = 22.0;
const NUM_W: f32 = 48.0;
const HEADER_H: f32 = 48.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSource {
    Commit(ObjectId),
    Working,
}

enum Row {
    Hunk(String),
    Line(Line),
}

pub struct DiffView {
    pub source: DiffSource,
    pub diff: FileDiff,
    rows: Vec<Row>,
}

impl DiffView {
    pub fn load(
        repo: &gix::Repository,
        workdir: Option<&Path>,
        source: DiffSource,
        path: &str,
    ) -> anyhow::Result<Self> {
        let diff = match (source, workdir) {
            (DiffSource::Commit(id), _) => diff::commit_file(repo, id, path)?,
            (DiffSource::Working, Some(workdir)) => diff::working_file(repo, workdir, path)?,
            (DiffSource::Working, None) => anyhow::bail!("this repository has no working tree"),
        };
        let rows = match &diff.body {
            Body::Text(hunks) => hunks
                .iter()
                .flat_map(|h| {
                    std::iter::once(Row::Hunk(h.header.clone()))
                        .chain(h.lines.iter().cloned().map(Row::Line))
                })
                .collect(),
            _ => Vec::new(),
        };
        Ok(Self { source, diff, rows })
    }

    pub fn path(&self) -> &str {
        &self.diff.path
    }

    pub fn ui(&mut self, ui: &mut Ui) -> bool {
        let close = self.header(ui);
        match &self.diff.body {
            Body::Binary => notice(ui, "Binary file, no text diff to show."),
            Body::TooLarge => notice(ui, "This file is too large to diff here."),
            Body::Text(_) if self.rows.is_empty() => notice(ui, "No changes in this file."),
            Body::Text(_) => self.lines(ui),
        }
        close
    }

    fn header(&self, ui: &mut Ui) -> bool {
        let mut close = false;
        egui::Frame::new()
            .fill(theme::PANEL)
            .inner_margin(egui::Margin::symmetric(16, 0))
            .show(ui, |ui| {
                ui.set_height(HEADER_H);
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    if ui
                        .add(
                            egui::Button::new(RichText::new("‹  Graph").size(12.0))
                                .corner_radius(5),
                        )
                        .on_hover_text("Back to the graph (Esc)")
                        .clicked()
                    {
                        close = true;
                    }
                    let (dir, name) = self
                        .path()
                        .rsplit_once('/')
                        .map_or(("", self.path()), |(d, n)| (d, n));
                    if !dir.is_empty() {
                        ui.label(
                            RichText::new(format!("{dir}/"))
                                .monospace()
                                .color(theme::TEXT_FAINT),
                        );
                        ui.add_space(-12.0);
                    }
                    ui.label(
                        RichText::new(name)
                            .monospace()
                            .strong()
                            .color(theme::TEXT_STRONG),
                    );
                    ui.label(
                        RichText::new(format!("+{}", self.diff.added))
                            .size(12.0)
                            .color(theme::ADDED),
                    );
                    ui.label(
                        RichText::new(format!("−{}", self.diff.removed))
                            .size(12.0)
                            .color(theme::DELETED),
                    );
                    let origin = match self.source {
                        DiffSource::Commit(id) => format!("in {}", id.to_hex_with_len(7)),
                        DiffSource::Working => "uncommitted".into(),
                    };
                    ui.label(RichText::new(origin).size(12.0).color(theme::TEXT_FAINT));
                });
            });
        let rect = ui.min_rect();
        ui.painter().hline(
            rect.x_range(),
            rect.bottom(),
            Stroke::new(1.0, theme::BORDER),
        );
        close
    }

    fn lines(&self, ui: &mut Ui) {
        let font = FontId::monospace(12.5);
        egui::ScrollArea::both().auto_shrink(false).show_rows(
            ui,
            LINE_H,
            self.rows.len(),
            |ui, range| {
                let width = ui.available_width().max(1200.0);
                let (rect, _) = ui
                    .allocate_exact_size(vec2(width, LINE_H * range.len() as f32), Sense::hover());
                let painter = ui.painter_at(rect);
                for (i, row) in self.rows[range].iter().enumerate() {
                    let top = rect.top() + i as f32 * LINE_H;
                    let line_rect = egui::Rect::from_min_size(
                        pos2(rect.left(), top),
                        vec2(rect.width(), LINE_H),
                    );
                    let mid = line_rect.center().y;
                    match row {
                        Row::Hunk(header) => {
                            painter.rect_filled(
                                line_rect,
                                0.0,
                                Color32::from_rgb(0x1a, 0x22, 0x30),
                            );
                            painter.text(
                                pos2(rect.left() + 16.0, mid),
                                Align2::LEFT_CENTER,
                                header,
                                font.clone(),
                                Color32::from_rgb(0x8f, 0xb4, 0xe8),
                            );
                        }
                        Row::Line(line) => {
                            let (bg, mark, mark_color) = match line.kind {
                                LineKind::Added => {
                                    (Color32::from_rgb(0x16, 0x30, 0x2a), "+", theme::ADDED)
                                }
                                LineKind::Removed => {
                                    (Color32::from_rgb(0x3a, 0x1f, 0x22), "-", theme::DELETED)
                                }
                                LineKind::Context => (Color32::TRANSPARENT, "", theme::TEXT_FAINT),
                            };
                            painter.rect_filled(line_rect, 0.0, bg);
                            let num = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
                            let nums = Color32::from_rgb(0x5e, 0x65, 0x73);
                            painter.text(
                                pos2(rect.left() + NUM_W - 8.0, mid),
                                Align2::RIGHT_CENTER,
                                num(line.old),
                                font.clone(),
                                nums,
                            );
                            painter.text(
                                pos2(rect.left() + NUM_W * 2.0 - 8.0, mid),
                                Align2::RIGHT_CENTER,
                                num(line.new),
                                font.clone(),
                                nums,
                            );
                            painter.text(
                                pos2(rect.left() + NUM_W * 2.0 + 10.0, mid),
                                Align2::LEFT_CENTER,
                                mark,
                                font.clone(),
                                mark_color,
                            );
                            let text_color = if line.kind == LineKind::Context {
                                Color32::from_rgb(0xb4, 0xb9, 0xc2)
                            } else {
                                theme::TEXT
                            };
                            painter.text(
                                pos2(rect.left() + NUM_W * 2.0 + 28.0, mid),
                                Align2::LEFT_CENTER,
                                &line.text,
                                font.clone(),
                                text_color,
                            );
                        }
                    }
                }
            },
        );
    }
}

fn notice(ui: &mut Ui, text: &str) {
    ui.centered_and_justified(|ui| ui.label(RichText::new(text).color(theme::TEXT_MUTED)));
}
