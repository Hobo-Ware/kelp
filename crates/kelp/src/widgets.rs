use eframe::egui::{self, Align2, CornerRadius, CursorIcon, FontId, Rect, Sense, Stroke, Ui, vec2};

use crate::theme;

const SEGMENT_H: f32 = 24.0;
const SEGMENT_PAD: f32 = 10.0;
const INSET: f32 = 2.0;

pub fn segmented<T: Copy + PartialEq>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) {
    let font = FontId::proportional(12.0);
    let galleys: Vec<_> = options
        .iter()
        .map(|(_, label)| {
            ui.painter()
                .layout_no_wrap(label.to_string(), font.clone(), theme::TEXT)
        })
        .collect();
    let widths: Vec<f32> = galleys
        .iter()
        .map(|g| g.size().x + SEGMENT_PAD * 2.0)
        .collect();
    let total = widths.iter().sum::<f32>() + INSET * 2.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, SEGMENT_H), Sense::hover());
    let painter = ui.painter_at(rect.expand(1.0));
    painter.rect(
        rect,
        CornerRadius::same(6),
        theme::FIELD,
        Stroke::new(1.0, theme::BORDER),
        egui::StrokeKind::Inside,
    );
    let mut x = rect.left() + INSET;
    for (i, ((option, _), galley)) in options.iter().zip(galleys).enumerate() {
        let segment = Rect::from_min_size(
            egui::pos2(x, rect.top() + INSET),
            vec2(widths[i], SEGMENT_H - INSET * 2.0),
        );
        x += widths[i];
        let response = ui
            .interact(segment, ui.id().with(("segment", i)), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        let active = *value == *option;
        let fill = if active {
            theme::CONTROL_ACTIVE
        } else if response.hovered() {
            theme::CONTROL
        } else {
            egui::Color32::TRANSPARENT
        };
        painter.rect_filled(segment, CornerRadius::same(4), fill);
        let color = if active {
            theme::TEXT_STRONG
        } else if response.hovered() {
            theme::TEXT
        } else {
            theme::TEXT_MUTED
        };
        let pos = Align2::CENTER_CENTER
            .align_size_within_rect(galley.size(), segment)
            .min;
        painter.galley(pos, galley, color);
        if response.clicked() {
            *value = *option;
        }
    }
}

pub fn close_button(ui: &mut Ui, hint: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    let response = response
        .on_hover_text(hint)
        .on_hover_cursor(CursorIcon::PointingHand);
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(5), theme::CONTROL_HOVER);
    }
    let color = if response.hovered() {
        theme::TEXT_STRONG
    } else {
        theme::TEXT_MUTED
    };
    let c = rect.center();
    let d = 4.5;
    let stroke = Stroke::new(1.5, color);
    painter.line_segment([c + vec2(-d, -d), c + vec2(d, d)], stroke);
    painter.line_segment([c + vec2(-d, d), c + vec2(d, -d)], stroke);
    response.clicked()
}
