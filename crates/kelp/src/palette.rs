use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Key, Margin, Modifiers, Rect, Sense, Stroke, Ui,
    pos2, text::LayoutJob, vec2,
};
use gix::ObjectId;

use crate::actions::{self, Action, Run};
use crate::commands::Command;
use crate::icons::{self, Icon};
use crate::menus::Entry;
use crate::theme;

const WIDTH: f32 = 640.0;
const ROW_H: f32 = 32.0;
const LIST_H: f32 = 430.0;
const RECENT_MAX: usize = 5;
const SECTION_MAX: usize = 8;
const COMMIT_HITS: usize = 30;
const COMMIT_MIN_QUERY: usize = 3;
const SEARCH_DELAY: Duration = Duration::from_millis(200);
const SHEET_KEYS_W: f32 = 190.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub score: i32,
    pub positions: Vec<usize>,
}

pub fn fuzzy(query: &str, text: &str) -> Option<Match> {
    let query: Vec<char> = query.trim().to_lowercase().chars().collect();
    if query.is_empty() {
        return Some(Match {
            score: 0,
            positions: Vec::new(),
        });
    }
    let chars: Vec<char> = text.chars().collect();
    let lower: Vec<char> = text.to_lowercase().chars().collect();
    if lower.len() != chars.len() {
        return None;
    }
    let word_start = |i: usize| {
        i == 0
            || matches!(chars[i - 1], ' ' | '/' | '-' | '_' | '.' | ':' | '(')
            || (chars[i].is_uppercase() && chars[i - 1].is_lowercase())
    };
    let mut positions = Vec::with_capacity(query.len());
    let mut score = 0;
    let mut from = 0;
    for &wanted in &query {
        let contiguous = !positions.is_empty() && lower.get(from) == Some(&wanted);
        let found = if contiguous {
            from
        } else {
            (from..lower.len())
                .find(|&i| lower[i] == wanted && word_start(i))
                .or_else(|| (from..lower.len()).find(|&i| lower[i] == wanted))?
        };
        score += if found == 0 {
            15
        } else if word_start(found) {
            10
        } else {
            1
        };
        if positions.last().is_some_and(|&last| found == last + 1) {
            score += 6;
        }
        score -= (found - from).min(10) as i32;
        positions.push(found);
        from = found + 1;
    }
    let joined: String = query.iter().collect();
    let haystack: String = lower.iter().collect();
    if haystack.starts_with(&joined) {
        score += 25;
    } else if haystack.contains(&joined) {
        score += 10;
    }
    score -= (chars.len() as i32 / 8).min(10);
    Some(Match { score, positions })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    All,
    Actions,
    Branches,
    Commits,
    Files,
}

fn parse(query: &str) -> (Mode, &str) {
    let trimmed = query.trim_start();
    let mode = match trimmed.chars().next() {
        Some('>') => Mode::Actions,
        Some('@') => Mode::Branches,
        Some('#') => Mode::Commits,
        Some('/') => Mode::Files,
        _ => return (Mode::All, trimmed),
    };
    (mode, trimmed[1..].trim_start())
}

#[derive(Clone)]
pub struct BranchItem {
    pub name: String,
    pub remote: bool,
    pub current: bool,
}

pub struct Sources<'a> {
    pub state: actions::State,
    pub branches: Vec<BranchItem>,
    pub files: Vec<String>,
    pub tabs: Vec<String>,
    pub active_tab: usize,
    pub history: Option<(&'a Path, &'a [ObjectId])>,
}

pub enum Pick {
    Run(Run),
    Checkout(BranchItem),
    BranchMenu(String),
    Command(Command),
    Commit(ObjectId),
    File(String),
    Tab(usize),
}

#[derive(Clone)]
struct CommitHit {
    id: ObjectId,
    short: String,
    title: String,
}

#[derive(Default)]
struct CommitSearch {
    wanted: String,
    changed_at: Option<Instant>,
    running: Option<(String, Receiver<Vec<CommitHit>>)>,
    done: String,
    hits: Vec<CommitHit>,
    generation: Arc<AtomicU64>,
}

impl CommitSearch {
    fn want(&mut self, query: &str) {
        if query != self.wanted {
            self.wanted = query.to_string();
            self.changed_at = Some(Instant::now());
            self.generation.fetch_add(1, Ordering::Relaxed);
            self.running = None;
            if query.chars().count() < COMMIT_MIN_QUERY {
                self.hits.clear();
                self.done.clear();
                self.changed_at = None;
            }
        }
    }

    fn poll(&mut self, ctx: &egui::Context, history: Option<(&Path, &[ObjectId])>) {
        if let Some((query, rx)) = &self.running
            && let Ok(hits) = rx.try_recv()
        {
            self.done = query.clone();
            self.hits = hits;
            self.running = None;
        }
        let Some(changed) = self.changed_at else {
            return;
        };
        let Some((dir, ids)) = history else {
            return;
        };
        let waited = changed.elapsed();
        if waited < SEARCH_DELAY {
            ctx.request_repaint_after(SEARCH_DELAY - waited);
            return;
        }
        self.changed_at = None;
        let (tx, rx) = mpsc::channel();
        let query = self.wanted.clone();
        let dir = dir.to_path_buf();
        let ids = ids.to_vec();
        let generation = self.generation.clone();
        let started = generation.load(Ordering::Relaxed);
        let repaint = ctx.clone();
        let needle = query.clone();
        std::thread::spawn(move || {
            let cancelled = || generation.load(Ordering::Relaxed) != started;
            let hits = search_commits(&dir, &ids, &needle, &cancelled);
            if !cancelled() && tx.send(hits).is_ok() {
                repaint.request_repaint();
            }
        });
        self.running = Some((query, rx));
    }

    fn current(&self) -> &[CommitHit] {
        if self.done == self.wanted {
            &self.hits
        } else {
            &[]
        }
    }
}

fn search_commits(
    dir: &Path,
    ids: &[ObjectId],
    query: &str,
    cancelled: &dyn Fn() -> bool,
) -> Vec<CommitHit> {
    let needle = query.trim().to_lowercase();
    let Ok(repo) = gix::open(dir) else {
        return Vec::new();
    };
    let looks_like_hash = needle.len() >= 4 && needle.chars().all(|c| c.is_ascii_hexdigit());
    let mut hits = Vec::new();
    for (n, id) in ids.iter().enumerate() {
        if n % 512 == 0 && cancelled() {
            return Vec::new();
        }
        let hex = id.to_hex().to_string();
        let Ok(commit) = repo.find_commit(*id) else {
            continue;
        };
        let title = commit
            .message_raw_sloppy()
            .to_string()
            .lines()
            .next()
            .unwrap_or_default()
            .to_string();
        let by_hash = looks_like_hash && hex.starts_with(&needle);
        if by_hash || title.to_lowercase().contains(&needle) {
            hits.push(CommitHit {
                id: *id,
                short: hex[..7].to_string(),
                title,
            });
            if hits.len() >= COMMIT_HITS {
                break;
            }
        }
    }
    hits
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Recent,
    Actions,
    Branches,
    Commits,
    Files,
    Tabs,
    BranchActions,
}

impl Section {
    fn title(self) -> &'static str {
        match self {
            Section::Recent => "RECENT",
            Section::Actions => "ACTIONS",
            Section::Branches => "BRANCHES",
            Section::Commits => "COMMITS",
            Section::Files => "FILES",
            Section::Tabs => "TABS",
            Section::BranchActions => "BRANCH",
        }
    }
}

#[derive(Clone, Copy)]
enum Target {
    Action(&'static Action),
    Branch(usize),
    Commit(usize),
    File(usize),
    Tab(usize),
    Entry(usize),
}

struct Row {
    section: Section,
    target: Target,
    positions: Vec<usize>,
}

pub struct Palette {
    pub open: bool,
    query: String,
    selected: usize,
    pub recent: Vec<String>,
    branch_menu: Option<(String, Vec<Entry>)>,
    commits: CommitSearch,
    focus: bool,
    scroll_to_selected: bool,
}

impl Default for Palette {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Palette {
    pub fn new(recent: Vec<String>) -> Self {
        Self {
            open: false,
            query: String::new(),
            selected: 0,
            recent,
            branch_menu: None,
            commits: CommitSearch::default(),
            focus: false,
            scroll_to_selected: false,
        }
    }

    pub fn show(&mut self, query: &str) {
        self.open = true;
        self.query = query.to_string();
        self.selected = 0;
        self.branch_menu = None;
        self.focus = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.branch_menu = None;
    }

    pub fn show_branch_menu(&mut self, name: String, entries: Vec<Entry>) {
        self.branch_menu = Some((name, entries));
        self.selected = 0;
        self.focus = true;
    }

    pub fn remember(&mut self, id: &str) {
        self.recent.retain(|r| r != id);
        self.recent.insert(0, id.to_string());
        self.recent.truncate(RECENT_MAX);
    }

    fn rows(&self, sources: &Sources<'_>) -> Vec<Row> {
        if let Some((_, entries)) = &self.branch_menu {
            let (_, text) = parse(&self.query);
            return scored(
                entries
                    .iter()
                    .enumerate()
                    .map(|(i, e)| (i, e.label.as_str())),
                text,
                usize::MAX,
            )
            .into_iter()
            .map(|(i, positions)| Row {
                section: Section::BranchActions,
                target: Target::Entry(i),
                positions,
            })
            .collect();
        }
        let (mode, text) = parse(&self.query);
        let wants = |m: Mode| mode == Mode::All || mode == m;
        let mut rows = Vec::new();
        let enabled: Vec<&'static Action> = actions::ACTIONS
            .iter()
            .filter(|a| sources.state.allows(a.run))
            .collect();
        if text.is_empty() && mode == Mode::All {
            rows.extend(
                self.recent
                    .iter()
                    .filter_map(|id| enabled.iter().find(|a| a.id == id))
                    .map(|a| Row {
                        section: Section::Recent,
                        target: Target::Action(a),
                        positions: Vec::new(),
                    }),
            );
        }
        if wants(Mode::Actions) {
            let limit = if mode == Mode::Actions || text.is_empty() {
                usize::MAX
            } else {
                SECTION_MAX
            };
            let recent_shown = rows.len();
            let candidates = enabled
                .iter()
                .enumerate()
                .filter(|(_, a)| recent_shown == 0 || !self.recent.iter().any(|r| r == a.id));
            for (i, positions) in scored_actions(candidates, text, limit) {
                rows.push(Row {
                    section: Section::Actions,
                    target: Target::Action(enabled[i]),
                    positions,
                });
            }
        }
        if wants(Mode::Branches) && (mode == Mode::Branches || !text.is_empty()) {
            let items = sources
                .branches
                .iter()
                .enumerate()
                .map(|(i, b)| (i, b.name.as_str()));
            let limit = if mode == Mode::Branches {
                usize::MAX
            } else {
                SECTION_MAX
            };
            for (i, positions) in scored(items, text, limit) {
                rows.push(Row {
                    section: Section::Branches,
                    target: Target::Branch(i),
                    positions,
                });
            }
        }
        if wants(Mode::Commits) {
            for (i, hit) in self.commits.current().iter().enumerate() {
                let positions = fuzzy(text, &hit.title)
                    .map(|m| m.positions)
                    .unwrap_or_default();
                rows.push(Row {
                    section: Section::Commits,
                    target: Target::Commit(i),
                    positions,
                });
            }
        }
        if wants(Mode::Files) && (mode == Mode::Files || !text.is_empty()) {
            let items = sources
                .files
                .iter()
                .enumerate()
                .map(|(i, f)| (i, f.as_str()));
            for (i, positions) in scored(items, text, SECTION_MAX) {
                rows.push(Row {
                    section: Section::Files,
                    target: Target::File(i),
                    positions,
                });
            }
        }
        if mode == Mode::All && sources.tabs.len() > 1 {
            let items = sources
                .tabs
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != sources.active_tab)
                .map(|(i, t)| (i, t.as_str()));
            for (i, positions) in scored(items, text, SECTION_MAX) {
                rows.push(Row {
                    section: Section::Tabs,
                    target: Target::Tab(i),
                    positions,
                });
            }
        }
        rows
    }

    pub fn ui(&mut self, ctx: &egui::Context, sources: &Sources<'_>) -> Option<Pick> {
        if !self.open {
            return None;
        }
        let (mode, text) = parse(&self.query);
        let commit_query =
            if self.branch_menu.is_none() && matches!(mode, Mode::All | Mode::Commits) {
                text.to_string()
            } else {
                String::new()
            };
        self.commits.want(&commit_query);
        self.commits.poll(ctx, sources.history);

        let rows = self.rows(sources);
        let (down, up, enter, secondary, escape) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::COMMAND, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if escape {
            if self.branch_menu.take().is_some() {
                self.query.clear();
                self.selected = 0;
            } else {
                self.close();
            }
            return None;
        }
        if !rows.is_empty() {
            if down {
                self.selected = (self.selected + 1) % rows.len();
                self.scroll_to_selected = true;
            }
            if up {
                self.selected = (self.selected + rows.len() - 1) % rows.len();
                self.scroll_to_selected = true;
            }
        }
        self.selected = self.selected.min(rows.len().saturating_sub(1));
        let mut picked = None;
        if (enter || secondary) && !rows.is_empty() {
            picked = Some((self.selected, secondary));
        }

        let modal = egui::Modal::new(egui::Id::new("kelp-palette"))
            .area(
                egui::Area::new(egui::Id::new("kelp-palette-area"))
                    .anchor(Align2::CENTER_TOP, vec2(0.0, 88.0))
                    .order(egui::Order::Foreground),
            )
            .backdrop_color(Color32::from_black_alpha(90))
            .frame(
                egui::Frame::new()
                    .fill(theme::POPUP)
                    .stroke(Stroke::new(1.0, theme::POPUP_BORDER))
                    .corner_radius(12)
                    .inner_margin(Margin::same(8)),
            )
            .show(ctx, |ui| {
                ui.set_width(WIDTH);
                ui.spacing_mut().item_spacing.y = 0.0;
                self.search_field(ui);
                ui.add_space(6.0);
                ui.painter().hline(
                    ui.max_rect().x_range(),
                    ui.cursor().top(),
                    Stroke::new(1.0, theme::BORDER),
                );
                ui.add_space(6.0);
                if rows.is_empty() {
                    let hint =
                        if self.commits.running.is_some() || self.commits.changed_at.is_some() {
                            "Searching commits…"
                        } else {
                            "Nothing matches"
                        };
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(WIDTH, ROW_H * 2.0), Sense::hover());
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        hint,
                        FontId::proportional(13.0),
                        theme::TEXT_FAINT,
                    );
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(LIST_H)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            if let Some(clicked) = self.list(ui, &rows, sources) {
                                picked = Some((clicked, false));
                            }
                        });
                }
                ui.add_space(6.0);
                footer(ui, self.branch_menu.is_some());
            });
        if modal.should_close() {
            self.close();
            return None;
        }
        let (index, secondary) = picked?;
        self.pick(&rows[index], secondary, sources)
    }

    fn search_field(&mut self, ui: &mut Ui) {
        let hint = match &self.branch_menu {
            Some((name, _)) => format!("Actions for {name}"),
            None => "Search actions, branches, commits and files   > actions  @ branches  # commits  / files".into(),
        };
        egui::Frame::new()
            .inner_margin(Margin::symmetric(10, 6))
            .show(ui, |ui| {
                let edit = egui::TextEdit::singleline(&mut self.query)
                    .hint_text(hint)
                    .frame(egui::Frame::NONE)
                    .font(FontId::proportional(15.0))
                    .desired_width(f32::INFINITY);
                let response = ui.add(edit);
                if self.focus {
                    response.request_focus();
                    self.focus = false;
                }
                if response.changed() {
                    self.selected = 0;
                }
            });
    }

    fn list(&mut self, ui: &mut Ui, rows: &[Row], sources: &Sources<'_>) -> Option<usize> {
        let mut clicked = None;
        let mut section = None;
        for (i, row) in rows.iter().enumerate() {
            if section != Some(row.section) {
                section = Some(row.section);
                crate::menus::heading(ui, row.section.title());
            }
            let (rect, response) = ui.allocate_exact_size(vec2(WIDTH, ROW_H), Sense::click());
            if response.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                self.selected = i;
            }
            if response.clicked() {
                clicked = Some(i);
            }
            let selected = i == self.selected;
            if selected && std::mem::take(&mut self.scroll_to_selected) {
                ui.scroll_to_rect(rect, None);
            }
            self.paint_row(ui, rect, row, selected, sources);
        }
        clicked
    }

    fn paint_row(&self, ui: &Ui, rect: Rect, row: &Row, selected: bool, sources: &Sources<'_>) {
        let painter = ui.painter_at(rect);
        if selected {
            painter.rect_filled(rect, CornerRadius::same(6), theme::MENU_HOVER);
        }
        let (icon, label, detail, hint, danger) = self.describe(row.target, sources);
        let ink = if danger {
            theme::DELETED
        } else if selected {
            theme::TEXT_STRONG
        } else {
            theme::TEXT
        };
        if let Some(icon) = icon {
            let glyph =
                Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), vec2(15.0, 15.0));
            icons::paint(
                &painter,
                glyph,
                icon,
                if selected { ink } else { theme::TEXT_MUTED },
            );
        }
        let mut job = LayoutJob::default();
        for (i, c) in label.chars().enumerate() {
            let color = if row.positions.contains(&i) {
                theme::ACCENT
            } else {
                ink
            };
            job.append(
                &c.to_string(),
                0.0,
                egui::TextFormat::simple(FontId::proportional(13.5), color),
            );
        }
        if let Some(detail) = detail {
            job.append(
                &detail,
                10.0,
                egui::TextFormat::simple(FontId::proportional(12.0), theme::TEXT_FAINT),
            );
        }
        let hint_w = hint.as_ref().map_or(0.0, |h| h.len() as f32 * 7.0 + 24.0);
        job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 44.0 - hint_w);
        let galley = painter.layout_job(job);
        painter.galley(
            pos2(rect.left() + 38.0, rect.center().y - galley.size().y / 2.0),
            galley,
            ink,
        );
        if let Some(hint) = hint {
            painter.text(
                pos2(rect.right() - 12.0, rect.center().y),
                Align2::RIGHT_CENTER,
                hint,
                FontId::monospace(11.0),
                theme::TEXT_FAINT,
            );
        }
    }

    fn describe(
        &self,
        target: Target,
        sources: &Sources<'_>,
    ) -> (Option<Icon>, String, Option<String>, Option<String>, bool) {
        match target {
            Target::Action(a) => (
                a.icon,
                a.title.to_string(),
                None,
                a.shortcut.map(str::to_string),
                false,
            ),
            Target::Branch(i) => {
                let b = &sources.branches[i];
                let detail = match (b.current, b.remote) {
                    (true, _) => Some("current".to_string()),
                    (_, true) => Some("remote".to_string()),
                    _ => None,
                };
                (
                    Some(Icon::Branch),
                    b.name.clone(),
                    detail,
                    Some("Cmd+Enter for more".into()),
                    false,
                )
            }
            Target::Commit(i) => {
                let hit = &self.commits.current()[i];
                (
                    None,
                    hit.title.clone(),
                    Some(hit.short.clone()),
                    None,
                    false,
                )
            }
            Target::File(i) => (
                Some(Icon::Pencil),
                sources.files[i].clone(),
                None,
                None,
                false,
            ),
            Target::Tab(i) => (
                Some(Icon::Folder),
                sources.tabs[i].clone(),
                Some("tab".into()),
                None,
                false,
            ),
            Target::Entry(i) => {
                let entry = &self.branch_menu.as_ref().expect("entries shown").1[i];
                (
                    Some(entry.icon),
                    entry.label.clone(),
                    None,
                    None,
                    entry.danger,
                )
            }
        }
    }

    fn pick(&mut self, row: &Row, secondary: bool, sources: &Sources<'_>) -> Option<Pick> {
        let pick = match row.target {
            Target::Action(action) => {
                self.remember(action.id);
                Pick::Run(action.run)
            }
            Target::Branch(i) => {
                let branch = sources.branches[i].clone();
                if secondary {
                    self.query.clear();
                    return Some(Pick::BranchMenu(branch.name));
                }
                Pick::Checkout(branch)
            }
            Target::Commit(i) => Pick::Commit(self.commits.current()[i].id),
            Target::File(i) => Pick::File(sources.files[i].clone()),
            Target::Tab(i) => Pick::Tab(i),
            Target::Entry(i) => {
                let (_, mut entries) = self.branch_menu.take()?;
                Pick::Command(entries.swap_remove(i).command)
            }
        };
        self.close();
        Some(pick)
    }
}

fn scored<'a>(
    items: impl Iterator<Item = (usize, &'a str)>,
    query: &str,
    limit: usize,
) -> Vec<(usize, Vec<usize>)> {
    let mut hits: Vec<(i32, usize, Vec<usize>)> = items
        .filter_map(|(i, text)| fuzzy(query, text).map(|m| (m.score, i, m.positions)))
        .collect();
    if !query.is_empty() {
        hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    }
    hits.into_iter()
        .take(limit)
        .map(|(_, i, p)| (i, p))
        .collect()
}

fn scored_actions<'a>(
    items: impl Iterator<Item = (usize, &'a &'static Action)>,
    query: &str,
    limit: usize,
) -> Vec<(usize, Vec<usize>)> {
    let mut hits: Vec<(i32, usize, Vec<usize>)> = items
        .filter_map(|(i, a)| {
            let by_title = fuzzy(query, a.title);
            let by_keyword = fuzzy(query, a.keywords).map(|m| Match {
                score: m.score - 8,
                positions: Vec::new(),
            });
            let best = match (by_title, by_keyword) {
                (Some(t), Some(k)) if k.score > t.score => Some(k),
                (Some(t), _) => Some(t),
                (None, k) => k,
            }?;
            Some((best.score, i, best.positions))
        })
        .collect();
    if !query.is_empty() {
        hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    }
    hits.into_iter()
        .take(limit)
        .map(|(_, i, p)| (i, p))
        .collect()
}

fn footer(ui: &mut Ui, branch_menu: bool) {
    let text = if branch_menu {
        "Enter to run   Esc to go back"
    } else {
        "Up / Down to move   Enter to run   Cmd+Enter for branch actions   Esc to close"
    };
    let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, 20.0), Sense::hover());
    ui.painter().text(
        pos2(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(11.0),
        theme::TEXT_FAINT,
    );
}

pub fn shortcut_sheet(ctx: &egui::Context, open: &mut bool) {
    if !*open {
        return;
    }
    let modal = egui::Modal::new(egui::Id::new("kelp-shortcuts"))
        .frame(
            egui::Frame::new()
                .fill(theme::POPUP)
                .stroke(Stroke::new(1.0, theme::POPUP_BORDER))
                .corner_radius(12)
                .inner_margin(Margin::same(22)),
        )
        .show(ctx, |ui| {
            ui.set_width(820.0);
            let mut close = false;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Keyboard shortcuts")
                        .size(17.0)
                        .family(theme::semibold())
                        .color(theme::TEXT_STRONG),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = crate::widgets::close_button(ui, "Close (Esc)");
                });
            });
            ui.add_space(12.0);
            ui.columns(2, |columns| {
                for (n, group) in actions::Group::ALL.iter().enumerate() {
                    group_block(&mut columns[n / 2], *group);
                }
            });
            close
        });
    if modal.should_close() || modal.inner {
        *open = false;
    }
}

fn group_block(ui: &mut Ui, group: actions::Group) {
    ui.label(
        egui::RichText::new(group.title().to_uppercase())
            .size(11.0)
            .family(theme::semibold())
            .color(theme::TEXT_MUTED),
    );
    ui.add_space(4.0);
    for row in actions::KEYS.iter().filter(|k| k.group == group) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.allocate_ui_with_layout(
                vec2(SHEET_KEYS_W, 22.0),
                egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
                |ui| {
                    ui.set_min_width(SHEET_KEYS_W);
                    for part in actions::key_parts(row.keys) {
                        key_label(ui, part);
                    }
                },
            );
            ui.add(
                egui::Label::new(egui::RichText::new(row.what).size(12.5).color(theme::TEXT))
                    .wrap(),
            );
        });
        ui.add_space(4.0);
    }
    ui.add_space(12.0);
}

fn is_gesture(part: &str) -> bool {
    part.split(' ')
        .any(|word| word.len() > 2 && word.chars().all(|c| c.is_ascii_lowercase()))
}

fn key_label(ui: &mut Ui, part: &str) {
    if is_gesture(part) {
        ui.label(
            egui::RichText::new(part)
                .size(12.0)
                .color(theme::TEXT_MUTED),
        );
    } else if let Some((first, last)) = part.split_once(" ... ") {
        key_chip(ui, first);
        ui.label(
            egui::RichText::new("to")
                .size(11.5)
                .color(theme::TEXT_FAINT),
        );
        key_chip(ui, last);
    } else {
        key_chip(ui, part);
    }
}

fn key_chip(ui: &mut Ui, text: &str) {
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        FontId::monospace(11.0),
        theme::TEXT_STRONG,
    );
    let size = galley.size() + vec2(12.0, 6.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect(
        rect,
        CornerRadius::same(5),
        theme::FIELD,
        Stroke::new(1.0, theme::BORDER),
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(
        rect.center() - galley.size() / 2.0,
        galley,
        theme::TEXT_STRONG,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(q: &str, t: &str) -> i32 {
        fuzzy(q, t).map_or(i32::MIN, |m| m.score)
    }

    #[test]
    fn prefix_beats_the_middle() {
        assert!(score("pu", "Push") > score("pu", "Stash push"));
        assert!(score("fet", "Fetch") > score("fet", "Prefetch"));
    }

    #[test]
    fn word_starts_beat_mid_word() {
        assert!(score("nb", "New branch") > score("nb", "Unbind"));
        assert!(score("fd", "feat/delighters") > score("fd", "fixed"));
    }

    #[test]
    fn subsequences_match_case_insensitively() {
        let m = fuzzy("NWT", "New worktree…").unwrap();
        assert_eq!(m.positions, vec![0, 4, 8]);
        assert!(fuzzy("xyz", "Fetch").is_none());
        assert!(fuzzy("", "anything").is_some());
    }

    #[test]
    fn prefixes_pick_a_mode() {
        assert_eq!(parse("> fetch"), (Mode::Actions, "fetch"));
        assert_eq!(parse("@main"), (Mode::Branches, "main"));
        assert_eq!(parse("#abc1"), (Mode::Commits, "abc1"));
        assert_eq!(parse("/src"), (Mode::Files, "src"));
        assert_eq!(parse("plain"), (Mode::All, "plain"));
    }

    fn sources() -> Sources<'static> {
        Sources {
            state: actions::State {
                repo: true,
                branch: true,
                ..actions::State::default()
            },
            branches: vec![
                BranchItem {
                    name: "main".into(),
                    remote: false,
                    current: true,
                },
                BranchItem {
                    name: "feat/export".into(),
                    remote: false,
                    current: false,
                },
            ],
            files: vec!["src/app.rs".into()],
            tabs: vec!["kelp".into()],
            active_tab: 0,
            history: None,
        }
    }

    fn frame(
        ctx: &egui::Context,
        palette: &mut Palette,
        sources: &Sources<'_>,
        events: Vec<egui::Event>,
    ) -> Option<Pick> {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0))),
            events,
            ..Default::default()
        };
        let mut picked = None;
        let _ = ctx.run_ui(input, |ui| picked = palette.ui(ui.ctx(), sources));
        picked
    }

    fn type_and_press(query: &str, modifiers: Modifiers) -> Option<Pick> {
        let ctx = egui::Context::default();
        let sources = sources();
        let mut palette = Palette::default();
        palette.show("");
        frame(&ctx, &mut palette, &sources, Vec::new());
        frame(
            &ctx,
            &mut palette,
            &sources,
            vec![egui::Event::Text(query.into())],
        );
        let enter = egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        frame(&ctx, &mut palette, &sources, vec![enter])
    }

    #[test]
    fn typing_and_enter_runs_the_best_action() {
        let pick = type_and_press("pull", Modifiers::NONE);
        assert!(matches!(
            pick,
            Some(Pick::Run(Run::Repo(actions::RepoAction::Pull)))
        ));
    }

    #[test]
    fn enter_on_a_branch_checks_it_out() {
        match type_and_press("@export", Modifiers::NONE) {
            Some(Pick::Checkout(branch)) => assert_eq!(branch.name, "feat/export"),
            _ => panic!("expected a checkout"),
        }
    }

    #[test]
    fn cmd_enter_on_a_branch_asks_for_its_actions() {
        match type_and_press("@export", Modifiers::COMMAND) {
            Some(Pick::BranchMenu(name)) => assert_eq!(name, "feat/export"),
            _ => panic!("expected the branch menu"),
        }
    }

    #[test]
    fn commit_search_finds_titles_and_stops_when_cancelled() {
        let dir = std::env::temp_dir().join(format!("kelp-palette-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| kelp_core::git_cli::run(&dir, args).unwrap();
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test"]);
        for (file, message) in [
            ("a", "feat: first thing"),
            ("b", "chore: release 0.2.0"),
            ("c", "fix: last thing"),
        ] {
            std::fs::write(dir.join(file), message).unwrap();
            git(&["add", "."]);
            git(&["commit", "-q", "-m", message]);
        }
        let (_, history) = kelp_core::history::History::open(&dir).unwrap();
        let hits = search_commits(&dir, history.ids(), "Release 0.2.0", &|| false);
        assert!(hits.iter().any(|h| h.title == "chore: release 0.2.0"));
        let prefix = &history.id(0).to_hex().to_string()[..8];
        let by_hash = search_commits(&dir, history.ids(), prefix, &|| false);
        assert_eq!(by_hash.first().map(|h| h.id), Some(history.id(0)));
        assert!(search_commits(&dir, history.ids(), "release", &|| true).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gestures_are_told_apart_from_keys() {
        assert!(is_gesture("Double-click a branch"));
        assert!(is_gesture("Drag a tab"));
        assert!(!is_gesture("Cmd+Shift+P"));
        assert!(!is_gesture("Cmd+1 ... Cmd+9"));
        assert!(!is_gesture("Right-click"));
    }

    #[test]
    fn recent_actions_keep_five_newest_first() {
        let mut palette = Palette::default();
        for id in ["a", "b", "c", "d", "e", "f", "b"] {
            palette.remember(id);
        }
        assert_eq!(palette.recent, ["b", "f", "e", "d", "c"]);
    }
}
