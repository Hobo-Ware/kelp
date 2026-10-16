use egui::{
    Align, Align2, FontId, Key, Layout, Margin, Modifiers, Rect, RichText, TextEdit, Ui, UiBuilder,
    pos2, vec2,
};
use kelp_core::review::{Anchor, Comment, Side};

use super::note::{self, ButtonKind};
use super::{DiffAction, DiffBody, DiffInput, THREAD_INDENT, now};
use crate::theme;

const PAD: f32 = 12.0;
const AVATAR: f32 = 22.0;
const GAP: f32 = 10.0;
const FIELD_PAD: f32 = 10.0;
const MIN_ROWS: f32 = 3.0;
const FONT_SIZE: f32 = 13.0;
const HINT_GAP: f32 = 12.0;
const PLACEHOLDER: &str = "Leave a comment";

fn font() -> FontId {
    FontId::proportional(FONT_SIZE)
}

fn field_wrap_width(card_w: f32) -> f32 {
    card_w - 2.0 * (PAD + FIELD_PAD + 1.0)
}

fn row_height(ui: &Ui) -> f32 {
    ui.painter()
        .layout_no_wrap("Ag".into(), font(), theme::text())
        .size()
        .y
}

/// The height of the composer row: the card, its margin, and a text field that is three rows
/// tall and grows with `draft`.
pub(super) fn height(ui: &Ui, draft: &str, card_w: f32) -> f32 {
    let text = ui
        .painter()
        .layout(
            draft.to_string(),
            font(),
            theme::text(),
            field_wrap_width(card_w),
        )
        .size()
        .y;
    card_height(text.max(MIN_ROWS * row_height(ui)))
}

fn card_height(text_h: f32) -> f32 {
    note::CARD_MARGIN_Y * 2.0
        + PAD
        + AVATAR
        + GAP
        + (FIELD_PAD + 1.0) * 2.0
        + text_h
        + GAP
        + note::BUTTON_H
        + PAD
}

pub(super) fn heading(first: u32, last: u32) -> String {
    if last > first {
        format!("Comment on lines {first}-{last}")
    } else {
        format!("Comment on line {first}")
    }
}

pub(super) fn shortcut_hint(mac: bool) -> String {
    format!(
        "{} to comment · Esc to cancel",
        crate::widgets::chord_label(mac, "⌘ Enter")
    )
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

impl DiffBody {
    pub(super) fn composer_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        card_w: f32,
        input: &DiffInput<'_>,
        actions: &mut Vec<DiffAction>,
    ) {
        let Some((side, line_no)) = self.composer else {
            return;
        };
        let accent = input.style.accent();
        let card = Rect::from_min_size(
            pos2(
                rect.left() + THREAD_INDENT,
                rect.top() + note::CARD_MARGIN_Y,
            ),
            vec2(card_w, rect.height() - note::CARD_MARGIN_Y * 2.0),
        );
        note::paint_card(ui, card, &input.style);

        let head = pos2(
            card.left() + PAD + AVATAR / 2.0,
            card.top() + PAD + AVATAR / 2.0,
        );
        note::avatar(ui, head, input.author, accent);
        let title = ui.painter_at(card).text(
            pos2(head.x + AVATAR / 2.0 + 8.0, head.y),
            Align2::LEFT_CENTER,
            heading(line_no, line_no),
            FontId::proportional(13.0),
            theme::text_control(),
        );
        ui.painter_at(card).text(
            pos2(title.right() + 8.0, head.y),
            Align2::LEFT_CENTER,
            file_name(&input.diff.path),
            FontId::proportional(12.0),
            theme::text_faint(),
        );

        let footer = Rect::from_min_max(
            pos2(card.left() + PAD, card.bottom() - PAD - note::BUTTON_H),
            pos2(card.right() - PAD, card.bottom() - PAD),
        );
        let field = Rect::from_min_max(
            pos2(card.left() + PAD, head.y + AVATAR / 2.0 + GAP),
            pos2(card.right() - PAD, footer.top() - GAP),
        );
        let slot = note::field_slot(ui);
        let id = ui.id().with(("composer-field", side as u8, line_no));
        let (save_key, cancel_key) = if self.composer_focused {
            ui.input_mut(|i| {
                (
                    i.consume_key(Modifiers::COMMAND, Key::Enter),
                    i.consume_key(Modifiers::NONE, Key::Escape),
                )
            })
        } else {
            (false, false)
        };
        let mut area = ui.new_child(
            UiBuilder::new()
                .max_rect(field.shrink(FIELD_PAD + 1.0))
                .layout(Layout::top_down(Align::Min)),
        );
        input.style.paint_accent(&mut area);
        let placeholder = if input.style.note_card {
            PLACEHOLDER
        } else {
            input.texts.comment_hint.as_ref()
        };
        let edit = area.add(
            TextEdit::multiline(&mut self.draft)
                .id(id)
                .frame(egui::Frame::NONE)
                .margin(Margin::ZERO)
                .font(font())
                .hint_text(RichText::new(placeholder).color(theme::text_faint()))
                .desired_rows(MIN_ROWS as usize)
                .desired_width(f32::INFINITY),
        );
        if !edit.has_focus() && ui.memory(|m| m.focused().is_none()) {
            edit.request_focus();
        }
        if edit.changed() || edit.has_focus() != self.composer_focused {
            ui.ctx().request_repaint();
        }
        self.composer_focused = edit.has_focus();
        note::paint_field(ui, slot, field, edit.has_focus(), &input.style);

        let enabled = !self.draft.trim().is_empty();
        let mut close = cancel_key;
        let mut save = save_key && enabled;
        let mut buttons = ui.new_child(
            UiBuilder::new()
                .max_rect(footer)
                .layout(Layout::right_to_left(Align::Center)),
        );
        buttons.spacing_mut().item_spacing.x = 8.0;
        if note::button(
            &mut buttons,
            "Comment",
            ButtonKind::Primary,
            enabled,
            &input.style,
        )
        .clicked()
        {
            save = true;
        }
        if note::button(
            &mut buttons,
            "Cancel",
            ButtonKind::Quiet,
            true,
            &input.style,
        )
        .clicked()
        {
            close = true;
        }
        let used = buttons.min_rect().width();
        let hint = shortcut_hint(cfg!(target_os = "macos"));
        let hint_galley =
            ui.painter()
                .layout_no_wrap(hint, FontId::proportional(11.0), theme::text_faint());
        if footer.width() - used >= hint_galley.size().x + HINT_GAP {
            ui.painter().galley(
                pos2(
                    footer.left(),
                    footer.center().y - hint_galley.size().y / 2.0,
                ),
                hint_galley,
                theme::text_faint(),
            );
        }

        if save {
            let lines: Vec<&str> = if side == Side::Old {
                &self.data.old_lines
            } else {
                &self.data.new_lines
            }
            .iter()
            .map(String::as_str)
            .collect();
            let index = (line_no as usize)
                .saturating_sub(1)
                .min(lines.len().saturating_sub(1));
            if !lines.is_empty() {
                actions.push(DiffAction::AddThread {
                    anchor: Anchor::at(&lines, index, side),
                    comment: Comment {
                        author: input.author.to_string(),
                        time: now(),
                        body: self.draft.trim().to_string(),
                    },
                });
            }
            close = true;
        }
        if close {
            self.composer = None;
            self.composer_focused = false;
            self.draft.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_heading_names_a_line_or_a_range() {
        assert_eq!(heading(6, 6), "Comment on line 6");
        assert_eq!(heading(6, 12), "Comment on lines 6-12");
    }

    #[test]
    fn the_shortcut_hint_reads_ctrl_off_macos() {
        assert_eq!(shortcut_hint(true), "⌘ Enter to comment · Esc to cancel");
        assert_eq!(
            shortcut_hint(false),
            "Ctrl+Enter to comment · Esc to cancel"
        );
    }

    #[test]
    fn the_file_name_is_the_last_path_part() {
        assert_eq!(file_name("src/graph/view.rs"), "view.rs");
        assert_eq!(file_name("f.txt"), "f.txt");
    }

    #[test]
    fn the_card_has_no_space_under_the_footer() {
        assert_eq!(
            card_height(48.0),
            6.0 * 2.0 + 12.0 + 22.0 + 10.0 + 22.0 + 48.0 + 10.0 + 28.0 + 12.0
        );
    }
}
