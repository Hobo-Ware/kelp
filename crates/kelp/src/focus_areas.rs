use eframe::egui::{self, Id, Key, Modifiers, Rect};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Area {
    Sidebar,
    Graph,
    Details,
}

const ORDER: [Area; 3] = [Area::Sidebar, Area::Graph, Area::Details];

#[derive(Clone, Copy, Default)]
struct Spot {
    panel: Option<Rect>,
    anchor: Option<(Id, bool)>,
}

#[derive(Clone, Copy, Default)]
struct Frame {
    spots: [Spot; 3],
}

fn slot(area: Area) -> usize {
    ORDER.iter().position(|a| *a == area).unwrap_or(1)
}

fn building_id() -> Id {
    Id::new("kelp-focus-areas-building")
}

fn last_id() -> Id {
    Id::new("kelp-focus-areas-last")
}

pub fn begin_frame(ctx: &egui::Context) {
    ctx.data_mut(|d| {
        let built: Frame = d.get_temp(building_id()).unwrap_or_default();
        d.insert_temp(last_id(), built);
        d.insert_temp(building_id(), Frame::default());
    });
}

pub fn mark_panel(ctx: &egui::Context, area: Area, rect: Rect) {
    ctx.data_mut(|d| {
        let mut frame: Frame = d.get_temp(building_id()).unwrap_or_default();
        frame.spots[slot(area)].panel = Some(rect);
        d.insert_temp(building_id(), frame);
    });
}

pub fn offer(ctx: &egui::Context, area: Area, id: Id, preferred: bool) {
    ctx.data_mut(|d| {
        let mut frame: Frame = d.get_temp(building_id()).unwrap_or_default();
        let spot = &mut frame.spots[slot(area)];
        let replace = match spot.anchor {
            None => true,
            Some((_, was_preferred)) => preferred && !was_preferred,
        };
        if replace {
            spot.anchor = Some((id, preferred));
        }
        d.insert_temp(building_id(), frame);
    });
}

fn last(ctx: &egui::Context) -> Frame {
    ctx.data(|d| d.get_temp(last_id())).unwrap_or_default()
}

pub fn current(ctx: &egui::Context) -> Area {
    let frame = last(ctx);
    let focused = ctx
        .memory(|m| m.focused())
        .and_then(|id| ctx.read_response(id))
        .map(|r| r.rect.center());
    focused
        .and_then(|point| {
            ORDER.into_iter().find(|a| {
                frame.spots[slot(*a)]
                    .panel
                    .is_some_and(|p| p.contains(point))
            })
        })
        .unwrap_or(Area::Graph)
}

pub fn next(from: Area, step: i32, available: impl Fn(Area) -> bool) -> Option<Area> {
    let start = slot(from) as i32;
    let len = ORDER.len() as i32;
    (1..=len)
        .map(|k| ORDER[(start + step * k).rem_euclid(len) as usize])
        .find(|a| available(*a))
}

pub fn focus(ctx: &egui::Context, area: Area) -> bool {
    match last(ctx).spots[slot(area)].anchor {
        Some((id, _)) => {
            ctx.memory_mut(|m| m.request_focus(id));
            true
        }
        None => false,
    }
}

fn parse(name: &str) -> Option<Area> {
    match name {
        "sidebar" => Some(Area::Sidebar),
        "graph" => Some(Area::Graph),
        "details" => Some(Area::Details),
        _ => None,
    }
}

pub fn apply_dev_focus(ctx: &egui::Context) {
    let Some(area) = std::env::var("KELP_FOCUS").ok().as_deref().and_then(parse) else {
        return;
    };
    if ctx.memory(|m| m.focused().is_none()) && focus(ctx, area) {
        crate::widgets::enter_keyboard_mode(ctx);
    }
}

pub fn handle_keys(ctx: &egui::Context) {
    let step = ctx.input_mut(|i| {
        if i.consume_key(Modifiers::SHIFT, Key::F6) {
            -1
        } else if i.consume_key(Modifiers::NONE, Key::F6) {
            1
        } else {
            0
        }
    });
    if step == 0 {
        return;
    }
    let frame = last(ctx);
    let target = next(current(ctx), step, |a| {
        frame.spots[slot(a)].anchor.is_some()
    });
    if let Some(area) = target {
        focus(ctx, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f6_cycles_through_available_areas() {
        let all = |_: Area| true;
        assert_eq!(next(Area::Graph, 1, all), Some(Area::Details));
        assert_eq!(next(Area::Details, 1, all), Some(Area::Sidebar));
        assert_eq!(next(Area::Sidebar, -1, all), Some(Area::Details));
        let no_sidebar = |a: Area| a != Area::Sidebar;
        assert_eq!(next(Area::Details, 1, no_sidebar), Some(Area::Graph));
        assert_eq!(
            next(Area::Graph, 1, |a| a == Area::Graph),
            Some(Area::Graph)
        );
        assert_eq!(next(Area::Graph, 1, |_| false), None);
    }

    fn frame(ctx: &egui::Context, key: Option<Modifiers>, ids: &mut Vec<Id>) {
        let events = key
            .map(|modifiers| {
                [true, false]
                    .map(|pressed| egui::Event::Key {
                        key: Key::F6,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers,
                    })
                    .to_vec()
            })
            .unwrap_or_default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 400.0),
            )),
            modifiers: key.unwrap_or_default(),
            events,
            ..Default::default()
        };
        ids.clear();
        let _ = ctx.run_ui(input, |ui| {
            begin_frame(ui.ctx());
            handle_keys(ui.ctx());
            for (area, x) in [
                (Area::Sidebar, 0.0),
                (Area::Graph, 300.0),
                (Area::Details, 600.0),
            ] {
                let panel = Rect::from_min_size(egui::pos2(x, 0.0), egui::vec2(300.0, 400.0));
                mark_panel(ui.ctx(), area, panel);
                ui.scope_builder(egui::UiBuilder::new().max_rect(panel), |ui| {
                    for preferred in [false, true] {
                        let response = ui.button(format!("{area:?} {preferred}"));
                        offer(ui.ctx(), area, response.id, preferred);
                        ids.push(response.id);
                    }
                });
            }
        });
    }

    #[test]
    fn f6_moves_focus_to_the_preferred_widget_of_each_area() {
        let ctx = egui::Context::default();
        let mut ids = Vec::new();
        frame(&ctx, None, &mut ids);
        frame(&ctx, None, &mut ids);
        let focused = || ctx.memory(|m| m.focused());
        let (sidebar, graph, details) = (ids[1], ids[3], ids[5]);
        for (key, expected) in [
            (Modifiers::NONE, details),
            (Modifiers::NONE, sidebar),
            (Modifiers::NONE, graph),
            (Modifiers::SHIFT, sidebar),
            (Modifiers::SHIFT, details),
        ] {
            frame(&ctx, Some(key), &mut ids);
            frame(&ctx, None, &mut ids);
            assert_eq!(focused(), Some(expected));
        }
    }
}
