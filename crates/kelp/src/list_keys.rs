use std::ops::Range;

use eframe::egui::{self, Id, Key, Modifiers, Rect, Ui, pos2, vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Prev,
    Next,
    First,
    Last,
}

pub fn target(step: Step, current: Option<usize>, len: usize) -> Option<usize> {
    let last = len.checked_sub(1)?;
    Some(match (step, current) {
        (Step::First, _) => 0,
        (Step::Last, _) => last,
        (Step::Next, None) => 0,
        (Step::Prev, None) => last,
        (Step::Next, Some(at)) => (at + 1).min(last),
        (Step::Prev, Some(at)) => at.saturating_sub(1),
    })
}

pub fn is_typing(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.focused())
        .is_some_and(|id| egui::text_edit::TextEditState::load(ctx, id).is_some())
}

pub fn nothing_focused(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.focused()).is_none()
}

fn read(ctx: &egui::Context) -> Option<Step> {
    ctx.input_mut(|i| {
        [
            (Key::ArrowDown, Step::Next),
            (Key::ArrowUp, Step::Prev),
            (Key::Home, Step::First),
            (Key::End, Step::Last),
        ]
        .into_iter()
        .find(|(key, _)| i.consume_key(Modifiers::NONE, *key))
        .map(|(_, step)| step)
    })
}

fn focused_row(ctx: &egui::Context, rows: &[(Id, Rect)]) -> Option<usize> {
    let focused = ctx.memory(|m| m.focused());
    rows.iter().position(|(id, _)| Some(*id) == focused)
}

pub fn current(
    ctx: &egui::Context,
    rows: &[(Id, Rect)],
    active: Option<usize>,
    owns_keys: bool,
) -> Option<usize> {
    focused_row(ctx, rows).or(active.filter(|_| owns_keys && !is_typing(ctx)))
}

pub fn pressed(ctx: &egui::Context, key: Key) -> bool {
    ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key))
}

pub fn keep_arrows(ctx: &egui::Context, id: Id) {
    let filter = egui::EventFilter {
        vertical_arrows: true,
        ..Default::default()
    };
    ctx.memory_mut(|m| m.set_focus_lock_filter(id, filter));
}

pub fn step_index(
    ctx: &egui::Context,
    focused_at: Option<usize>,
    active: Option<usize>,
    len: usize,
    owns_keys: bool,
) -> Option<usize> {
    if focused_at.is_none() && (!owns_keys || is_typing(ctx)) {
        return None;
    }
    target(read(ctx)?, focused_at.or(active), len)
}

/// Moves through `rows` with Up, Down, Home and End. A focused row keeps
/// the arrows and hands focus to the new row; otherwise the keys only
/// count when `owns_keys` is true and move on from `active`.
pub fn step(ui: &Ui, rows: &[(Id, Rect)], active: Option<usize>, owns_keys: bool) -> Option<usize> {
    let ctx = ui.ctx();
    let focused_at = focused_row(ctx, rows);
    if let Some(at) = focused_at {
        keep_arrows(ctx, rows[at].0);
    }
    let index = step_index(ctx, focused_at, active, rows.len(), owns_keys)?;
    if focused_at.is_some() {
        ctx.memory_mut(|m| m.request_focus(rows[index].0));
    }
    ui.scroll_to_rect(rows[index].1, None);
    Some(index)
}

pub struct Rows<F> {
    pub visible: Range<usize>,
    pub len: usize,
    pub height: f32,
    pub id: F,
}

/// The same keys for a `ScrollArea::show_rows` list: call it inside the
/// rows closure before drawing, with every row's widget id from `rows.id`.
pub fn step_rows(
    ui: &Ui,
    rows: Rows<impl Fn(usize) -> Id>,
    active: Option<usize>,
    owns_keys: bool,
) -> Option<usize> {
    let ctx = ui.ctx();
    let focused_at = rows
        .visible
        .clone()
        .find(|&i| ctx.memory(|m| m.has_focus((rows.id)(i))));
    if let Some(at) = focused_at {
        keep_arrows(ctx, (rows.id)(at));
    }
    let index = step_index(ctx, focused_at, active, rows.len, owns_keys)?;
    if focused_at.is_some() {
        ctx.memory_mut(|m| m.request_focus((rows.id)(index)));
    }
    let offset = (index as f32 - rows.visible.start as f32) * rows.height;
    let top = ui.cursor().top() + offset;
    let rect = Rect::from_min_size(
        pos2(ui.cursor().left(), top),
        vec2(ui.available_width(), rows.height),
    );
    ui.scroll_to_rect(rect, None);
    Some(index)
}

#[cfg(test)]
pub fn tap(key: Key) -> Vec<egui::Event> {
    [true, false]
        .map(|pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        })
        .to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_stop_at_the_ends() {
        assert_eq!(target(Step::Next, Some(2), 3), Some(2));
        assert_eq!(target(Step::Prev, Some(0), 3), Some(0));
        assert_eq!(target(Step::Next, Some(0), 3), Some(1));
        assert_eq!(target(Step::Prev, Some(2), 3), Some(1));
    }

    #[test]
    fn without_a_current_row_down_starts_at_the_top_and_up_at_the_bottom() {
        assert_eq!(target(Step::Next, None, 4), Some(0));
        assert_eq!(target(Step::Prev, None, 4), Some(3));
        assert_eq!(target(Step::First, Some(3), 4), Some(0));
        assert_eq!(target(Step::Last, None, 4), Some(3));
    }

    #[test]
    fn an_empty_list_goes_nowhere() {
        assert_eq!(target(Step::Next, None, 0), None);
        assert_eq!(target(Step::Last, Some(0), 0), None);
    }
}
