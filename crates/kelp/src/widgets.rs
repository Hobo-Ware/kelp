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
    let (rect, control) = ui.allocate_exact_size(vec2(total, SEGMENT_H), Sense::hover());
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
            .interact(segment, control.id.with(i), Sense::click())
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

#[cfg(test)]
mod tests {
    use eframe::egui::{self, Event, PointerButton, Pos2, RawInput, Rect, pos2, vec2};

    use super::segmented;

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Pick {
        A,
        B,
    }

    struct Frame {
        first: Pick,
        second: Pick,
        rects: Vec<Rect>,
    }

    fn frame(ctx: &egui::Context, state: &mut Frame, events: Vec<Event>) {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 200.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run_ui(input, |ui| {
            ui.horizontal(|ui| {
                let before = ui.min_rect();
                segmented(ui, &mut state.first, &[(Pick::A, "Split"), (Pick::B, "Unified")]);
                segmented(ui, &mut state.second, &[(Pick::A, "File"), (Pick::B, "Diff")]);
                state.rects = vec![before, ui.min_rect()];
            });
        });
    }

    fn click(ctx: &egui::Context, state: &mut Frame, at: Pos2) {
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(ctx, state, vec![Event::PointerMoved(at)]);
        frame(ctx, state, vec![press(true)]);
        frame(ctx, state, vec![press(false)]);
        frame(ctx, state, vec![]);
    }

    #[test]
    fn each_control_toggles_on_its_own() {
        let ctx = egui::Context::default();
        let mut state = Frame {
            first: Pick::B,
            second: Pick::B,
            rects: Vec::new(),
        };
        frame(&ctx, &mut state, vec![]);
        let row = state.rects[1];
        click(&ctx, &mut state, pos2(row.left() + 12.0, row.center().y));
        assert_eq!((state.first, state.second), (Pick::A, Pick::B));
        click(&ctx, &mut state, pos2(row.right() - 60.0, row.center().y));
        assert_eq!(state.second, Pick::A);
        assert_eq!(state.first, Pick::A);
    }
}
