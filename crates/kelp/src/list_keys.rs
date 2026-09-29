use eframe::egui::{self, Id, Key, Modifiers, Rect, Ui};

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

/// Moves through `rows` with Up, Down, Home and End. A focused row keeps
/// the arrows and hands focus to the new row; otherwise the keys only
/// count when `owns_keys` is true and move on from `active`.
pub fn step(ui: &Ui, rows: &[(Id, Rect)], active: Option<usize>, owns_keys: bool) -> Option<usize> {
    let ctx = ui.ctx();
    let focused_at = focused_row(ctx, rows);
    if let Some(at) = focused_at {
        let filter = egui::EventFilter {
            vertical_arrows: true,
            ..Default::default()
        };
        ctx.memory_mut(|m| m.set_focus_lock_filter(rows[at].0, filter));
    } else if !owns_keys || is_typing(ctx) {
        return None;
    }
    let step = read(ctx)?;
    let index = target(step, focused_at.or(active), rows.len())?;
    if focused_at.is_some() {
        ctx.memory_mut(|m| m.request_focus(rows[index].0));
    }
    ui.scroll_to_rect(rows[index].1, None);
    Some(index)
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
