use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Pos2, Rect, Sense, Stroke, Ui,
    containers::menu::SubMenu, pos2, vec2,
};
use kelp_core::ops::Op;
use kelp_core::refs::{RefKind, RefLabel};

use crate::commands::Command;
use crate::repo_view::Selection;
use crate::theme;

const LABEL_H: f32 = 22.0;
const LABEL_MAX_TEXT_W: f32 = 114.0;
const LIST_ROW_H: f32 = 30.0;
const LIST_W: f32 = 260.0;
const PILL_GAP: f32 = 6.0;

pub enum MenuFor<'a> {
    Commit(Selection, &'a str),
    Ref(&'a RefLabel),
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

#[derive(Clone, Debug)]
pub struct DropTarget {
    pub row: usize,
    pub commit: String,
    pub label: Option<RefLabel>,
    pub is_head: bool,
}

#[derive(Clone, Debug)]
pub struct DropPlan {
    pub source: RefLabel,
    pub target: DropTarget,
    pub pos: Pos2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropChoice {
    Merge { source: String, into: String },
    CheckoutAndMerge { branch: String, source: String },
    Rebase { current: String, onto: String },
    Reset { current: String, commit: String },
}

pub enum LabelEvent {
    Select,
    Command(Command),
    DragStart(RefLabel),
    DragStop,
}

pub fn target_name(target: &DropTarget) -> String {
    target
        .label
        .as_ref()
        .map_or_else(|| short(&target.commit).to_string(), |l| l.name.clone())
}

pub fn drop_choices(
    source: &RefLabel,
    target: &DropTarget,
    current: Option<&str>,
) -> Vec<DropChoice> {
    if target
        .label
        .as_ref()
        .is_some_and(|l| l.name == source.name && l.kind == source.kind)
    {
        return Vec::new();
    }
    let mut choices = Vec::new();
    let target_branch = target
        .label
        .as_ref()
        .filter(|l| l.kind == RefKind::Local)
        .map(|l| l.name.as_str());
    match (target_branch, current) {
        (Some(branch), Some(current)) if branch == current => choices.push(DropChoice::Merge {
            source: source.name.clone(),
            into: branch.to_string(),
        }),
        (Some(branch), _) => choices.push(DropChoice::CheckoutAndMerge {
            branch: branch.to_string(),
            source: source.name.clone(),
        }),
        (None, Some(current)) if target.is_head && source.name != current => {
            choices.push(DropChoice::Merge {
                source: source.name.clone(),
                into: current.to_string(),
            })
        }
        _ => {}
    }
    if let Some(current) = current
        && !target.is_head
    {
        let onto = target_name(target);
        if onto != current {
            choices.push(DropChoice::Rebase {
                current: current.to_string(),
                onto,
            });
        }
        choices.push(DropChoice::Reset {
            current: current.to_string(),
            commit: target.commit.clone(),
        });
    }
    choices
}

pub fn checkout_op(label: &RefLabel) -> Option<Op> {
    match label.kind {
        RefKind::Local if !label.is_head => Some(Op::Switch(label.name.clone())),
        RefKind::Remote => Some(Op::SwitchTrack(label.name.clone())),
        _ => None,
    }
}

pub fn paint(
    painter: &egui::Painter,
    right_edge: f32,
    left_edge: f32,
    mid: f32,
    labels: &[&RefLabel],
    lane: Color32,
) -> Vec<Placed> {
    let font = FontId::proportional(12.0);
    let mut right = right_edge;
    let mut placed = Vec::new();
    for (i, label) in labels.iter().enumerate() {
        let galley = crate::graph_view::truncated(
            painter,
            label_text(label),
            font.clone(),
            Color32::PLACEHOLDER,
            LABEL_MAX_TEXT_W,
        );
        let pill_w = label.pull.as_ref().map_or(0.0, |pull| {
            crate::pulls_ui::pill_width(painter, pull) + PILL_GAP
        });
        let w = galley.size().x + 16.0 + pill_w;
        let remaining = labels.len() - i;
        if right - w < left_edge || (i > 0 && right - w - 30.0 < left_edge) {
            let g = painter.layout_no_wrap(format!("+{remaining}"), font, theme::TEXT_MUTED);
            let rect = Rect::from_min_size(
                pos2(right - g.size().x - 12.0, mid - LABEL_H / 2.0),
                vec2(g.size().x + 12.0, LABEL_H),
            );
            painter.rect_filled(rect, CornerRadius::same(5), theme::with_alpha(lane, 0x26));
            painter.galley(
                pos2(rect.left() + 6.0, mid - g.size().y / 2.0),
                g,
                theme::TEXT_MUTED,
            );
            placed.push(Placed {
                rect,
                slot: Slot::More,
            });
            break;
        }
        let rect = Rect::from_min_size(pos2(right - w, mid - LABEL_H / 2.0), vec2(w, LABEL_H));
        let (fill, stroke, ink) = label_style(label, lane);
        painter.rect(
            rect,
            CornerRadius::same(5),
            fill,
            stroke,
            egui::StrokeKind::Inside,
        );
        painter.galley(
            pos2(rect.left() + 8.0, mid - galley.size().y / 2.0),
            galley,
            ink,
        );
        placed.push(Placed {
            rect,
            slot: Slot::Label(i),
        });
        if let Some(pull) = &label.pull {
            let pill = Rect::from_min_size(
                pos2(
                    rect.right() - pill_w + PILL_GAP - 4.0,
                    mid - crate::pulls_ui::PILL_H / 2.0,
                ),
                vec2(pill_w - PILL_GAP, crate::pulls_ui::PILL_H),
            );
            crate::pulls_ui::paint_pill(painter, pill, pull, false);
            placed.push(Placed {
                rect: pill,
                slot: Slot::Pull(i),
            });
        }
        right -= w + 4.0;
    }
    placed
}

pub fn paint_ghost(ctx: &egui::Context, label: &RefLabel, at: Pos2, lane: Color32) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        Id::new("ref-label-ghost"),
    ));
    let galley = crate::graph_view::truncated(
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
        Color32::from_black_alpha(90),
        Stroke::NONE,
        egui::StrokeKind::Inside,
    );
    painter.rect(
        rect,
        CornerRadius::same(5),
        theme::POPUP.lerp_to_gamma(fill, 0.6),
        stroke,
        egui::StrokeKind::Inside,
    );
    painter.galley(
        pos2(rect.left() + 8.0, rect.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
}

pub fn interact(
    ui: &Ui,
    row: usize,
    labels: &[&RefLabel],
    placed: &[Placed],
    lane: Color32,
    force_list_open: bool,
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
                if response.double_clicked() {
                    if let Some(op) = checkout_op(label) {
                        events.push(LabelEvent::Command(Command::Run(op)));
                    }
                } else if response.clicked() {
                    events.push(LabelEvent::Select);
                }
                if response.drag_started() {
                    events.push(LabelEvent::DragStart(label.clone()));
                }
                if response.drag_stopped() {
                    events.push(LabelEvent::DragStop);
                }
                response.context_menu(|ui| menu(ui, MenuFor::Ref(label)));
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
                    events.push(LabelEvent::Command(Command::OpenUrl(pull.url.clone())));
                }
            }
            Slot::More => {
                let names: Vec<&str> = labels.iter().map(|l| l.name.as_str()).collect();
                let response = ui
                    .interact(spot.rect, list_button_id(row), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text(names.join("\n"));
                let mut popup = egui::Popup::menu(&response);
                if force_list_open {
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
    crate::menus::heading(ui, &format!("{} refs on this commit", labels.len()));
    for label in labels {
        let (rect, response) = ui.allocate_exact_size(vec2(LIST_W, LIST_ROW_H), Sense::click());
        let painter = ui.painter_at(rect.expand(1.0));
        if response.hovered() {
            painter.rect_filled(rect, 6.0, theme::MENU_HOVER);
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
                    Stroke::new(1.5, theme::TEXT_MUTED),
                    egui::StrokeKind::Inside,
                );
            }
        }
        let name = crate::graph_view::truncated(
            &painter,
            label_text(label),
            FontId::proportional(13.0),
            theme::TEXT,
            LIST_W - 110.0,
        );
        painter.galley(
            pos2(rect.left() + 30.0, rect.center().y - name.size().y / 2.0),
            name,
            theme::TEXT,
        );
        let kind = painter.text(
            pos2(rect.right() - 26.0, rect.center().y),
            Align2::RIGHT_CENTER,
            kind_name(label),
            FontId::proportional(11.0),
            theme::TEXT_FAINT,
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
        let ink = Stroke::new(1.3, theme::TEXT_MUTED);
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
            theme::TEXT_STRONG,
        ),
        RefKind::Remote => (
            Color32::TRANSPARENT,
            Stroke::new(1.0, theme::with_alpha(lane, 0x99)),
            theme::TEXT_STRONG,
        ),
        RefKind::Tag => (
            Color32::from_rgb(0x1f, 0x23, 0x2b),
            Stroke::new(1.0, Color32::from_rgb(0x4a, 0x51, 0x60)),
            Color32::from_rgb(0xc9, 0xcc, 0xd2),
        ),
    }
}

fn short(commit: &str) -> &str {
    &commit[..7.min(commit.len())]
}

#[cfg(test)]
mod tests {
    use eframe::egui::{self, Event, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
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

    fn target(label: Option<RefLabel>, is_head: bool) -> DropTarget {
        DropTarget {
            row: 3,
            commit: "abcdef1234567890".into(),
            label,
            is_head,
        }
    }

    #[test]
    fn dropping_on_the_checked_out_branch_merges_into_it() {
        let src = label("feat/a", RefKind::Local, false);
        let main = label("main", RefKind::Local, true);
        let choices = drop_choices(&src, &target(Some(main), true), Some("main"));
        assert_eq!(
            choices,
            vec![DropChoice::Merge {
                source: "feat/a".into(),
                into: "main".into()
            }]
        );
    }

    #[test]
    fn dropping_on_another_branch_checks_it_out_first() {
        let src = label("feat/a", RefKind::Local, false);
        let release = label("release", RefKind::Local, false);
        let choices = drop_choices(&src, &target(Some(release), false), Some("main"));
        assert_eq!(
            choices[0],
            DropChoice::CheckoutAndMerge {
                branch: "release".into(),
                source: "feat/a".into()
            }
        );
        assert_eq!(
            choices[1],
            DropChoice::Rebase {
                current: "main".into(),
                onto: "release".into()
            }
        );
        assert!(matches!(choices[2], DropChoice::Reset { .. }));
    }

    #[test]
    fn dropping_on_a_plain_commit_rebases_or_resets_by_hash() {
        let src = label("main", RefKind::Local, true);
        let choices = drop_choices(&src, &target(None, false), Some("main"));
        assert_eq!(
            choices,
            vec![
                DropChoice::Rebase {
                    current: "main".into(),
                    onto: "abcdef1".into()
                },
                DropChoice::Reset {
                    current: "main".into(),
                    commit: "abcdef1234567890".into()
                }
            ]
        );
    }

    #[test]
    fn dropping_on_head_merges_and_nothing_else() {
        let src = label("origin/feat/a", RefKind::Remote, false);
        let choices = drop_choices(&src, &target(None, true), Some("main"));
        assert_eq!(
            choices,
            vec![DropChoice::Merge {
                source: "origin/feat/a".into(),
                into: "main".into()
            }]
        );
    }

    #[test]
    fn dropping_a_label_on_itself_does_nothing() {
        let src = label("feat/a", RefKind::Local, false);
        let same = label("feat/a", RefKind::Local, false);
        assert!(drop_choices(&src, &target(Some(same), false), Some("main")).is_empty());
    }

    #[test]
    fn detached_head_only_offers_checkout_and_merge() {
        let src = label("feat/a", RefKind::Local, false);
        let release = label("release", RefKind::Local, false);
        assert_eq!(
            drop_choices(&src, &target(Some(release), false), None),
            vec![DropChoice::CheckoutAndMerge {
                branch: "release".into(),
                source: "feat/a".into()
            }]
        );
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
                let lane = theme::ACCENT;
                for event in interact(ui, 0, &labels, &placed, lane, false, &mut |_, _| {}) {
                    seen.push(match event {
                        LabelEvent::Select => "select",
                        LabelEvent::Command(Command::Run(Op::Switch(_))) => "switch",
                        LabelEvent::Command(Command::OpenUrl(_)) => "open-url",
                        LabelEvent::Command(_) => "command",
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
    fn double_clicking_a_local_label_checks_it_out() {
        let mut h = Harness::new(
            vec![label("feat/a", RefKind::Local, false)],
            vec![spot(Slot::Label(0))],
        );
        h.frame(vec![]);
        h.click(pos2(40.0, 30.0));
        h.click(pos2(40.0, 30.0));
        assert!(h.events.contains(&"switch"), "{:?}", h.events);
    }

    #[test]
    fn double_clicking_the_head_label_does_not_check_out() {
        let mut h = Harness::new(
            vec![label("main", RefKind::Local, true)],
            vec![spot(Slot::Label(0))],
        );
        h.frame(vec![]);
        h.click(pos2(40.0, 30.0));
        h.click(pos2(40.0, 30.0));
        assert!(!h.events.contains(&"switch"));
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
