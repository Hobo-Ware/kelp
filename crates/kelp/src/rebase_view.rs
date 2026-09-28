use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Key, Margin, Rect, RichText, Sense,
    Stroke, Ui, pos2, vec2,
};
use gix::ObjectId;
use kelp_core::commit;
use kelp_core::history::History;
use kelp_core::rebase::{self, Action, Group, Outcome, Plan, Step};

use crate::icons::Icon;
use crate::{menus, theme, widgets};

const ROW_H: f32 = 46.0;
const PILL_W: f32 = 84.0;
const FOLD_INDENT: f32 = 26.0;
const LIST_W: f32 = 880.0;
const PILL_X: f32 = 44.0;
const EDITOR_INDENT: f32 = PILL_X + PILL_W + 14.0;

enum Loaded {
    Loading(Receiver<Result<(Plan, String), String>>),
    Ready { plan: Plan, base_summary: String },
    Failed(String),
}

pub struct RebaseView {
    loaded: Loaded,
    original_order: Vec<ObjectId>,
    combined: HashMap<ObjectId, String>,
    preset: String,
}

pub struct Start {
    plan: Plan,
    combined: HashMap<ObjectId, String>,
}

pub enum Event {
    None,
    Close,
    Start(Start),
}

impl Start {
    pub fn run(&self, dir: &Path) -> anyhow::Result<Outcome> {
        let combined = |group: &Group| {
            self.combined
                .get(&self.plan.steps[group.leader].id)
                .cloned()
                .unwrap_or_else(|| rebase::default_combined(&self.plan.steps, group))
        };
        rebase::run(dir, &self.plan, &combined)
    }
}

impl RebaseView {
    pub fn open(ctx: &egui::Context, dir: PathBuf, base: String) -> Self {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = rebase::load(&dir, &base)
                .map(|plan| {
                    let summary =
                        kelp_core::git_cli::run(&dir, &["log", "-1", "--format=%s", &base])
                            .map(|s| s.trim().to_string())
                            .unwrap_or_default();
                    (plan, summary)
                })
                .map_err(|e| format!("{e:#}"));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        Self {
            loaded: Loaded::Loading(rx),
            original_order: Vec::new(),
            combined: HashMap::new(),
            preset: String::new(),
        }
    }

    pub fn with_actions(mut self, letters: &str) -> Self {
        self.preset = letters.to_string();
        self
    }

    pub fn ui(&mut self, ui: &mut Ui) -> Event {
        self.poll();
        let mut event = Event::None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(32, 28))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 12.0;
                        let width = ui.available_width().min(LIST_W);
                        ui.set_max_width(width);
                        event = self.page(ui);
                    });
            });
        event
    }

    fn poll(&mut self) {
        let Loaded::Loading(rx) = &self.loaded else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok((mut plan, base_summary))) => {
                self.original_order = plan.steps.iter().map(|s| s.id).collect();
                let editable = plan.steps.iter_mut().filter(|s| !s.merge);
                for (step, letter) in editable.zip(self.preset.chars()) {
                    if let Some(action) = Action::ALL
                        .into_iter()
                        .find(|a| action_key(*a).eq_ignore_ascii_case(&letter.to_string()))
                    {
                        step.action = action;
                    }
                }
                self.loaded = Loaded::Ready { plan, base_summary };
            }
            Ok(Err(e)) => self.loaded = Loaded::Failed(e),
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.loaded = Loaded::Failed("Could not read the commits.".into())
            }
        }
    }

    fn page(&mut self, ui: &mut Ui) -> Event {
        let mut event = Event::None;
        let subtitle = match &self.loaded {
            Loaded::Ready { plan, base_summary } => {
                format!("onto {}  {}", plan.base.to_hex_with_len(7), base_summary)
            }
            _ => String::new(),
        };
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Interactive rebase")
                    .size(18.0)
                    .family(theme::semibold())
                    .color(theme::TEXT_STRONG),
            );
            ui.add_space(8.0);
            ui.label(RichText::new(subtitle).color(theme::TEXT_FAINT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::close_button(ui, "Close (Esc)") {
                    event = Event::Close;
                }
            });
        });
        let Loaded::Ready { plan, .. } = &mut self.loaded else {
            let text = match &self.loaded {
                Loaded::Failed(e) => e.as_str(),
                _ => "Reading commits…",
            };
            ui.add_space(24.0);
            ui.label(RichText::new(text).color(theme::TEXT_MUTED));
            return event;
        };
        let merges = plan.has_merges();
        let help = if merges {
            "Oldest first. This range has merge commits, so the order and the merges stay as they \
             are: Pick, Reword, Edit or Drop the other commits. Edit stops there so you can amend."
        } else {
            "Oldest first, applied top to bottom. Drag to reorder. Squash and Fixup fold a commit \
             into the one above it; Squash keeps both messages. Edit stops there so you can amend."
        };
        ui.label(RichText::new(help).size(12.0).color(theme::TEXT_FAINT));
        list(ui, &mut plan.steps, &mut self.combined, merges);
        let problem = rebase::problem(&plan.steps);
        let changed = has_changes(&plan.steps, &self.original_order, &self.combined);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            match &problem {
                Some(problem) => {
                    ui.label(RichText::new(problem).size(12.0).color(theme::DELETED));
                }
                None => {
                    ui.label(
                        RichText::new(summary(&plan.steps, &self.original_order))
                            .size(12.0)
                            .color(theme::TEXT_MUTED),
                    );
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let start = ui.add_enabled(
                    problem.is_none() && changed,
                    egui::Button::new(
                        RichText::new("Start rebase")
                            .size(12.0)
                            .family(theme::semibold())
                            .color(Color32::from_rgb(0x10, 0x13, 0x1a)),
                    )
                    .fill(theme::ACCENT)
                    .corner_radius(5)
                    .min_size(vec2(0.0, 30.0)),
                );
                if start.clicked() {
                    event = Event::Start(Start {
                        plan: plan.clone(),
                        combined: self.combined.clone(),
                    });
                }
                let cancel = egui::Button::new(RichText::new("Cancel").size(12.0))
                    .corner_radius(5)
                    .min_size(vec2(0.0, 30.0));
                if ui.add(cancel).clicked() {
                    event = Event::Close;
                }
            });
        });
        event
    }
}

fn list(
    ui: &mut Ui,
    steps: &mut Vec<Step>,
    combined: &mut HashMap<ObjectId, String>,
    keep_order: bool,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let groups = rebase::groups(steps);
    let group_end: HashMap<usize, Group> = groups
        .iter()
        .filter(|g| g.squashes(steps))
        .map(|g| (*g.members.last().unwrap_or(&g.leader), g.clone()))
        .collect();
    let folded_leaders: Vec<usize> = group_end.values().map(|g| g.leader).collect();
    let mut centers = Vec::with_capacity(steps.len());
    let mut dragged = None;
    let mut new_action = None;
    let typing = ui.ctx().egui_wants_keyboard_input();
    egui::Frame::new()
        .fill(theme::PANEL)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(0, 4))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for i in 0..steps.len() {
                let step = &steps[i];
                let (rect, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
                let sense = if keep_order {
                    Sense::hover()
                } else {
                    Sense::click_and_drag()
                };
                let response = ui.interact(rect, egui::Id::new(("rebase-row", step.id)), sense);
                centers.push(rect.center().y);
                if response.dragged()
                    && let Some(pointer) = response.interact_pointer_pos()
                {
                    dragged = Some((i, pointer.y));
                    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                } else if response.hovered() && !keep_order {
                    ui.ctx().set_cursor_icon(CursorIcon::Grab);
                }
                if response.hovered()
                    && !typing
                    && !step.merge
                    && let Some(action) = key_action(ui)
                    && (!keep_order || action.allowed_with_merges())
                {
                    new_action = Some((i, action));
                }
                paint_row(
                    ui,
                    rect,
                    step,
                    response.hovered() || response.dragged(),
                    !keep_order,
                    now,
                );
                let pill = pill_rect(rect, step.action);
                let pill_sense = if step.merge {
                    Sense::hover()
                } else {
                    Sense::click()
                };
                let pill_response =
                    ui.interact(pill, egui::Id::new(("rebase-pill", step.id)), pill_sense);
                let pill_response = if step.merge {
                    pill_response.on_hover_text("Merge commits keep their place")
                } else {
                    pill_response.on_hover_cursor(CursorIcon::PointingHand)
                };
                egui::Popup::menu(&pill_response).show(|ui| {
                    ui.set_min_width(190.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for action in Action::ALL
                        .into_iter()
                        .filter(|a| !keep_order || a.allowed_with_merges())
                    {
                        if menus::row(
                            ui,
                            Some(action_icon(action)),
                            action.label(),
                            Some(action_key(action)),
                            action == Action::Drop,
                        ) {
                            new_action = Some((i, action));
                            ui.close();
                        }
                    }
                });
                if step.action == Action::Reword && !folded_leaders.contains(&i) {
                    let step = &mut steps[i];
                    editor(
                        ui,
                        egui::Id::new(("reword", step.id)),
                        &mut step.message,
                        "New message",
                    );
                }
                if let Some(group) = group_end.get(&i) {
                    let leader = steps[group.leader].id;
                    let text = combined
                        .entry(leader)
                        .or_insert_with(|| rebase::default_combined(steps, group));
                    editor(
                        ui,
                        egui::Id::new(("combined", leader)),
                        text,
                        "Message for the squashed commit",
                    );
                }
            }
        });
    if let Some((i, action)) = new_action {
        let regroups = |a: Action| a.joins_previous() || a == Action::Drop;
        if regroups(steps[i].action) || regroups(action) {
            combined.clear();
        }
        steps[i].action = action;
    }
    if let Some((from, y)) = dragged {
        let to = drop_index(&centers, from, y);
        if to != from {
            let step = steps.remove(from);
            steps.insert(to, step);
            combined.clear();
        }
    }
}

fn paint_row(ui: &Ui, rect: Rect, step: &Step, hovered: bool, grip: bool, now: i64) {
    let painter = ui.painter_at(rect);
    let folded = step.action.joins_previous();
    let inner = rect.shrink2(vec2(8.0, 3.0));
    if hovered {
        painter.rect_filled(inner, 6.0, theme::CONTROL);
    }
    let mut x = rect.left() + 22.0;
    if grip {
        for dy in [-5.0, 0.0, 5.0] {
            for dx in [0.0, 5.0] {
                painter.circle_filled(pos2(x + dx, rect.center().y + dy), 1.3, theme::TEXT_FAINT);
            }
        }
    }
    x += 22.0;
    if folded {
        let stroke = Stroke::new(1.5, theme::with_alpha(action_color(step.action), 0xaa));
        painter.line_segment(
            [
                pos2(x + 6.0, rect.top() - 4.0),
                pos2(x + 6.0, rect.center().y),
            ],
            stroke,
        );
        painter.line_segment(
            [
                pos2(x + 6.0, rect.center().y),
                pos2(x + FOLD_INDENT - 6.0, rect.center().y),
            ],
            stroke,
        );
    }
    let pill = pill_rect(rect, step.action);
    let color = if step.merge {
        theme::TEXT_FAINT
    } else {
        action_color(step.action)
    };
    let pill_label = if step.merge {
        "Merge"
    } else {
        step.action.label()
    };
    painter.rect(
        pill,
        CornerRadius::same(11),
        theme::with_alpha(color, 0x2a),
        Stroke::new(1.0, theme::with_alpha(color, 0x99)),
        egui::StrokeKind::Inside,
    );
    let label_offset = if step.merge { 0.0 } else { 6.0 };
    painter.text(
        pill.center() - vec2(label_offset, 0.0),
        Align2::CENTER_CENTER,
        pill_label,
        FontId::proportional(12.0),
        color,
    );
    if !step.merge {
        let chevron = pill.right_center() - vec2(12.0, 0.0);
        painter.line_segment(
            [chevron + vec2(-3.0, -1.5), chevron + vec2(0.0, 1.5)],
            Stroke::new(1.2, color),
        );
        painter.line_segment(
            [chevron + vec2(0.0, 1.5), chevron + vec2(3.0, -1.5)],
            Stroke::new(1.2, color),
        );
    }
    let text_x = pill.right() + 14.0;
    painter.text(
        pos2(text_x, rect.center().y),
        Align2::LEFT_CENTER,
        step.id.to_hex_with_len(7).to_string(),
        FontId::monospace(12.0),
        theme::TEXT_FAINT,
    );
    let meta = format!(
        "{}  ·  {}",
        step.author,
        commit::relative_time(step.time, now)
    );
    let meta_galley = painter.layout_no_wrap(meta, FontId::proportional(12.0), theme::TEXT_FAINT);
    let meta_w = meta_galley.size().x;
    painter.galley(
        pos2(
            rect.right() - 20.0 - meta_w,
            rect.center().y - meta_galley.size().y / 2.0,
        ),
        meta_galley,
        theme::TEXT_FAINT,
    );
    let dropped = step.action == Action::Drop;
    let title = if step.action == Action::Reword {
        step.message.lines().next().unwrap_or_default()
    } else {
        step.summary()
    };
    let title_x = text_x + 70.0;
    let max_w = (rect.right() - 40.0 - meta_w - title_x).max(40.0);
    let mut job = egui::text::LayoutJob::simple_singleline(
        title.to_string(),
        FontId::proportional(13.5),
        if dropped {
            theme::TEXT_FAINT
        } else {
            theme::TEXT_STRONG
        },
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w);
    if dropped && let Some(section) = job.sections.first_mut() {
        section.format.strikethrough = Stroke::new(1.0, theme::TEXT_FAINT);
    }
    let galley = painter.layout_job(job);
    painter.galley(
        pos2(title_x, rect.center().y - galley.size().y / 2.0),
        galley,
        theme::TEXT_STRONG,
    );
}

fn pill_rect(row: Rect, action: Action) -> Rect {
    let indent = if action.joins_previous() {
        FOLD_INDENT
    } else {
        0.0
    };
    Rect::from_min_size(
        pos2(row.left() + PILL_X + indent, row.center().y - 11.0),
        vec2(PILL_W, 22.0),
    )
}

fn editor(ui: &mut Ui, id: egui::Id, text: &mut String, caption: &str) {
    ui.horizontal(|ui| {
        ui.add_space(EDITOR_INDENT);
        ui.label(RichText::new(caption).size(11.0).color(theme::TEXT_FAINT));
    });
    ui.add_space(4.0);
    ui.horizontal_top(|ui| {
        ui.add_space(EDITOR_INDENT);
        ui.add(
            egui::TextEdit::multiline(text)
                .id(id)
                .desired_rows(3)
                .desired_width(ui.available_width() - 20.0)
                .font(FontId::proportional(13.0)),
        );
    });
    ui.add_space(10.0);
}

fn key_action(ui: &Ui) -> Option<Action> {
    ui.input(|i| {
        Action::ALL.into_iter().find(|&action| {
            let key = match action {
                Action::Pick => Key::P,
                Action::Reword => Key::R,
                Action::Edit => Key::E,
                Action::Squash => Key::S,
                Action::Fixup => Key::F,
                Action::Drop => Key::D,
            };
            i.key_pressed(key) && i.modifiers.is_none()
        })
    })
}

fn action_key(action: Action) -> &'static str {
    match action {
        Action::Pick => "P",
        Action::Reword => "R",
        Action::Edit => "E",
        Action::Squash => "S",
        Action::Fixup => "F",
        Action::Drop => "D",
    }
}

fn action_icon(action: Action) -> Icon {
    match action {
        Action::Pick => Icon::Check,
        Action::Reword => Icon::Pencil,
        Action::Edit => Icon::Terminal,
        Action::Squash => Icon::Merge,
        Action::Fixup => Icon::Rebase,
        Action::Drop => Icon::Trash,
    }
}

fn action_color(action: Action) -> Color32 {
    match action {
        Action::Pick => theme::TEXT_MUTED,
        Action::Reword => theme::lane(3),
        Action::Edit => theme::lane(5),
        Action::Squash => theme::lane(2),
        Action::Fixup => theme::lane(1),
        Action::Drop => theme::DELETED,
    }
}

fn drop_index(centers: &[f32], from: usize, pointer_y: f32) -> usize {
    centers
        .iter()
        .enumerate()
        .filter(|&(i, &center)| i != from && center < pointer_y)
        .count()
}

fn has_changes(
    steps: &[Step],
    original: &[ObjectId],
    combined: &HashMap<ObjectId, String>,
) -> bool {
    let reordered = steps.iter().map(|s| s.id).ne(original.iter().copied());
    let acted = steps.iter().any(|s| match s.action {
        Action::Pick => false,
        Action::Reword => s.message != s.original,
        _ => true,
    });
    reordered || acted || !combined.is_empty()
}

fn summary(steps: &[Step], original: &[ObjectId]) -> String {
    let count = |action: Action| steps.iter().filter(|s| s.action == action).count();
    let folded = count(Action::Squash) + count(Action::Fixup);
    let reworded = steps
        .iter()
        .filter(|s| s.action == Action::Reword && s.message != s.original)
        .count();
    let moved = steps
        .iter()
        .zip(original)
        .filter(|(s, id)| s.id != **id)
        .count();
    let parts: Vec<String> = [
        (count(Action::Drop), "dropped"),
        (folded, "squashed"),
        (reworded, "reworded"),
        (count(Action::Edit), "to edit"),
        (moved, "moved"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, what)| format!("{n} {what}"))
    .collect();
    let commits = format!(
        "{} commit{}",
        steps.len(),
        if steps.len() == 1 { "" } else { "s" }
    );
    if parts.is_empty() {
        format!("{commits}, nothing changed yet")
    } else {
        format!("{commits}: {}", parts.join(", "))
    }
}

pub struct HeadReach {
    key: Option<(ObjectId, usize)>,
    rows: Vec<bool>,
}

impl HeadReach {
    pub fn new() -> Self {
        Self {
            key: None,
            rows: Vec::new(),
        }
    }

    pub fn can_rebase_from(
        &mut self,
        history: &History,
        head_row: Option<usize>,
        row: usize,
    ) -> bool {
        let Some(head) = head_row else {
            return false;
        };
        let key = (history.id(head), history.len());
        if self.key != Some(key) {
            self.rows = vec![false; history.len()];
            self.rows[head] = true;
            for r in head..history.len() {
                if self.rows[r] {
                    for &p in history.parents(r) {
                        self.rows[p as usize] = true;
                    }
                }
            }
            self.key = Some(key);
        }
        row != head && self.rows.get(row).copied().unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(n: u8, action: Action) -> Step {
        let hex = format!("{n:02x}").repeat(20);
        Step {
            id: ObjectId::from_hex(hex.as_bytes()).unwrap(),
            author: "a".into(),
            time: 0,
            original: format!("c{n}"),
            action,
            message: format!("c{n}"),
            merge: false,
        }
    }

    #[test]
    fn summary_counts_what_changed() {
        let mut steps = vec![
            step(1, Action::Pick),
            step(2, Action::Squash),
            step(3, Action::Drop),
        ];
        let original: Vec<ObjectId> = steps.iter().map(|s| s.id).collect();
        assert_eq!(
            summary(&steps, &original),
            "3 commits: 1 dropped, 1 squashed"
        );
        steps.swap(0, 2);
        assert!(summary(&steps, &original).ends_with("2 moved"));
    }

    #[test]
    fn untouched_plan_has_no_changes() {
        let mut steps = vec![step(1, Action::Pick), step(2, Action::Reword)];
        let original: Vec<ObjectId> = steps.iter().map(|s| s.id).collect();
        assert!(!has_changes(&steps, &original, &HashMap::new()));
        steps[1].message = "new".into();
        assert!(has_changes(&steps, &original, &HashMap::new()));
    }

    #[test]
    fn rows_move_past_neighbor_centers() {
        let centers = [20.0, 66.0, 112.0];
        assert_eq!(drop_index(&centers, 0, 70.0), 1);
        assert_eq!(drop_index(&centers, 2, 10.0), 0);
        assert_eq!(drop_index(&centers, 1, 60.0), 1);
    }

    #[test]
    fn start_uses_the_edited_squash_message() {
        let dir = std::env::temp_dir().join(format!("kelp-rebase-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| kelp_core::git_cli::run(&dir, args).unwrap();
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "t@example.com"]);
        git(&["config", "user.name", "T"]);
        for name in ["a", "b", "c"] {
            std::fs::write(dir.join(name), name).unwrap();
            git(&["add", "."]);
            git(&["commit", "-q", "-m", name]);
        }
        let mut plan = rebase::load(&dir, "HEAD~2").unwrap();
        plan.steps[1].action = Action::Squash;
        let leader = plan.steps[0].id;
        let start = Start {
            plan,
            combined: HashMap::from([(leader, "b and c".to_string())]),
        };
        start.run(&dir).unwrap();
        assert_eq!(git(&["log", "-1", "--format=%s"]).trim(), "b and c");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
