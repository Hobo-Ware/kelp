use eframe::egui::{Align2, CursorIcon, FontId, Painter, Rect, Sense, Stroke, Ui, pos2, vec2};
use gix::ObjectId;
use kelp_core::commit::{self, Summary};
use serde::{Deserialize, Serialize};

use crate::{graph_view, menus, theme};

const MIN_W: f32 = 56.0;
const MAX_W: f32 = 420.0;
const PAD: f32 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Author,
    Date,
    Hash,
}

impl Column {
    const ALL: [Column; 3] = [Column::Author, Column::Date, Column::Hash];

    fn title(self) -> &'static str {
        match self {
            Column::Author => "AUTHOR",
            Column::Date => "DATE",
            Column::Hash => "HASH",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Column::Author => "Author",
            Column::Date => "Date",
            Column::Hash => "Hash",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphColumns {
    pub author: bool,
    pub date: bool,
    pub hash: bool,
    pub author_w: f32,
    pub date_w: f32,
    pub hash_w: f32,
}

impl Default for GraphColumns {
    fn default() -> Self {
        Self {
            author: false,
            date: false,
            hash: false,
            author_w: 140.0,
            date_w: 110.0,
            hash_w: 80.0,
        }
    }
}

impl GraphColumns {
    pub fn from_names(names: &str) -> Self {
        let mut columns = Self::default();
        for name in names.split(',').map(str::trim) {
            match name {
                "author" => columns.author = true,
                "date" => columns.date = true,
                "hash" => columns.hash = true,
                _ => {}
            }
        }
        columns
    }

    pub fn shows(&self, column: Column) -> bool {
        match column {
            Column::Author => self.author,
            Column::Date => self.date,
            Column::Hash => self.hash,
        }
    }

    pub fn toggle(&mut self, column: Column) {
        let shown = self.shown_mut(column);
        *shown = !*shown;
    }

    fn shown_mut(&mut self, column: Column) -> &mut bool {
        match column {
            Column::Author => &mut self.author,
            Column::Date => &mut self.date,
            Column::Hash => &mut self.hash,
        }
    }

    fn width(&self, column: Column) -> f32 {
        match column {
            Column::Author => self.author_w,
            Column::Date => self.date_w,
            Column::Hash => self.hash_w,
        }
    }

    fn width_mut(&mut self, column: Column) -> &mut f32 {
        match column {
            Column::Author => &mut self.author_w,
            Column::Date => &mut self.date_w,
            Column::Hash => &mut self.hash_w,
        }
    }

    pub fn total(&self) -> f32 {
        Column::ALL
            .iter()
            .filter(|c| self.shows(**c))
            .map(|c| self.width(*c))
            .sum()
    }

    fn spans(&self, right: f32) -> Vec<(Column, f32, f32)> {
        let mut x = right - self.total();
        Column::ALL
            .into_iter()
            .filter(|c| self.shows(*c))
            .map(|c| {
                let start = x;
                x += self.width(c);
                (c, start, x)
            })
            .collect()
    }
}

pub fn message_right(right: f32, columns: &GraphColumns) -> f32 {
    right - columns.total()
}

pub fn paint_cells(
    painter: &Painter,
    columns: &GraphColumns,
    right: f32,
    mid_y: f32,
    id: ObjectId,
    summary: &Summary,
    now: i64,
) {
    for (column, start, end) in columns.spans(right) {
        let (text, font, color) = match column {
            Column::Author => (
                summary.author.clone(),
                FontId::proportional(12.0),
                theme::text_muted(),
            ),
            Column::Date => (
                commit::relative_time(summary.time, now),
                FontId::proportional(11.5),
                theme::text_faint(),
            ),
            Column::Hash => (
                id.to_hex_with_len(7).to_string(),
                FontId::monospace(11.5),
                theme::text_faint(),
            ),
        };
        let max_width = (end - start - PAD * 2.0).max(0.0);
        let galley = graph_view::truncated(painter, text, font, color, max_width);
        painter.galley(
            pos2(start + PAD, mid_y - galley.size().y / 2.0),
            galley,
            color,
        );
    }
}

pub fn header(ui: &mut Ui, rect: Rect, columns: &mut GraphColumns) -> bool {
    let painter = ui.painter_at(rect);
    let mut changed = false;
    let font = FontId::monospace(10.0);
    for (column, start, _) in columns.spans(rect.right()) {
        painter.text(
            pos2(start + PAD, rect.center().y),
            Align2::LEFT_CENTER,
            column.title(),
            font.clone(),
            theme::text_faint(),
        );
        let handle = Rect::from_center_size(pos2(start, rect.center().y), vec2(9.0, rect.height()));
        let response = ui.interact(
            handle,
            ui.id().with(("column-edge", column.title())),
            Sense::drag(),
        );
        let hot = response.hovered() || response.dragged();
        if hot {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
        painter.vline(
            start,
            rect.top() + 6.0..=rect.bottom() - 6.0,
            Stroke::new(
                if hot { 2.0 } else { 1.0 },
                if hot {
                    theme::accent()
                } else {
                    theme::border()
                },
            ),
        );
        if response.dragged() {
            let width = columns.width_mut(column);
            *width = (*width - response.drag_delta().x).clamp(MIN_W, MAX_W);
            changed = true;
        }
    }
    let area = ui.interact(rect, ui.id().with("column-header"), Sense::click());
    crate::menus::context_menu(&area, |ui| changed |= menu(ui, columns));
    changed
}

pub fn menu(ui: &mut Ui, columns: &mut GraphColumns) -> bool {
    crate::menus::menu_width(ui, 180.0);
    ui.spacing_mut().item_spacing.y = 0.0;
    menus::heading(ui, "COLUMNS");
    let mut changed = false;
    for column in Column::ALL {
        let shown = columns.shows(column);
        let icon = shown.then_some(crate::icons::Icon::Check);
        if menus::row(ui, icon, column.label(), None, false) {
            *columns.shown_mut(column) = !shown;
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use eframe::egui::{self, Event, PointerButton, Pos2, RawInput, pos2, vec2};

    use super::*;

    #[test]
    fn spans_end_at_the_right_edge() {
        let columns = GraphColumns::from_names("author,hash");
        let spans = columns.spans(1000.0);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0], (Column::Author, 780.0, 920.0));
        assert_eq!(spans[1], (Column::Hash, 920.0, 1000.0));
        assert_eq!(message_right(1000.0, &columns), 780.0);
        assert_eq!(message_right(1000.0, &GraphColumns::default()), 1000.0);
    }

    #[test]
    fn settings_round_trip_keeps_columns() {
        let mut columns = GraphColumns::from_names("date");
        columns.date_w = 150.0;
        let json = serde_json::to_string(&columns).unwrap();
        assert_eq!(
            serde_json::from_str::<GraphColumns>(&json).unwrap(),
            columns
        );
        assert_eq!(
            serde_json::from_str::<GraphColumns>("{}").unwrap(),
            GraphColumns::default()
        );
    }

    #[test]
    fn clicking_a_menu_row_toggles_that_column() {
        let ctx = egui::Context::default();
        let mut columns = GraphColumns::default();
        let run = |events: Vec<Event>, columns: &mut GraphColumns| {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
                events,
                ..Default::default()
            };
            let mut changed = false;
            let _ = ctx.run_ui(input, |ui| changed |= menu(ui, columns));
            changed
        };
        let author_row = pos2(40.0, 26.0 + 15.0);
        let press = |pressed| Event::PointerButton {
            pos: author_row,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        run(vec![Event::PointerMoved(author_row)], &mut columns);
        run(vec![press(true)], &mut columns);
        assert!(run(vec![press(false)], &mut columns));
        assert!(columns.author && !columns.date && !columns.hash);
    }
}
