use egui::layers::ShapeIdx;
use egui::{
    Align, Align2, Color32, CursorIcon, FontId, Layout, Margin, Rect, Response, Sense, Shape,
    Stroke, StrokeKind, TextEdit, Ui, UiBuilder, epaint::RectShape, pos2, vec2,
};
use kelp_core::review::{Comment, Thread};

use super::range::anchor_label;
use super::{DiffAction, DiffBody, DiffInput, DiffStyle, now};
use crate::theme;

const PAD: f32 = 12.0;
const AVATAR: f32 = 22.0;
const TEXT_X: f32 = PAD + AVATAR + 8.0;
const BODY_DY: f32 = 28.0;
const COMMENT_GAP: f32 = 12.0;
const FIELD_H: f32 = 36.0;
const FIELD_PAD_X: i8 = 10;
const FIELD_PAD_Y: i8 = 9;
const ROW_GAP: f32 = 8.0;
pub(super) const BUTTON_H: f32 = 28.0;
const BUTTON_PAD_X: f32 = 12.0;
const CARD_RADIUS: u8 = 8;
const BAR_W: f32 = 2.0;
pub(super) const CARD_MARGIN_Y: f32 = 6.0;
pub(super) const DEFAULT_LINE: Color32 = Color32::from_rgb(0x33, 0x40, 0x55);

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

pub(super) fn button(
    ui: &mut Ui,
    label: &str,
    kind: ButtonKind,
    enabled: bool,
    style: &DiffStyle,
) -> Response {
    let (_, line) = style.field();
    let accent = style.accent();
    let (ink, fill, border) = match (kind, enabled) {
        (ButtonKind::Primary, true) => (theme::on_accent(), accent, accent),
        (ButtonKind::Primary, false) => (
            theme::with_alpha(theme::on_accent(), 0xb0),
            theme::with_alpha(accent, 0x55),
            theme::with_alpha(accent, 0x55),
        ),
        (ButtonKind::Quiet, true) => (theme::text(), Color32::TRANSPARENT, line),
        (ButtonKind::Quiet, false) => (
            theme::with_alpha(theme::text(), 0x66),
            Color32::TRANSPARENT,
            line,
        ),
        (ButtonKind::Danger, _) => (
            theme::deleted(),
            Color32::TRANSPARENT,
            theme::with_alpha(theme::deleted(), 0x66),
        ),
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), FontId::proportional(12.0), ink);
    let size = vec2(galley.size().x + BUTTON_PAD_X * 2.0, BUTTON_H);
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

/// The card every note shares: the style's fill and a quiet 1 px line, with a 2 px accent bar
/// down the left edge.
pub(super) fn paint_card(ui: &Ui, card: Rect, style: &DiffStyle) {
    let (fill, line) = style.note(theme::tone(DEFAULT_LINE));
    ui.painter().rect(
        card,
        CARD_RADIUS,
        fill,
        Stroke::new(1.0, line),
        StrokeKind::Inside,
    );
    ui.painter_at(Rect::from_min_size(card.min, vec2(BAR_W, card.height())))
        .rect_filled(card, CARD_RADIUS, style.accent());
}

/// Reserves the layer under a field's text, so its fill can be painted once the text edit has
/// said whether it has focus.
pub(super) fn field_slot(ui: &Ui) -> ShapeIdx {
    ui.painter().add(Shape::Noop)
}

/// A field's fill and 1 px line; the accent shows only while it has focus, as a thin ring.
pub(super) fn paint_field(ui: &Ui, slot: ShapeIdx, rect: Rect, focused: bool, style: &DiffStyle) {
    let (fill, line) = style.field();
    let accent = style.accent();
    let stroke = Stroke::new(1.0, if focused { accent } else { line });
    let mut shapes = vec![Shape::Rect(RectShape::new(
        rect,
        CARD_RADIUS,
        fill,
        stroke,
        StrokeKind::Inside,
    ))];
    if focused {
        shapes.push(Shape::Rect(RectShape::stroke(
            rect.expand(2.0),
            CARD_RADIUS + 2,
            Stroke::new(2.0, theme::with_alpha(accent, 0x38)),
            StrokeKind::Inside,
        )));
    }
    ui.painter().set(slot, Shape::Vec(shapes));
}

pub(super) fn avatar(ui: &Ui, center: egui::Pos2, name: &str, accent: Color32) {
    ui.painter()
        .circle_filled(center, AVATAR / 2.0, theme::with_alpha(accent, 0x26));
    ui.painter().text(
        center,
        Align2::CENTER_CENTER,
        kelp_core::avatar::initials(name),
        FontId::new(10.0, theme::semibold()),
        accent,
    );
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
        for (n, comment) in thread.comments.iter().enumerate() {
            let center = pos2(card.left() + PAD + AVATAR / 2.0, y + AVATAR / 2.0);
            avatar(ui, center, &comment.author, accent);
            if n == 0 && thread.anchor.span > 0 {
                ui.painter().text(
                    pos2(card.right() - PAD, center.y),
                    Align2::RIGHT_CENTER,
                    anchor_label(&thread.anchor),
                    FontId::proportional(12.0),
                    theme::text_faint(),
                );
            }
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
        let field_rect =
            Rect::from_min_size(child.cursor().min, vec2(child.available_width(), FIELD_H));
        let inner = field_rect.shrink2(vec2(
            f32::from(FIELD_PAD_X) + 1.0,
            f32::from(FIELD_PAD_Y) + 1.0,
        ));
        let slot = field_slot(&child);
        let mut field = child.new_child(UiBuilder::new().max_rect(inner));
        let draft = self.replies.entry(id).or_default();
        let edit = field.add(
            TextEdit::multiline(draft)
                .frame(egui::Frame::NONE)
                .margin(Margin::ZERO)
                .font(FontId::proportional(13.0))
                .hint_text("Reply")
                .desired_rows(1)
                .desired_width(f32::INFINITY),
        );
        let has_text = !draft.trim().is_empty();
        if edit.has_focus() {
            self.lit_next = Some(id);
        }
        paint_field(&child, slot, field_rect, edit.has_focus(), &input.style);
        child.allocate_rect(field_rect, Sense::hover());
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
                let galley = ui.painter().layout_no_wrap(
                    hint.clone(),
                    egui::FontId::proportional(11.0),
                    theme::text_faint(),
                );
                if super::composer::hint_fits(ui.available_width(), galley.size().x) {
                    ui.label(
                        egui::RichText::new(hint)
                            .size(11.0)
                            .color(theme::text_faint()),
                    );
                }
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
            12.0 + 28.0 + 32.0 + 12.0 + 36.0 + 8.0 + 28.0 + 12.0 + 12.0
        );
        assert_eq!(thread_height([32.0, 16.0]), one + 28.0 + 16.0 + 12.0);
        assert_eq!(body_width(640.0), 640.0 - 42.0 - 12.0);
    }
}
