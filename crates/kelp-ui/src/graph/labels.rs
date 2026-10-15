use egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Pos2, Rect, Sense, Stroke, Ui,
    containers::menu::SubMenu, pos2, vec2,
};
use kelp_core::refs::{RefKind, RefLabel};

use super::Selection;
use crate::theme;

const LABEL_H: f32 = 22.0;
const LABEL_MAX_TEXT_W: f32 = 114.0;
const LIST_ROW_H: f32 = 30.0;
const LIST_W: f32 = 260.0;
const PILL_GAP: f32 = 6.0;

/// What a graph context menu is for.
pub enum MenuFor<'a> {
    /// A commit row (or the uncommitted changes row), with the commit title.
    Commit(Selection, &'a str),
    Ref(&'a RefLabel),
    /// A ref chip dropped on a commit or another chip.
    Drop(&'a DropPlan),
    Worktree(&'a kelp_core::workspace::Worktree),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Label(usize),
    Pull(usize),
    More,
}

#[derive(Clone, Copy, Debug)]
pub struct Placed {
    pub rect: Rect,
    pub slot: Slot,
}

#[derive(Clone, Debug)]
pub struct Drag {
    pub label: RefLabel,
    pub from_row: usize,
}

/// Where a dragged ref chip was dropped: a commit row, and the chip on it if there was one.
#[derive(Clone, Debug)]
pub struct DropTarget {
    pub row: usize,
    pub commit: String,
    pub label: Option<RefLabel>,
    pub is_head: bool,
}

/// A ref chip dropped on a target, with the pointer position the menu should open at.
#[derive(Clone, Debug)]
pub struct DropPlan {
    pub source: RefLabel,
    pub target: DropTarget,
    pub pos: Pos2,
}

pub enum LabelEvent {
    Select,
    Activate(RefLabel),
    OpenUrl(String),
    DragStart(RefLabel),
    DragStop,
}

const LABEL_PAD: f32 = 16.0;
const LABEL_MIN_TEXT_W: f32 = 40.0;
const MORE_CHIP_W: f32 = 30.0;

pub type RemoteBadges = std::collections::HashMap<String, egui::TextureId>;

const BADGE_SIZE: f32 = 14.0;
const BADGE_GAP: f32 = 5.0;

fn remote_badge<'a>(
    label: &'a RefLabel,
    badges: &RemoteBadges,
) -> Option<(egui::TextureId, &'a str)> {
    if label.kind != RefKind::Remote {
        return None;
    }
    let (remote, branch) = label.name.split_once('/')?;
    Some((*badges.get(remote)?, branch))
}

struct Laid<'a> {
    label: &'a RefLabel,
    galley: std::sync::Arc<egui::Galley>,
    badge: Option<egui::TextureId>,
    pill_w: f32,
    width: f32,
}

fn lay<'a>(
    painter: &egui::Painter,
    label: &'a RefLabel,
    badges: &RemoteBadges,
    text_limit: f32,
) -> Laid<'a> {
    let pill_w = label.pull.as_ref().map_or(0.0, |pull| {
        crate::pulls_ui::pill_width(painter, pull) + PILL_GAP
    });
    let badge = remote_badge(label, badges);
    let badge_w = if badge.is_some() {
        BADGE_SIZE + BADGE_GAP
    } else {
        0.0
    };
    let text_room = (text_limit - LABEL_PAD - pill_w - badge_w).min(LABEL_MAX_TEXT_W);
    let text = badge.map_or_else(|| label_text(label), |(_, branch)| branch.to_string());
    let galley = crate::paint::truncated(
        painter,
        text,
        FontId::proportional(12.0),
        Color32::PLACEHOLDER,
        text_room.max(0.0),
    );
    let width = galley.size().x + LABEL_PAD + pill_w + badge_w;
    Laid {
        label,
        galley,
        badge: badge.map(|(texture, _)| texture),
        pill_w,
        width,
    }
}

fn draw(
    painter: &egui::Painter,
    laid: Laid<'_>,
    right: f32,
    mid: f32,
    lane: Color32,
    index: usize,
    placed: &mut Vec<Placed>,
) {
    let rect = Rect::from_min_size(
        pos2(right - laid.width, mid - LABEL_H / 2.0),
        vec2(laid.width, LABEL_H),
    );
    let (fill, stroke, ink) = label_style(laid.label, lane);
    painter.rect(
        rect,
        CornerRadius::same(5),
        fill,
        stroke,
        egui::StrokeKind::Inside,
    );
    let mut text_left = rect.left() + 8.0;
    if let Some(texture) = laid.badge {
        let spot = Rect::from_center_size(
            pos2(rect.left() + 7.0 + BADGE_SIZE / 2.0, mid),
            vec2(BADGE_SIZE, BADGE_SIZE),
        );
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        painter.image(texture, spot, uv, Color32::WHITE);
        text_left += BADGE_SIZE + BADGE_GAP;
    }
    painter.galley(
        pos2(text_left, mid - laid.galley.size().y / 2.0),
        laid.galley,
        ink,
    );
    placed.push(Placed {
        rect,
        slot: Slot::Label(index),
    });
    if let Some(pull) = &laid.label.pull {
        let pill = Rect::from_min_size(
            pos2(
                rect.right() - laid.pill_w + PILL_GAP - 4.0,
                mid - crate::pulls_ui::PILL_H / 2.0,
            ),
            vec2(laid.pill_w - PILL_GAP, crate::pulls_ui::PILL_H),
        );
        crate::pulls_ui::paint_pill(painter, pill, pull, false);
        placed.push(Placed {
            rect: pill,
            slot: Slot::Pull(index),
        });
    }
}

pub fn paint(
    painter: &egui::Painter,
    right_edge: f32,
    left_edge: f32,
    mid: f32,
    labels: &[&RefLabel],
    badges: &RemoteBadges,
    lane: Color32,
) -> Vec<Placed> {
    let font = FontId::proportional(12.0);
    let mut right = right_edge;
    let mut placed = Vec::new();
    for (i, label) in labels.iter().enumerate() {
        let more_chip = if i + 1 < labels.len() {
            MORE_CHIP_W
        } else {
            0.0
        };
        let limit = right - left_edge - more_chip;
        let laid = lay(painter, label, badges, limit);
        let text_room = (limit - (laid.width - laid.galley.size().x)).min(LABEL_MAX_TEXT_W);
        let remaining = labels.len() - i;
        let fits = text_room >= LABEL_MIN_TEXT_W && (i == 0 || right - left_edge > MORE_CHIP_W);
        if !fits || right - laid.width < left_edge {
            let g = painter.layout_no_wrap(format!("+{remaining}"), font, theme::text_muted());
            let rect = Rect::from_min_size(
                pos2(right - g.size().x - 12.0, mid - LABEL_H / 2.0),
                vec2(g.size().x + 12.0, LABEL_H),
            );
            painter.rect_filled(rect, CornerRadius::same(5), theme::with_alpha(lane, 0x26));
            painter.galley(
                pos2(rect.left() + 6.0, mid - g.size().y / 2.0),
                g,
                theme::text_muted(),
            );
            placed.push(Placed {
                rect,
                slot: Slot::More,
            });
            break;
        }
        let width = laid.width;
        draw(painter, laid, right, mid, lane, i, &mut placed);
        right -= width + 4.0;
    }
    placed
}

const INLINE_H: f32 = 18.0;
const INLINE_PAD: f32 = 7.0;
const INLINE_GAP: f32 = 6.0;
const INLINE_MAX_TEXT_W: f32 = 150.0;
const INLINE_MIN_TEXT_W: f32 = 36.0;

/// Draws small outlined chips left to right from `left`, stopping before `right` with a "+N"
/// chip for the rest. Returns the chips and the x where the commit title can start.
pub fn paint_inline(
    painter: &egui::Painter,
    left: f32,
    right: f32,
    mid: f32,
    labels: &[&RefLabel],
    accent: Color32,
) -> (Vec<Placed>, f32) {
    let font = FontId::monospace(11.0);
    let mut placed = Vec::new();
    let mut x = left;
    for (i, label) in labels.iter().enumerate() {
        let more = if i + 1 < labels.len() {
            MORE_CHIP_W
        } else {
            0.0
        };
        let room = right - x - INLINE_PAD * 2.0 - more;
        if i > 0 && room < INLINE_MIN_TEXT_W {
            let g =
                painter.layout_no_wrap(format!("+{}", labels.len() - i), font, theme::text_muted());
            let rect = Rect::from_min_size(
                pos2(x, mid - INLINE_H / 2.0),
                vec2(g.size().x + INLINE_PAD * 2.0, INLINE_H),
            );
            painter.rect_stroke(
                rect,
                CornerRadius::same(5),
                Stroke::new(1.0, theme::border()),
                egui::StrokeKind::Inside,
            );
            painter.galley(
                pos2(rect.left() + INLINE_PAD, mid - g.size().y / 2.0),
                g,
                theme::text_muted(),
            );
            x = rect.right() + INLINE_GAP;
            placed.push(Placed {
                rect,
                slot: Slot::More,
            });
            break;
        }
        let head = label.kind == RefKind::Local && label.is_head;
        let (ink, edge) = if head {
            (accent, theme::with_alpha(accent, 0x73))
        } else {
            (theme::text_muted(), theme::border())
        };
        let galley = crate::paint::truncated(
            painter,
            label.name.clone(),
            font.clone(),
            ink,
            room.clamp(INLINE_MIN_TEXT_W, INLINE_MAX_TEXT_W),
        );
        let rect = Rect::from_min_size(
            pos2(x, mid - INLINE_H / 2.0),
            vec2(galley.size().x + INLINE_PAD * 2.0, INLINE_H),
        );
        painter.rect_stroke(
            rect,
            CornerRadius::same(5),
            Stroke::new(1.0, edge),
            egui::StrokeKind::Inside,
        );
        painter.galley(
            pos2(rect.left() + INLINE_PAD, mid - galley.size().y / 2.0),
            galley,
            ink,
        );
        x = rect.right() + INLINE_GAP;
        placed.push(Placed {
            rect,
            slot: Slot::Label(i),
        });
    }
    (placed, x)
}

const STACK_GAP: f32 = 3.0;

pub fn stack_step() -> f32 {
    LABEL_H + STACK_GAP
}

pub fn paint_stack(
    painter: &egui::Painter,
    right_edge: f32,
    left_edge: f32,
    mid: f32,
    labels: &[&RefLabel],
    badges: &RemoteBadges,
    lane: Color32,
) -> Vec<Placed> {
    let mut placed = Vec::new();
    for (i, label) in labels.iter().enumerate() {
        let row_mid = mid + i as f32 * stack_step();
        let laid = lay(painter, label, badges, right_edge - left_edge);
        let backing = Rect::from_min_size(
            pos2(right_edge - laid.width, row_mid - LABEL_H / 2.0),
            vec2(laid.width, LABEL_H),
        )
        .expand(1.5);
        painter.rect_filled(backing, CornerRadius::same(6), theme::bg());
        draw(painter, laid, right_edge, row_mid, lane, i, &mut placed);
    }
    placed
}

pub fn paint_ghost(ctx: &egui::Context, label: &RefLabel, at: Pos2, lane: Color32) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        Id::new("ref-label-ghost"),
    ));
    let galley = crate::paint::truncated(
        &painter,
        label_text(label),
        FontId::proportional(12.0),
        Color32::PLACEHOLDER,
        LABEL_MAX_TEXT_W,
    );
    let rect = Rect::from_min_size(
        at + vec2(12.0, -LABEL_H / 2.0),
        vec2(galley.size().x + 16.0, LABEL_H),
    );
    let (fill, stroke, ink) = label_style(label, lane);
    painter.rect(
        rect.translate(vec2(0.0, 2.0)),
        CornerRadius::same(5),
        theme::backdrop(),
        Stroke::NONE,
        egui::StrokeKind::Inside,
    );
    painter.rect(
        rect,
        CornerRadius::same(5),
        theme::popup().lerp_to_gamma(fill, 0.6),
        stroke,
        egui::StrokeKind::Inside,
    );
    painter.galley(
        pos2(rect.left() + 8.0, rect.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
}

#[derive(Clone, Copy, Default)]
pub struct Forced<'a> {
    pub list: bool,
    pub menu: Option<&'a str>,
}

pub fn interact(
    ui: &Ui,
    row: usize,
    labels: &[&RefLabel],
    placed: &[Placed],
    lane: Color32,
    forced: Forced<'_>,
    menu: &mut dyn FnMut(&mut Ui, MenuFor<'_>),
) -> Vec<LabelEvent> {
    let mut events = Vec::new();
    for spot in placed {
        match spot.slot {
            Slot::Label(i) => {
                let label = labels[i];
                let response = ui
                    .interact(
                        spot.rect,
                        Id::new(("ref-label", row, i)),
                        Sense::click_and_drag(),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand);
                let response = response.on_hover_text(&label.name);
                if response.double_clicked() {
                    events.push(LabelEvent::Activate(label.clone()));
                } else if response.clicked() {
                    events.push(LabelEvent::Select);
                }
                if response.drag_started() {
                    events.push(LabelEvent::DragStart(label.clone()));
                }
                if response.drag_stopped() {
                    events.push(LabelEvent::DragStop);
                }
                if forced.menu == Some(label.name.as_str()) {
                    egui::Popup::context_menu(&response)
                        .open(true)
                        .at_position(response.rect.left_bottom())
                        .show(|ui| menu(ui, MenuFor::Ref(label)));
                } else {
                    crate::menu::context_menu(&response, |ui| menu(ui, MenuFor::Ref(label)));
                }
            }
            Slot::Pull(i) => {
                let Some(pull) = &labels[i].pull else {
                    continue;
                };
                let response = ui
                    .interact(spot.rect, Id::new(("ref-pull", row, i)), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_ui(|ui| crate::pulls_ui::tooltip(ui, pull));
                if response.clicked() {
                    events.push(LabelEvent::OpenUrl(pull.url.clone()));
                }
            }
            Slot::More => {
                let names: Vec<&str> = labels.iter().map(|l| l.name.as_str()).collect();
                let response = ui
                    .interact(spot.rect, list_button_id(row), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text(names.join("\n"));
                let mut popup = egui::Popup::menu(&response);
                if forced.list {
                    popup = popup.open(true);
                }
                popup.show(|ui| ref_list(ui, labels, lane, menu));
            }
        }
    }
    events
}

pub fn list_button_id(row: usize) -> Id {
    Id::new(("ref-label-more", row))
}

fn ref_list(
    ui: &mut Ui,
    labels: &[&RefLabel],
    lane: Color32,
    menu: &mut dyn FnMut(&mut Ui, MenuFor<'_>),
) {
    ui.set_min_width(LIST_W);
    ui.set_max_width(LIST_W);
    ui.spacing_mut().item_spacing.y = 0.0;
    crate::menu::heading(ui, &format!("{} refs on this commit", labels.len()));
    for label in labels {
        let (rect, response) = ui.allocate_exact_size(vec2(LIST_W, LIST_ROW_H), Sense::click());
        let painter = ui.painter_at(rect.expand(1.0));
        if response.hovered() {
            painter.rect_filled(rect, 6.0, theme::menu_hover());
        }
        let dot = pos2(rect.left() + 16.0, rect.center().y);
        match label.kind {
            RefKind::Local => {
                painter.circle_filled(dot, 4.0, lane);
            }
            RefKind::Remote => {
                painter.circle_stroke(dot, 3.5, Stroke::new(1.5, lane));
            }
            RefKind::Tag => {
                painter.rect_stroke(
                    Rect::from_center_size(dot, vec2(8.0, 8.0)),
                    CornerRadius::same(2),
                    Stroke::new(1.5, theme::text_muted()),
                    egui::StrokeKind::Inside,
                );
            }
        }
        let name = crate::paint::truncated(
            &painter,
            label_text(label),
            FontId::proportional(13.0),
            theme::text(),
            LIST_W - 110.0,
        );
        painter.galley(
            pos2(rect.left() + 30.0, rect.center().y - name.size().y / 2.0),
            name,
            theme::text(),
        );
        let kind = painter.text(
            pos2(rect.right() - 26.0, rect.center().y),
            Align2::RIGHT_CENTER,
            kind_name(label),
            FontId::proportional(11.0),
            theme::text_faint(),
        );
        if let Some(pull) = &label.pull {
            let w = crate::pulls_ui::pill_width(&painter, pull);
            let pill = Rect::from_min_size(
                pos2(
                    kind.left() - 8.0 - w,
                    rect.center().y - crate::pulls_ui::PILL_H / 2.0,
                ),
                vec2(w, crate::pulls_ui::PILL_H),
            );
            crate::pulls_ui::paint_pill(&painter, pill, pull, false);
        }
        let chevron = pos2(rect.right() - 14.0, rect.center().y);
        let ink = Stroke::new(1.3, theme::text_muted());
        painter.line_segment([chevron + vec2(-2.0, -4.0), chevron + vec2(2.0, 0.0)], ink);
        painter.line_segment([chevron + vec2(2.0, 0.0), chevron + vec2(-2.0, 4.0)], ink);
        SubMenu::new().show(ui, &response, |ui| menu(ui, MenuFor::Ref(label)));
    }
}

fn label_text(label: &RefLabel) -> String {
    match label.kind {
        RefKind::Local if label.is_head => format!("✔ {}", label.name),
        _ => label.name.clone(),
    }
}

fn kind_name(label: &RefLabel) -> &'static str {
    match label.kind {
        RefKind::Local if label.is_head => "HEAD",
        RefKind::Local => "local",
        RefKind::Remote => "remote",
        RefKind::Tag => "tag",
    }
}

pub fn label_style(label: &RefLabel, lane: Color32) -> (Color32, Stroke, Color32) {
    match label.kind {
        RefKind::Local if label.is_head => (
            theme::with_alpha(lane, 0x40),
            Stroke::new(1.0, theme::with_alpha(lane, 0xcc)),
            lane,
        ),
        RefKind::Local => (
            theme::with_alpha(lane, 0x26),
            Stroke::new(1.0, theme::with_alpha(lane, 0x99)),
            theme::text_strong(),
        ),
        RefKind::Remote => (
            Color32::TRANSPARENT,
            Stroke::new(1.0, theme::with_alpha(lane, 0x99)),
            theme::text_strong(),
        ),
        RefKind::Tag => (
            theme::tone(Color32::from_rgb(0x1f, 0x23, 0x2b)),
            Stroke::new(1.0, theme::tone(Color32::from_rgb(0x4a, 0x51, 0x60))),
            theme::text_label(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use egui::{self, Event, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
    use kelp_core::refs::{RefKind, RefLabel};

    use super::*;

    fn label(name: &str, kind: RefKind, is_head: bool) -> RefLabel {
        RefLabel {
            name: name.into(),
            kind,
            target: gix::ObjectId::null(gix::hash::Kind::Sha1),
            row: Some(0),
            is_head,
            has_remote: false,
            hidden: false,
            pull: None,
        }
    }

    #[test]
    fn remote_labels_swap_the_remote_name_for_its_owner_badge() {
        let texture = egui::TextureId::Managed(7);
        let badges = RemoteBadges::from([("origin".to_string(), texture)]);
        let remote = label("origin/feat/a", RefKind::Remote, false);
        assert_eq!(remote_badge(&remote, &badges), Some((texture, "feat/a")));
        let other = label("fork/feat/a", RefKind::Remote, false);
        assert_eq!(remote_badge(&other, &badges), None);
        let local = label("origin/feat/a", RefKind::Local, false);
        assert_eq!(remote_badge(&local, &badges), None);
    }

    struct Harness {
        ctx: egui::Context,
        time: f64,
        labels: Vec<RefLabel>,
        placed: Vec<Placed>,
        events: Vec<&'static str>,
    }

    impl Harness {
        fn new(labels: Vec<RefLabel>, placed: Vec<Placed>) -> Self {
            Self {
                ctx: egui::Context::default(),
                time: 0.0,
                labels,
                placed,
                events: Vec::new(),
            }
        }

        fn frame(&mut self, events: Vec<Event>) {
            self.time += 0.05;
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 300.0))),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let labels: Vec<&RefLabel> = self.labels.iter().collect();
            let placed = self.placed.clone();
            let mut seen = Vec::new();
            let _ = self.ctx.run_ui(input, |ui| {
                let lane = theme::accent();
                for event in interact(
                    ui,
                    0,
                    &labels,
                    &placed,
                    lane,
                    Forced::default(),
                    &mut |_, _| {},
                ) {
                    seen.push(match event {
                        LabelEvent::Select => "select",
                        LabelEvent::Activate(_) => "activate",
                        LabelEvent::OpenUrl(_) => "open-url",
                        LabelEvent::DragStart(_) => "drag-start",
                        LabelEvent::DragStop => "drag-stop",
                    });
                }
            });
            self.events.extend(seen);
        }

        fn click(&mut self, at: Pos2) {
            let press = |pressed| Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            self.frame(vec![Event::PointerMoved(at)]);
            self.frame(vec![press(true)]);
            self.frame(vec![press(false)]);
        }
    }

    fn spot(slot: Slot) -> Placed {
        Placed {
            rect: Rect::from_min_size(pos2(20.0, 20.0), vec2(90.0, 22.0)),
            slot,
        }
    }

    #[test]
    fn clicking_a_label_selects_its_commit() {
        let mut h = Harness::new(
            vec![label("feat/a", RefKind::Local, false)],
            vec![spot(Slot::Label(0))],
        );
        h.frame(vec![]);
        h.click(pos2(40.0, 30.0));
        assert_eq!(h.events, ["select"]);
    }

    #[test]
    fn double_clicking_a_label_activates_it() {
        let mut h = Harness::new(
            vec![label("feat/a", RefKind::Local, false)],
            vec![spot(Slot::Label(0))],
        );
        h.frame(vec![]);
        h.click(pos2(40.0, 30.0));
        h.click(pos2(40.0, 30.0));
        assert!(h.events.contains(&"activate"), "{:?}", h.events);
    }

    #[test]
    fn clicking_the_pull_pill_opens_the_pull_request_not_the_commit() {
        let mut feat = label("feat/a", RefKind::Local, false);
        feat.pull = Some(kelp_core::pulls::Pull {
            number: 42,
            title: "Add a".into(),
            head: "feat/a".into(),
            head_owner: None,
            state: kelp_core::pulls::State::Open,
            url: "https://github.com/o/r/pull/42".into(),
            review: None,
            checks: None,
        });
        let pill = Placed {
            rect: Rect::from_min_size(pos2(80.0, 23.0), vec2(26.0, 16.0)),
            slot: Slot::Pull(0),
        };
        let mut h = Harness::new(vec![feat], vec![spot(Slot::Label(0)), pill]);
        h.frame(vec![]);
        h.click(pos2(92.0, 31.0));
        assert_eq!(h.events, ["open-url"]);
        h.events.clear();
        h.click(pos2(40.0, 30.0));
        assert_eq!(h.events, ["select"]);
    }

    #[test]
    fn clicking_the_more_chip_opens_the_ref_list() {
        let mut h = Harness::new(
            vec![
                label("main", RefKind::Local, true),
                label("origin/main", RefKind::Remote, false),
            ],
            vec![spot(Slot::More)],
        );
        h.frame(vec![]);
        assert!(!egui::Popup::is_any_open(&h.ctx));
        h.click(pos2(40.0, 30.0));
        h.frame(vec![]);
        assert!(egui::Popup::is_any_open(&h.ctx));
    }
}
