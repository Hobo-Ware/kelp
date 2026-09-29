use eframe::egui::{self, Color32, FontId, Painter, Pos2, Rect, Sense, Ui, pos2, vec2};
use kelp_core::commit::{ChangeKind, FileChange};

use crate::icons::{self, Icon};
use crate::theme;

const ICON: f32 = 12.0;
const ICON_GAP: f32 = 3.0;
const PAIR_GAP: f32 = 10.0;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Counts {
    pub modified: usize,
    pub added: usize,
    pub deleted: usize,
}

impl Counts {
    pub fn of(changes: &[FileChange]) -> Self {
        let count =
            |kinds: &[ChangeKind]| changes.iter().filter(|c| kinds.contains(&c.kind)).count();
        Self {
            modified: count(&[ChangeKind::Modified, ChangeKind::Renamed]),
            added: count(&[ChangeKind::Added]),
            deleted: count(&[ChangeKind::Deleted]),
        }
    }

    pub fn changed(total: usize) -> Self {
        Self {
            modified: total,
            ..Self::default()
        }
    }

    fn pairs(&self) -> impl Iterator<Item = (Icon, usize, Color32, &'static str)> {
        [
            (Icon::Pencil, self.modified, theme::modified(), "modified"),
            (Icon::Plus, self.added, theme::added(), "added"),
            (Icon::Minus, self.deleted, theme::deleted(), "deleted"),
        ]
        .into_iter()
        .filter(|(_, n, _, _)| *n > 0)
    }

    pub fn words(&self) -> String {
        self.pairs()
            .map(|(_, n, _, word)| format!("{n} {word}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub fn width(painter: &Painter, counts: Counts, size: f32) -> f32 {
    let font = FontId::proportional(size);
    let widths: Vec<f32> = counts
        .pairs()
        .map(|(_, n, color, _)| {
            ICON + ICON_GAP
                + painter
                    .layout_no_wrap(n.to_string(), font.clone(), color)
                    .size()
                    .x
        })
        .collect();
    widths.iter().sum::<f32>() + PAIR_GAP * widths.len().saturating_sub(1) as f32
}

pub fn paint(painter: &Painter, left: f32, mid: f32, counts: Counts, size: f32) {
    let font = FontId::proportional(size);
    let mut x = left;
    for (icon, n, color, _) in counts.pairs() {
        let spot = Rect::from_center_size(pos2(x + ICON / 2.0, mid), vec2(ICON, ICON));
        icons::paint(painter, spot, icon, color);
        x += ICON + ICON_GAP;
        let galley = painter.layout_no_wrap(n.to_string(), font.clone(), color);
        let width = galley.size().x;
        painter.galley(Pos2::new(x, mid - galley.size().y / 2.0), galley, color);
        x += width + PAIR_GAP;
    }
}

pub fn show(ui: &mut Ui, counts: Counts) -> egui::Response {
    let size = 12.0;
    let width = width(ui.painter(), counts, size);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 18.0), Sense::hover());
    paint(ui.painter(), rect.left(), rect.center().y, counts, size);
    let words = counts.words();
    crate::widgets::describe(&response, egui::WidgetType::Label, words.clone());
    response.on_hover_text(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(path: &str, kind: ChangeKind) -> FileChange {
        FileChange {
            path: path.into(),
            kind,
        }
    }

    #[test]
    fn renames_count_as_modified_and_words_skip_zeroes() {
        let counts = Counts::of(&[
            change("a", ChangeKind::Modified),
            change("b", ChangeKind::Renamed),
            change("c", ChangeKind::Added),
        ]);
        assert_eq!(
            counts,
            Counts {
                modified: 2,
                added: 1,
                deleted: 0
            }
        );
        assert_eq!(counts.words(), "2 modified, 1 added");
    }
}
