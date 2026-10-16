use egui::{
    Align, Align2, Color32, CursorIcon, FontId, Layout, Margin, Rect, Response, Sense, Stroke,
    StrokeKind, TextEdit, Ui, UiBuilder, pos2, vec2,
};
use kelp_core::review::{Comment, Thread};

use super::{DiffAction, DiffBody, DiffInput, DiffStyle, now};
use crate::theme;

const PAD: f32 = 12.0;
const AVATAR: f32 = 22.0;
const TEXT_X: f32 = PAD + AVATAR + 8.0;
const BODY_DY: f32 = 28.0;
const COMMENT_GAP: f32 = 12.0;
const FIELD_H: f32 = 32.0;
const FIELD_PAD_Y: f32 = 6.0;
const ROW_GAP: f32 = 8.0;
const BUTTON_H: f32 = 22.0;
const CARD_MARGIN_Y: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ButtonKind {
    Primary,
    Quiet,
    Danger,
}

/// The kinds of the Reply, Resolve and Delete buttons, in that order. Only Reply is ever filled,
/// and only when there is text to send.
pub(super) fn button_kinds(has_text: bool) -> [ButtonKind; 3] {
    [
        if has_text {
            ButtonKind::Primary
        } else {
            ButtonKind::Quiet
        },
        ButtonKind::Quiet,
        ButtonKind::Danger,
    ]
}

pub(super) fn body_width(card_w: f32) -> f32 {
    card_w - TEXT_X - PAD
}

/// The height of a thread row whose comment bodies are `body_heights` tall: the card fits its
/// content, with a small margin above and below.
pub(super) fn thread_height(body_heights: impl IntoIterator<Item = f32>) -> f32 {
    let comments: f32 = body_heights
        .into_iter()
        .map(|h| BODY_DY + h + COMMENT_GAP)
        .sum();
    PAD + comments + FIELD_H + ROW_GAP + BUTTON_H + PAD + CARD_MARGIN_Y * 2.0
}

pub(super) fn measured_height(ui: &Ui, thread: &Thread, card_w: f32) -> f32 {
    thread_height(thread.comments.iter().map(|c| {
        ui.painter()
            .layout(
                c.body.clone(),
                FontId::proportional(13.0),
                theme::text(),
                body_width(card_w),
            )
            .size()
            .y
    }))
}

fn button(
    ui: &mut Ui,
    label: &str,
    kind: ButtonKind,
    enabled: bool,
    style: &DiffStyle,
) -> Response {
    let font = if kind == ButtonKind::Primary {
        FontId::new(12.0, theme::semibold())
    } else {
        FontId::proportional(12.0)
    };
    let (_, line) = style.field();
    let (ink, fill, border) = match kind {
        ButtonKind::Primary => (theme::on_accent(), style.accent(), style.accent()),
        ButtonKind::Quiet => (theme::text(), Color32::TRANSPARENT, line),
        ButtonKind::Danger => (
            theme::deleted(),
            Color32::TRANSPARENT,
            theme::with_alpha(theme::deleted(), 0x66),
        ),
    };
    let ink = if enabled {
        ink
    } else {
        theme::with_alpha(ink, 0x66)
    };
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, ink);
    let size = vec2(galley.size().x + 20.0, BUTTON_H);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hovered = enabled && response.hovered();
    let hover_fill = match kind {
        ButtonKind::Primary => theme::with_alpha(theme::text_strong(), 0x1c),
        ButtonKind::Quiet => theme::overlay(0x0c),
        ButtonKind::Danger => theme::with_alpha(theme::deleted(), 0x1a),
    };
    let painter = ui.painter();
    painter.rect(
        rect,
        6.0,
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    if hovered {
        painter.rect_filled(rect.shrink(1.0), 5.0, hover_fill);
    }
    painter.galley(rect.center() - galley.size() / 2.0, galley, ink);
    crate::widgets::focus_ring(ui, &response, 6.0);
    crate::widgets::describe(&response, egui::WidgetType::Button, label);
    if hovered {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

impl DiffBody {
    pub(super) fn embedded_thread(
        &mut self,
        ui: &mut Ui,
        card: Rect,
        thread: &Thread,
        input: &DiffInput<'_>,
        actions: &mut Vec<DiffAction>,
    ) {
        let id = thread.id;
        let accent = input.style.accent();
        let mut y = card.top() + PAD;
        for comment in &thread.comments {
            let center = pos2(card.left() + PAD + AVATAR / 2.0, y + AVATAR / 2.0);
            ui.painter()
                .circle_filled(center, AVATAR / 2.0, theme::with_alpha(accent, 0x26));
            ui.painter().text(
                center,
                Align2::CENTER_CENTER,
                kelp_core::avatar::initials(&comment.author),
                FontId::new(10.0, theme::semibold()),
                accent,
            );
            let header = ui.painter().text(
                pos2(card.left() + TEXT_X, center.y),
                Align2::LEFT_CENTER,
                &comment.author,
                FontId::proportional(13.0),
                theme::text_strong(),
            );
            ui.painter().text(
                pos2(header.right() + 8.0, center.y),
                Align2::LEFT_CENTER,
                kelp_core::commit::relative_time(comment.time, now()),
                FontId::proportional(12.0),
                theme::text_faint(),
            );
            let body = ui.painter().layout(
                comment.body.clone(),
                FontId::proportional(13.0),
                theme::text_control(),
                body_width(card.width()),
            );
            let body_h = body.size().y;
            ui.painter().galley(
                pos2(card.left() + TEXT_X, y + BODY_DY),
                body,
                theme::text_control(),
            );
            y += BODY_DY + body_h + COMMENT_GAP;
        }
        let area = Rect::from_min_max(
            pos2(card.left() + PAD, y),
            pos2(card.right() - PAD, card.bottom() - PAD),
        );
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(area)
                .layout(Layout::top_down(Align::Min)),
        );
        child.spacing_mut().item_spacing = vec2(6.0, ROW_GAP);
        input.style.paint_accent(&mut child);
        let (field_fill, field_line) = input.style.field();
        let mut field = egui::Frame::new()
            .fill(field_fill)
            .corner_radius(8)
            .stroke(Stroke::new(1.0, field_line))
            .inner_margin(Margin::symmetric(10, FIELD_PAD_Y as i8))
            .begin(&mut child);
        let draft = self.replies.entry(id).or_default();
        let edit = field.content_ui.add(
            TextEdit::multiline(draft)
                .frame(egui::Frame::NONE)
                .hint_text("Reply")
                .desired_rows(1)
                .desired_width(f32::INFINITY)
                .min_size(vec2(0.0, FIELD_H - 2.0 * FIELD_PAD_Y - 2.0)),
        );
        let has_text = !draft.trim().is_empty();
        if edit.has_focus() {
            field.frame.stroke = Stroke::new(1.0, accent);
        }
        field.end(&mut child);
        let send_shortcut =
            child.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter)) && has_text;
        let [reply_kind, resolve_kind, delete_kind] = button_kinds(has_text);
        let hint = format!(
            "{} to send",
            crate::widgets::chord_label(cfg!(target_os = "macos"), "⌘ Enter")
        );
        let row = vec2(child.available_width(), BUTTON_H);
        child.allocate_ui_with_layout(row, Layout::left_to_right(Align::Center), |ui| {
            if (button(ui, "Reply", reply_kind, has_text, &input.style).clicked() || send_shortcut)
                && has_text
            {
                let body = self.replies.remove(&id).unwrap_or_default();
                actions.push(DiffAction::Reply {
                    thread: id,
                    comment: Comment {
                        author: input.author.to_string(),
                        time: now(),
                        body: body.trim().to_string(),
                    },
                });
            }
            let resolve_label = if thread.resolved { "Reopen" } else { "Resolve" };
            if button(ui, resolve_label, resolve_kind, true, &input.style).clicked() {
                actions.push(DiffAction::SetResolved {
                    thread: id,
                    resolved: !thread.resolved,
                });
                self.expanded.remove(&id);
            }
            if button(ui, "Delete", delete_kind, true, &input.style).clicked() {
                actions.push(DiffAction::DeleteThread(id));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(hint)
                        .size(11.0)
                        .color(theme::text_faint()),
                );
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_reply_is_filled_and_only_with_text() {
        assert_eq!(
            button_kinds(true),
            [ButtonKind::Primary, ButtonKind::Quiet, ButtonKind::Danger]
        );
        assert_eq!(
            button_kinds(false),
            [ButtonKind::Quiet, ButtonKind::Quiet, ButtonKind::Danger]
        );
    }

    #[test]
    fn a_thread_row_is_its_content_plus_padding() {
        let one = thread_height([32.0]);
        assert_eq!(
            one,
            12.0 + 28.0 + 32.0 + 12.0 + 32.0 + 8.0 + 22.0 + 12.0 + 12.0
        );
        assert_eq!(thread_height([32.0, 16.0]), one + 28.0 + 16.0 + 12.0);
        assert_eq!(body_width(640.0), 640.0 - 42.0 - 12.0);
    }
}
