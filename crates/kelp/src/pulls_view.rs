use std::sync::mpsc::{self, Receiver};

use eframe::egui::{
    self, Align2, CornerRadius, FontId, Rect, RichText, Sense, Stroke, Ui, pos2, vec2,
};
use kelp_core::avatar::GitHubRepo;
use kelp_core::ops::Op;
use kelp_core::pulls::{ListTab, Pull, PullSummary};

use crate::commands::Command;
use crate::icons::Icon;
use crate::{menus, theme, widgets};

const ROW_H: f32 = 52.0;
const AVATAR_R: f32 = 12.0;
const PULL_REMOTE: &str = "origin";

enum Loaded {
    Loading(Receiver<Result<Vec<PullSummary>, String>>),
    Ready(Vec<PullSummary>),
    Failed(String),
}

pub struct PullsView {
    github: GitHubRepo,
    tab: ListTab,
    filter: String,
    loaded: Loaded,
}

pub enum Event {
    None,
    Close,
    Command(Command),
    ShowBranch(String),
}

impl PullsView {
    pub fn open(ctx: &egui::Context, github: GitHubRepo) -> Self {
        let loaded = load(ctx, &github, ListTab::Open);
        Self {
            github,
            tab: ListTab::Open,
            filter: String::new(),
            loaded,
        }
    }

    pub fn reload(&mut self, ctx: &egui::Context) {
        self.loaded = load(ctx, &self.github, self.tab);
    }

    pub fn ui(&mut self, ui: &mut Ui, local: &[String], remote: &[String]) -> Event {
        if let Loaded::Loading(rx) = &self.loaded
            && let Ok(result) = rx.try_recv()
        {
            self.loaded = match result {
                Ok(list) => Loaded::Ready(list),
                Err(e) => Loaded::Failed(e),
            };
        }
        let mut event = Event::None;
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(24, 18))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Pull requests")
                            .size(18.0)
                            .family(theme::semibold())
                            .color(theme::text_strong()),
                    );
                    ui.label(
                        RichText::new(format!("{}/{}", self.github.owner, self.github.name))
                            .size(12.0)
                            .color(theme::text_faint()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::close_button(ui, "Close (Esc)") {
                            event = Event::Close;
                        }
                    });
                });
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let before = self.tab;
                    widgets::segmented(
                        ui,
                        &mut self.tab,
                        &[
                            (ListTab::Open, "Open"),
                            (ListTab::Mine, "Mine"),
                            (ListTab::ReviewRequested, "Review requested"),
                        ],
                    );
                    if self.tab != before {
                        self.loaded = load(ui.ctx(), &self.github, self.tab);
                    }
                    ui.add_space(8.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter)
                            .hint_text("Filter by title, number, author or branch")
                            .desired_width(320.0),
                    );
                });
                ui.add_space(12.0);
                let list = match &self.loaded {
                    Loaded::Loading(_) => return notice(ui, "Loading pull requests…"),
                    Loaded::Failed(e) => return notice(ui, e),
                    Loaded::Ready(list) => list,
                };
                let shown: Vec<&PullSummary> =
                    list.iter().filter(|p| matches(p, &self.filter)).collect();
                if shown.is_empty() {
                    return notice(ui, "No pull requests here.");
                }
                let now = now();
                egui::ScrollArea::vertical().auto_shrink(false).show_rows(
                    ui,
                    ROW_H,
                    shown.len(),
                    |ui, rows| {
                        for i in rows {
                            if let Some(e) = row(ui, shown[i], &self.github, local, remote, now) {
                                event = e;
                            }
                        }
                    },
                );
            });
        event
    }
}

fn load(ctx: &egui::Context, github: &GitHubRepo, tab: ListTab) -> Loaded {
    let (tx, rx) = mpsc::channel();
    let github = github.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let result = kelp_core::pulls::list_open(&github, tab).map_err(|e| {
            if kelp_core::pulls::gh_available() {
                format!("Could not load pull requests: {e:#}")
            } else {
                "Install the GitHub CLI (gh) and run gh auth login to list pull requests here."
                    .to_string()
            }
        });
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    Loaded::Loading(rx)
}

fn matches(pull: &PullSummary, filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    filter.is_empty()
        || [
            pull.pull.title.to_lowercase(),
            format!("#{}", pull.pull.number),
            pull.author.to_lowercase(),
            pull.pull.head.to_lowercase(),
        ]
        .iter()
        .any(|field| field.contains(&filter))
}

pub fn checkout_op(pull: &Pull, owner: &str, local: &[String]) -> Op {
    if local.contains(&pull.head) {
        return Op::Switch(pull.head.clone());
    }
    let from_fork = pull
        .head_owner
        .as_deref()
        .is_some_and(|o| !o.eq_ignore_ascii_case(owner));
    let branch = if from_fork {
        format!("pr/{}-{}", pull.number, pull.head)
    } else {
        pull.head.clone()
    };
    Op::CheckoutPull {
        remote: PULL_REMOTE.to_string(),
        number: pull.number,
        branch,
    }
}

fn row(
    ui: &mut Ui,
    item: &PullSummary,
    github: &GitHubRepo,
    local: &[String],
    remote: &[String],
    now: i64,
) -> Option<Event> {
    let pull = &item.pull;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let painter = ui.painter_at(rect);
    let hovered = response.hovered();
    if hovered {
        painter.rect_filled(rect, CornerRadius::same(8), theme::control());
    }
    crate::graph_view::draw_avatar(
        &painter,
        pos2(rect.left() + 12.0 + AVATAR_R, rect.center().y),
        AVATAR_R,
        &item.author,
        &item.author,
        None,
        theme::border(),
        false,
    );
    let text_x = rect.left() + 12.0 + AVATAR_R * 2.0 + 12.0;
    let pill_w = crate::pulls_ui::pill_width(&painter, pull);
    let pill = Rect::from_min_size(
        pos2(text_x, rect.top() + 9.0),
        vec2(pill_w, crate::pulls_ui::PILL_H),
    );
    crate::pulls_ui::paint_pill(&painter, pill, pull, false);
    let actions_w = if hovered { 250.0 } else { 90.0 };
    let title_room = (rect.right() - actions_w - pill.right() - 10.0).max(40.0);
    let title = crate::graph_view::truncated(
        &painter,
        pull.title.clone(),
        FontId::new(13.5, theme::semibold()),
        theme::text_strong(),
        title_room,
    );
    painter.galley(
        pos2(pill.right() + 8.0, rect.top() + 8.0),
        title,
        theme::text_strong(),
    );
    let mut meta = vec![format!("{} wants to merge {}", item.author, pull.head)];
    meta.extend(pull.review_text().map(str::to_string));
    meta.extend(pull.checks_text().map(str::to_string));
    let meta = crate::graph_view::truncated(
        &painter,
        meta.join("  ·  "),
        FontId::proportional(12.0),
        theme::text_muted(),
        (rect.right() - actions_w - text_x).max(40.0),
    );
    painter.galley(pos2(text_x, rect.top() + 30.0), meta, theme::text_muted());
    if let Some(updated) = item.updated {
        painter.text(
            pos2(rect.right() - 14.0, rect.center().y),
            Align2::RIGHT_CENTER,
            kelp_core::commit::relative_time(updated, now),
            FontId::proportional(11.5),
            theme::text_faint(),
        );
    }
    let on_remote = remote
        .iter()
        .any(|r| r.split_once('/').is_some_and(|(_, b)| b == pull.head));
    let in_graph = local.contains(&pull.head) || on_remote;
    let mut picked = None;
    if hovered {
        let mut x = rect.right() - 90.0;
        let mut button = |label: &str, width: f32| {
            let r = Rect::from_min_size(pos2(x - width, rect.center().y - 13.0), vec2(width, 26.0));
            x -= width + 6.0;
            let hot = ui.rect_contains_pointer(r);
            painter.rect(
                r,
                CornerRadius::same(5),
                if hot {
                    theme::control_hover()
                } else {
                    theme::panel()
                },
                Stroke::new(1.0, theme::border()),
                egui::StrokeKind::Inside,
            );
            painter.text(
                r.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(12.0),
                theme::text(),
            );
            hot && response.clicked()
        };
        if button("Check out", 76.0) {
            picked = Some(Event::Command(Command::Run(checkout_op(
                pull,
                &github.owner,
                local,
            ))));
        }
        if button("Open", 52.0) {
            picked = Some(Event::Command(Command::OpenUrl(pull.url.clone())));
        }
        if in_graph && button("Show in graph", 104.0) {
            picked = Some(Event::ShowBranch(pull.head.clone()));
        }
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if picked.is_none() && response.double_clicked() {
        picked = Some(Event::Command(Command::OpenUrl(pull.url.clone())));
    }
    crate::menus::context_menu(&response, |ui| {
        menus::menu_width(ui, 220.0);
        if menus::row(ui, Some(Icon::Check), "Check out", None, false) {
            picked = Some(Event::Command(Command::Run(checkout_op(
                pull,
                &github.owner,
                local,
            ))));
            ui.close();
        }
        if menus::row(ui, Some(Icon::Push), "Open in browser", None, false) {
            picked = Some(Event::Command(Command::OpenUrl(pull.url.clone())));
            ui.close();
        }
        if in_graph && menus::row(ui, Some(Icon::Branch), "Show branch in graph", None, false) {
            picked = Some(Event::ShowBranch(pull.head.clone()));
            ui.close();
        }
        if menus::row(ui, Some(Icon::Copy), "Copy link", None, false) {
            picked = Some(Event::Command(Command::Copy(pull.url.clone())));
            ui.close();
        }
    });
    picked
}

fn notice(ui: &mut Ui, text: &str) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(text).color(theme::text_muted()));
    });
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kelp_core::pulls::State;

    fn pull(head: &str, owner: &str) -> Pull {
        Pull {
            number: 42,
            title: "Fix it".into(),
            head: head.into(),
            head_owner: Some(owner.into()),
            state: State::Open,
            url: "https://github.com/hobo-ware/kelp/pull/42".into(),
            review: None,
            checks: None,
        }
    }

    #[test]
    fn checkout_picks_switch_fetch_or_a_fork_safe_name() {
        let local = vec!["feat/x".to_string()];
        assert_eq!(
            checkout_op(&pull("feat/x", "hobo-ware"), "hobo-ware", &local),
            Op::Switch("feat/x".into())
        );
        assert_eq!(
            checkout_op(&pull("feat/y", "Hobo-Ware"), "hobo-ware", &local),
            Op::CheckoutPull {
                remote: "origin".into(),
                number: 42,
                branch: "feat/y".into()
            }
        );
        assert_eq!(
            checkout_op(&pull("main", "someone"), "hobo-ware", &local),
            Op::CheckoutPull {
                remote: "origin".into(),
                number: 42,
                branch: "pr/42-main".into()
            }
        );
    }

    #[test]
    fn clicking_check_out_on_a_row_emits_the_checkout_op() {
        use eframe::egui::{Event as Input, PointerButton, Pos2, RawInput};
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let github = GitHubRepo {
            owner: "hobo-ware".into(),
            name: "kelp".into(),
        };
        let item = PullSummary {
            pull: pull("feat/y", "hobo-ware"),
            author: "maya".into(),
            updated: None,
        };
        let frame = |events: Vec<Input>| {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 200.0))),
                events,
                ..Default::default()
            };
            let mut got = None;
            let mut rect = Rect::NOTHING;
            let _ = ctx.run_ui(input, |ui| {
                let top = ui.cursor().min;
                got = row(ui, &item, &github, &[], &[], 0);
                rect = Rect::from_min_size(top, vec2(ui.min_rect().width(), ROW_H));
            });
            (got, rect)
        };
        let (_, rect) = frame(vec![]);
        let at = pos2(rect.right() - 90.0 - 38.0, rect.center().y);
        let press = |pressed| Input::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(vec![Input::PointerMoved(at)]);
        frame(vec![Input::PointerMoved(at)]);
        frame(vec![press(true)]);
        let (event, _) = frame(vec![press(false)]);
        assert!(matches!(
            event,
            Some(Event::Command(Command::Run(Op::CheckoutPull { number: 42, ref branch, .. })))
                if branch == "feat/y"
        ));
    }

    #[test]
    fn the_filter_matches_title_number_author_and_branch() {
        let item = PullSummary {
            pull: pull("feat/export", "hobo-ware"),
            author: "maya".into(),
            updated: None,
        };
        for query in ["fix", "#42", "MAYA", "export", ""] {
            assert!(matches(&item, query), "{query}");
        }
        assert!(!matches(&item, "nothing"));
    }
}
