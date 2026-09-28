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
                .layout_no_wrap(label.to_string(), font.clone(), theme::text())
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
        theme::field(),
        Stroke::new(1.0, theme::border()),
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
            theme::control_active()
        } else if response.hovered() {
            theme::control()
        } else {
            egui::Color32::TRANSPARENT
        };
        painter.rect_filled(segment, CornerRadius::same(4), fill);
        let color = if active {
            theme::text_strong()
        } else if response.hovered() {
            theme::text()
        } else {
            theme::text_muted()
        };
        let pos = Align2::CENTER_CENTER
            .align_size_within_rect(galley.size(), segment)
            .min;
        painter.galley(pos, galley, color);
        focus_ring(ui, &response, 4.0);
        describe_selected(&response, options[i].1, active);
        if response.clicked() {
            *value = *option;
        }
    }
}

pub fn icon_button(ui: &mut Ui, icon: crate::icons::Icon, hint: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    let response = response
        .on_hover_text(hint)
        .on_hover_cursor(CursorIcon::PointingHand);
    let color = if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), theme::control_hover());
        theme::text_strong()
    } else {
        theme::text_muted()
    };
    let glyph = Rect::from_center_size(rect.center(), vec2(15.0, 15.0));
    crate::icons::paint(ui.painter(), glyph, icon, color);
    focus_ring(ui, &response, 5.0);
    describe(&response, egui::WidgetType::Button, hint);
    response.clicked()
}

pub fn close_button(ui: &mut Ui, hint: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    let response = response
        .on_hover_text(hint)
        .on_hover_cursor(CursorIcon::PointingHand);
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(5), theme::control_hover());
    }
    let color = if response.hovered() {
        theme::text_strong()
    } else {
        theme::text_muted()
    };
    let c = rect.center();
    let d = 4.5;
    let stroke = Stroke::new(1.5, color);
    painter.line_segment([c + vec2(-d, -d), c + vec2(d, d)], stroke);
    painter.line_segment([c + vec2(-d, d), c + vec2(d, -d)], stroke);
    focus_ring(ui, &response, 5.0);
    describe(&response, egui::WidgetType::Button, hint);
    response.clicked()
}

const KEYBOARD_MODE: &str = "kelp-keyboard-mode";
const FOCUS_RING_W: f32 = 2.0;

pub fn track_input_mode(ctx: &egui::Context) {
    use egui::Key;
    let (keyboard, pointer) = ctx.input(|i| {
        let keyboard = i.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::Key {
                    key: Key::Tab
                        | Key::ArrowUp
                        | Key::ArrowDown
                        | Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::F6
                        | Key::F10
                        | Key::Enter
                        | Key::Space,
                    pressed: true,
                    ..
                }
            )
        });
        (keyboard, i.pointer.any_pressed())
    });
    if keyboard || pointer {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(KEYBOARD_MODE), keyboard && !pointer));
    }
}

pub fn enter_keyboard_mode(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(KEYBOARD_MODE), true));
}

pub fn keyboard_mode(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp(egui::Id::new(KEYBOARD_MODE)))
        .unwrap_or(false)
}

pub fn focus_ring(ui: &Ui, response: &egui::Response, radius: f32) {
    if response.has_focus() && keyboard_mode(ui.ctx()) {
        ui.painter().rect_stroke(
            response.rect.shrink(1.0),
            radius,
            Stroke::new(FOCUS_RING_W, theme::accent()),
            egui::StrokeKind::Inside,
        );
    }
}

pub fn describe(response: &egui::Response, kind: egui::WidgetType, label: impl Into<String>) {
    let label = label.into();
    let enabled = response.enabled();
    response.widget_info(|| egui::WidgetInfo::labeled(kind, enabled, label.clone()));
}

pub fn describe_selected(response: &egui::Response, label: impl Into<String>, selected: bool) {
    let label = label.into();
    let enabled = response.enabled();
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            enabled,
            selected,
            label.clone(),
        )
    });
    response
        .ctx
        .accesskit_node_builder(response.id, |node| node.set_selected(selected));
}

pub fn describe_toggle(response: &egui::Response, label: impl Into<String>, on: bool) {
    let label = label.into();
    let enabled = response.enabled();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, on, label.clone())
    });
}

pub fn enter_pressed(response: &egui::Response) -> bool {
    response.has_focus() && response.ctx.input(|i| i.key_pressed(egui::Key::Enter))
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
                segmented(
                    ui,
                    &mut state.first,
                    &[(Pick::A, "Split"), (Pick::B, "Unified")],
                );
                segmented(
                    ui,
                    &mut state.second,
                    &[(Pick::A, "File"), (Pick::B, "Diff")],
                );
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
