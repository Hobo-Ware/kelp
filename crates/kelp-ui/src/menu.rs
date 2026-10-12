use egui::{self, Align2, FontId, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::icons::{self, Icon};
use crate::theme;

const ROW_H: f32 = 30.0;

pub fn row(ui: &mut Ui, icon: Option<Icon>, label: &str, hint: Option<&str>, danger: bool) -> bool {
    let width = ui.available_width().max(ui.min_rect().width());
    let (rect, response) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
    let painter = ui.painter_at(rect.expand(1.0));
    if ui
        .ctx()
        .data_mut(|d| d.remove_temp::<bool>(egui::Id::new(FOCUS_FIRST_ROW)))
        == Some(true)
    {
        response.request_focus();
    }
    ui.ctx().data_mut(|d| {
        if let Some(mut rows) = d.get_temp::<Vec<egui::Id>>(egui::Id::new(KEYBOARD_ROWS)) {
            rows.push(response.id);
            d.insert_temp(egui::Id::new(KEYBOARD_ROWS), rows);
        }
    });
    let hovered =
        response.hovered() || (response.has_focus() && crate::widgets::keyboard_mode(ui.ctx()));
    if hovered {
        let fill = if danger {
            theme::with_alpha(theme::deleted(), 0x22)
        } else {
            theme::menu_hover()
        };
        painter.rect_filled(rect, 6.0, fill);
    }
    let text = match (danger, hovered) {
        (true, _) => theme::deleted(),
        (false, true) => theme::text_strong(),
        (false, false) => theme::text_control(),
    };
    let icon_color = if danger {
        theme::deleted()
    } else if hovered {
        theme::text_strong()
    } else {
        theme::text_muted()
    };
    if let Some(icon) = icon {
        let icon_rect =
            Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(15.0, 15.0));
        icons::paint(&painter, icon_rect, icon, icon_color);
    }
    painter.text(
        pos2(rect.left() + 36.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        text,
    );
    if let Some(hint) = hint {
        painter.text(
            pos2(rect.right() - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            hint,
            FontId::proportional(11.5),
            theme::text_faint(),
        );
    }
    crate::widgets::focus_ring(ui, &response, 6.0);
    crate::widgets::describe(&response, egui::WidgetType::Button, label);
    if response.clicked() {
        ui.close();
        return true;
    }
    false
}

const FOCUS_FIRST_ROW: &str = "kelp-menu-focus-first-row";
const KEYBOARD_ROWS: &str = "kelp-menu-keyboard-rows";

fn step_between_rows(ctx: &egui::Context, rows: &[egui::Id]) {
    let focused = ctx.memory(|m| m.focused());
    let Some(at) = rows.iter().position(|r| Some(*r) == focused) else {
        return;
    };
    let arrows_stay_here = egui::EventFilter {
        vertical_arrows: true,
        ..Default::default()
    };
    ctx.memory_mut(|m| m.set_focus_lock_filter(rows[at], arrows_stay_here));
    let step = ctx.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
            1
        } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
            -1
        } else {
            0
        }
    });
    if step != 0 {
        let next = (at as i32 + step).rem_euclid(rows.len() as i32) as usize;
        ctx.memory_mut(|m| m.request_focus(rows[next]));
    }
}

fn opened_from_keyboard(ctx: &egui::Context, response: &egui::Response) -> bool {
    response.has_focus() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F10))
}

pub fn context_menu(
    response: &egui::Response,
    add_contents: impl FnOnce(&mut Ui),
) -> Option<egui::InnerResponse<()>> {
    context_menu_at(
        response,
        response.rect.left_bottom() + vec2(16.0, 0.0),
        add_contents,
    )
}

pub fn context_menu_at(
    response: &egui::Response,
    keyboard_anchor: egui::Pos2,
    add_contents: impl FnOnce(&mut Ui),
) -> Option<egui::InnerResponse<()>> {
    let ctx = response.ctx.clone();
    let popup_id = egui::Popup::default_response_id(response);
    let anchored = popup_id.with("keyboard");
    if opened_from_keyboard(&ctx, response) {
        ctx.data_mut(|d| {
            d.insert_temp(anchored, true);
            d.insert_temp(egui::Id::new(FOCUS_FIRST_ROW), true);
        });
        egui::Popup::open_id(&ctx, popup_id);
    }
    let keyboard = ctx.data(|d| d.get_temp::<bool>(anchored)).unwrap_or(false);
    if keyboard && egui::Popup::is_id_open(&ctx, popup_id) {
        let rows_key = egui::Id::new(KEYBOARD_ROWS);
        ctx.data_mut(|d| d.insert_temp(rows_key, Vec::<egui::Id>::new()));
        let shown = egui::Popup::menu(response)
            .open_memory(None)
            .at_position(keyboard_anchor)
            .show(add_contents);
        let rows: Vec<egui::Id> = ctx
            .data_mut(|d| d.remove_temp(rows_key))
            .unwrap_or_default();
        step_between_rows(&ctx, &rows);
        return shown;
    }
    if keyboard {
        ctx.data_mut(|d| d.remove_temp::<bool>(anchored));
        response.request_focus();
    }
    response.context_menu(add_contents)
}

pub fn heading(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
    let g = crate::paint::truncated(
        ui.painter(),
        text.to_string(),
        FontId::monospace(11.0),
        theme::text_faint(),
        rect.width() - 20.0,
    );
    ui.painter().galley(
        pos2(rect.left() + 10.0, rect.center().y - g.size().y / 2.0),
        g,
        theme::text_faint(),
    );
}

pub fn separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(
        rect.left() + 8.0..=rect.right() - 8.0,
        rect.center().y,
        Stroke::new(1.0, theme::separator()),
    );
}

pub fn menu_width(ui: &mut Ui, width: f32) {
    ui.set_min_width(width);
    ui.set_max_width(width);
    ui.spacing_mut().item_spacing.y = 0.0;
}
