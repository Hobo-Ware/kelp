use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use eframe::egui::{
    self, Color32, FontId, Key, KeyboardShortcut, Modifiers, RichText, Sense, Ui, text::LayoutJob,
    vec2,
};
use kelp_core::pulls::Pull;
use kelp_core::ref_tree::{self, Node, Sort};
use kelp_core::refs::{RefKind, RefLabel};
use kelp_core::sidebar_prefs::{self, SidebarPrefs};

use crate::commands::Command;
use crate::icons::Icon;
use crate::menus::{self, MenuContext};
use crate::repo_view::{Repo, Selection};
use crate::theme;

const FOLDER_H: f32 = 26.0;
const INDENT: f32 = 14.0;
const FILTER_SHORTCUT: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::F);

pub struct State {
    common_dir: PathBuf,
    prefs: SidebarPrefs,
    filter: String,
    focus_filter: bool,
    trees: Option<(u64, Arc<Trees>)>,
    merged: Merged,
}

#[derive(Default)]
struct Merged {
    key: Option<u64>,
    names: HashSet<String>,
    pending: Option<Receiver<(u64, HashSet<String>)>>,
}

struct SectionTree {
    nodes: Vec<Node>,
    labels: Vec<usize>,
    highlights: Vec<Vec<Range<usize>>>,
    merged_hidden: usize,
}

struct Trees {
    local: SectionTree,
    remote: SectionTree,
    tag: SectionTree,
    pinned: Vec<(usize, Vec<Range<usize>>)>,
}

enum Action {
    ToggleFolder(String),
    TogglePin(String),
    Sort(RefKind, Sort),
    HideMerged(bool),
    CollapseAll(RefKind),
}

impl State {
    pub fn new(common_dir: &Path) -> Self {
        Self {
            common_dir: common_dir.to_path_buf(),
            prefs: SidebarPrefs::load(common_dir),
            filter: std::env::var("KELP_SIDEBAR_FILTER").unwrap_or_default(),
            focus_filter: false,
            trees: None,
            merged: Merged::default(),
        }
    }

    fn apply(&mut self, actions: Vec<Action>) {
        if actions.is_empty() {
            return;
        }
        for action in actions {
            match action {
                Action::ToggleFolder(key) => self.prefs.toggle_folder(&key),
                Action::TogglePin(name) => self.prefs.toggle_pin(&name),
                Action::Sort(kind, sort) => *self.sort_mut(kind) = sort,
                Action::HideMerged(on) => self.prefs.hide_merged = on,
                Action::CollapseAll(kind) => {
                    let prefix = format!("{}:", section_key(kind));
                    self.prefs.open_folders.retain(|k| !k.starts_with(&prefix));
                }
            }
        }
        self.trees = None;
        if !crate::settings::is_dev_run()
            && let Err(e) = self.prefs.save(&self.common_dir)
        {
            eprintln!("kelp: could not save sidebar settings: {e}");
        }
    }

    fn sort(&self, kind: RefKind) -> Sort {
        match kind {
            RefKind::Local => self.prefs.local_sort,
            RefKind::Remote => self.prefs.remote_sort,
            RefKind::Tag => self.prefs.tag_sort,
        }
    }

    fn sort_mut(&mut self, kind: RefKind) -> &mut Sort {
        match kind {
            RefKind::Local => &mut self.prefs.local_sort,
            RefKind::Remote => &mut self.prefs.remote_sort,
            RefKind::Tag => &mut self.prefs.tag_sort,
        }
    }

    fn refresh_merged(&mut self, ctx: &egui::Context, dir: &Path, refs_key: u64) {
        if let Some(rx) = &self.merged.pending
            && let Ok((key, names)) = rx.try_recv()
        {
            self.merged.pending = None;
            if self.merged.key == Some(key) {
                self.merged.names = names;
                self.trees = None;
            }
        }
        if !self.prefs.hide_merged
            || self.merged.key == Some(refs_key)
            || self.merged.pending.is_some()
        {
            return;
        }
        self.merged.key = Some(refs_key);
        let (tx, rx) = mpsc::channel();
        let dir = dir.to_path_buf();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let base = sidebar_prefs::merge_base_ref(&dir);
            let names = sidebar_prefs::merged_into(&dir, &base).unwrap_or_default();
            if tx.send((refs_key, names)).is_ok() {
                ctx.request_repaint();
            }
        });
        self.merged.pending = Some(rx);
    }

    fn trees(
        &mut self,
        repo: &gix::Repository,
        labels: &[RefLabel],
        head: Option<&str>,
    ) -> Arc<Trees> {
        let key = self.tree_key(labels);
        if let Some((cached, trees)) = &self.trees
            && *cached == key
        {
            return trees.clone();
        }
        let mut time_cache = std::collections::HashMap::new();
        let mut time_of = |label: &RefLabel| {
            *time_cache
                .entry(label.target)
                .or_insert_with(|| commit_time(repo, label.target))
        };
        let merged = self.prefs.hide_merged.then_some(&self.merged.names);
        let mut section = |kind: RefKind| {
            let sort = self.sort(kind);
            let mut indices = Vec::new();
            let mut highlights = Vec::new();
            let mut merged_hidden = 0;
            for (i, label) in labels.iter().enumerate().filter(|(_, l)| l.kind == kind) {
                let Some(ranges) = ref_tree::fuzzy(&label.name, &self.filter) else {
                    continue;
                };
                if kind == RefKind::Local
                    && let Some(merged) = merged
                    && merged.contains(&label.name)
                    && !label.is_head
                    && head != Some(label.name.as_str())
                    && !is_base_branch(&label.name)
                    && !self.prefs.pinned.contains(&label.full_name())
                {
                    merged_hidden += 1;
                    continue;
                }
                indices.push(i);
                highlights.push(ranges);
            }
            let names: Vec<(&str, i64)> = indices
                .iter()
                .map(|&i| {
                    let label = &labels[i];
                    let time = if sort == Sort::Newest {
                        time_of(label)
                    } else {
                        0
                    };
                    (label.name.as_str(), time)
                })
                .collect();
            SectionTree {
                nodes: ref_tree::build(&names, sort),
                labels: indices,
                highlights,
                merged_hidden,
            }
        };
        let local = section(RefKind::Local);
        let remote = section(RefKind::Remote);
        let tag = section(RefKind::Tag);
        let pinned = labels
            .iter()
            .enumerate()
            .filter(|(_, l)| self.prefs.pinned.contains(&l.full_name()))
            .filter_map(|(i, l)| ref_tree::fuzzy(&l.name, &self.filter).map(|r| (i, r)))
            .collect();
        let trees = Arc::new(Trees {
            local,
            remote,
            tag,
            pinned,
        });
        self.trees = Some((key, trees.clone()));
        trees
    }

    fn tree_key(&self, labels: &[RefLabel]) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        refs_key(labels).hash(&mut hasher);
        self.filter.trim().hash(&mut hasher);
        self.prefs.hash(&mut hasher);
        if self.prefs.hide_merged {
            let mut names: Vec<&String> = self.merged.names.iter().collect();
            names.sort();
            names.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn is_open(&self, kind: RefKind, path: &str) -> bool {
        !self.filter.trim().is_empty() || self.prefs.open_folders.contains(&folder_key(kind, path))
    }
}

fn refs_key(labels: &[RefLabel]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for label in labels {
        label.name.hash(&mut hasher);
        (label.kind as u8).hash(&mut hasher);
        label.target.hash(&mut hasher);
        label.is_head.hash(&mut hasher);
        label.hidden.hash(&mut hasher);
        label.row.hash(&mut hasher);
    }
    hasher.finish()
}

fn is_base_branch(name: &str) -> bool {
    matches!(name, "main" | "master")
}

fn commit_time(repo: &gix::Repository, id: gix::ObjectId) -> i64 {
    repo.find_commit(id)
        .ok()
        .and_then(|c| c.time().ok())
        .map_or(0, |t| t.seconds)
}

fn section_key(kind: RefKind) -> &'static str {
    match kind {
        RefKind::Local => "local",
        RefKind::Remote => "remote",
        RefKind::Tag => "tag",
    }
}

fn folder_key(kind: RefKind, path: &str) -> String {
    format!("{}:{path}", section_key(kind))
}

struct RefRows<'a> {
    labels: &'a [RefLabel],
    repo: &'a Repo,
    menu_ctx: &'a MenuContext,
    filtering_view: bool,
    pinned: &'a std::collections::BTreeSet<String>,
    commands: &'a mut Vec<Command>,
    actions: &'a mut Vec<Action>,
}

pub fn ui(ui: &mut Ui, repo: &mut Repo, commands: &mut Vec<Command>) {
    let mut actions = Vec::new();
    filter_box(ui, &mut repo.sidebar);
    let refs_key = refs_key(&repo.history.refs.labels);
    let dir = repo.dir.clone();
    repo.sidebar.refresh_merged(ui.ctx(), &dir, refs_key);
    let head = repo.current_branch().map(str::to_string);
    let trees = repo
        .sidebar
        .trees(&repo.repo, &repo.history.refs.labels, head.as_deref());
    let query = repo.sidebar.filter.trim().to_string();
    let menu_ctx = repo.menu_context();
    let view: &Repo = repo;
    let state = &view.sidebar;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(4.0);
            let mut rows = RefRows {
                labels: &view.history.refs.labels,
                repo: view,
                menu_ctx: &menu_ctx,
                filtering_view: view.view.is_filtering(),
                pinned: &state.prefs.pinned,
                commands,
                actions: &mut actions,
            };
            let worktrees: Vec<_> = view
                .workspace
                .worktrees
                .iter()
                .filter(|wt| {
                    let branch = wt.tree.branch.as_deref().unwrap_or("");
                    ref_tree::fuzzy(&format!("{} {branch}", wt.tree.name()), &query).is_some()
                })
                .collect();
            let mut manage = false;
            if query.is_empty() || !worktrees.is_empty() {
                section(
                    ui,
                    "WORKTREES",
                    worktrees.len(),
                    true,
                    |ui| {
                        manage_button(ui, &mut manage);
                    },
                    |ui| {
                        for wt in &worktrees {
                            worktree_row(ui, view, wt, &query, rows.commands);
                        }
                    },
                );
            }
            if manage {
                rows.commands.push(Command::ShowWorktrees);
            }

            if !trees.pinned.is_empty() {
                section(
                    ui,
                    "PINNED",
                    trees.pinned.len(),
                    true,
                    |_| {},
                    |ui| {
                        let labels = rows.labels;
                        for (index, ranges) in &trees.pinned {
                            rows.show(ui, *index, &labels[*index].name, ranges, 0.0, true);
                        }
                    },
                );
            }
            for (title, kind, tree) in [
                ("LOCAL", RefKind::Local, &trees.local),
                ("REMOTE", RefKind::Remote, &trees.remote),
                ("TAGS", RefKind::Tag, &trees.tag),
            ] {
                if !query.is_empty() && tree.labels.is_empty() {
                    continue;
                }
                let sort = state.sort(kind);
                let hide_merged = state.prefs.hide_merged;
                let header_actions = &mut Vec::new();
                section(
                    ui,
                    title,
                    tree.labels.len(),
                    kind != RefKind::Tag,
                    |ui| section_menu(ui, kind, sort, hide_merged, header_actions),
                    |ui| {
                        show_nodes(ui, &tree.nodes, 0, kind, tree, state, &mut rows);
                        if tree.merged_hidden > 0 {
                            merged_note(ui, tree.merged_hidden, rows.actions);
                        }
                    },
                );
                rows.actions.append(header_actions);
            }

            let stashes: Vec<_> = view
                .workspace
                .stashes
                .iter()
                .filter(|s| ref_tree::fuzzy(&format!("{} {}", s.message, s.name), &query).is_some())
                .collect();
            if query.is_empty() || !stashes.is_empty() {
                section(
                    ui,
                    "STASHES",
                    stashes.len(),
                    false,
                    |_| {},
                    |ui| {
                        for stash in &stashes {
                            let highlight =
                                ref_tree::fuzzy(&stash.message, &query).unwrap_or_default();
                            let response = Row {
                                name: &stash.message,
                                highlight: &highlight,
                                subtitle: Some(&stash.name),
                                ..Row::new(theme::TEXT_FAINT)
                            }
                            .show(ui);
                            response.context_menu(|ui| menus::stash(ui, stash, rows.commands));
                        }
                    },
                );
            }
            if !query.is_empty()
                && trees.local.labels.is_empty()
                && trees.remote.labels.is_empty()
                && trees.tag.labels.is_empty()
                && worktrees.is_empty()
                && stashes.is_empty()
            {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new(format!("Nothing matches \"{query}\""))
                            .size(12.0)
                            .color(theme::TEXT_FAINT),
                    );
                });
            }
        });
    repo.sidebar.apply(actions);
}

fn filter_box(ui: &mut Ui, state: &mut State) {
    let focus = ui.input_mut(|i| i.consume_shortcut(&FILTER_SHORTCUT));
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 10,
            right: 10,
            top: 10,
            bottom: 4,
        })
        .show(ui, |ui| {
            let id = egui::Id::new("sidebar-filter");
            let edit = egui::TextEdit::singleline(&mut state.filter)
                .id(id)
                .hint_text("Filter branches, tags, worktrees")
                .desired_width(f32::INFINITY);
            let response = ui.add(edit);
            if focus || state.focus_filter {
                response.request_focus();
                state.focus_filter = false;
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                state.filter.clear();
            }
            if !state.filter.is_empty() && clear_button(ui, response.rect) {
                state.filter.clear();
            }
        });
}

fn clear_button(ui: &mut Ui, field: egui::Rect) -> bool {
    let rect = egui::Rect::from_center_size(
        egui::pos2(field.right() - 12.0, field.center().y),
        vec2(16.0, 16.0),
    );
    let response = ui
        .interact(rect, egui::Id::new("sidebar-filter-clear"), Sense::click())
        .on_hover_text("Clear the filter (Esc)")
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let color = if response.hovered() {
        theme::TEXT_STRONG
    } else {
        theme::TEXT_FAINT
    };
    let c = rect.center();
    let stroke = egui::Stroke::new(1.3, color);
    ui.painter()
        .line_segment([c + vec2(-3.5, -3.5), c + vec2(3.5, 3.5)], stroke);
    ui.painter()
        .line_segment([c + vec2(-3.5, 3.5), c + vec2(3.5, -3.5)], stroke);
    response.clicked()
}

fn section_menu(
    ui: &mut Ui,
    kind: RefKind,
    sort: Sort,
    hide_merged: bool,
    actions: &mut Vec<Action>,
) {
    let (rect, response) = ui.allocate_exact_size(vec2(22.0, 18.0), Sense::click());
    let color = if response.hovered() {
        theme::TEXT_STRONG
    } else {
        theme::TEXT_FAINT
    };
    for dx in [-5.0, 0.0, 5.0] {
        ui.painter()
            .circle_filled(rect.center() + vec2(dx, 0.0), 1.4, color);
    }
    let response = response
        .on_hover_text("Sort and filter")
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let forced = kind == RefKind::Local && std::env::var("KELP_OPEN_MENU").as_deref() == Ok("sort");
    let popup = egui::Popup::menu(&response);
    let popup = if forced { popup.open(true) } else { popup };
    popup.show(|ui| {
        ui.set_min_width(220.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        let check = |on: bool| on.then_some(Icon::Check);
        if menus::row(ui, check(sort == Sort::Name), "Sort by name", None, false) {
            actions.push(Action::Sort(kind, Sort::Name));
        }
        if menus::row(
            ui,
            check(sort == Sort::Newest),
            "Sort by last commit",
            None,
            false,
        ) {
            actions.push(Action::Sort(kind, Sort::Newest));
        }
        menus::separator(ui);
        if kind == RefKind::Local
            && menus::row(ui, check(hide_merged), "Hide merged branches", None, false)
        {
            actions.push(Action::HideMerged(!hide_merged));
        }
        if menus::row(ui, Some(Icon::Minus), "Collapse all folders", None, false) {
            actions.push(Action::CollapseAll(kind));
        }
    });
}

fn manage_button(ui: &mut Ui, manage: &mut bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        "Manage".to_string(),
        FontId::proportional(11.0),
        theme::ACCENT,
    );
    let size = vec2(galley.size().x + 14.0, 20.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = response
        .on_hover_text("Open the worktrees page")
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 4.0, theme::with_alpha(theme::ACCENT, 0x22));
    }
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, theme::ACCENT);
    if response.clicked() {
        *manage = true;
    }
    response
}

fn merged_note(ui: &mut Ui, count: usize, actions: &mut Vec<Action>) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    let text = format!(
        "{count} merged {} hidden · Show",
        if count == 1 { "branch" } else { "branches" }
    );
    let color = if response.hovered() {
        theme::TEXT
    } else {
        theme::TEXT_FAINT
    };
    ui.painter().text(
        rect.left_center() + vec2(24.0, 0.0),
        egui::Align2::LEFT_CENTER,
        text,
        FontId::proportional(11.5),
        color,
    );
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        actions.push(Action::HideMerged(false));
    }
}

fn show_nodes(
    ui: &mut Ui,
    nodes: &[Node],
    depth: usize,
    kind: RefKind,
    tree: &SectionTree,
    state: &State,
    rows: &mut RefRows<'_>,
) {
    let indent = depth as f32 * INDENT;
    let labels = rows.labels;
    for node in nodes {
        match node {
            Node::Leaf(leaf) => {
                let index = tree.labels[leaf.index];
                let label = &labels[index];
                let offset = label.name.len() - leaf.label.len();
                let ranges: Vec<Range<usize>> = tree.highlights[leaf.index]
                    .iter()
                    .filter(|r| r.end > offset)
                    .map(|r| r.start.max(offset) - offset..r.end - offset)
                    .collect();
                rows.show(ui, index, &leaf.label, &ranges, indent, false);
            }
            Node::Folder(folder) => {
                let open = state.is_open(kind, &folder.path);
                let leaves: Vec<&RefLabel> = node
                    .leaves()
                    .into_iter()
                    .map(|i| &labels[tree.labels[i]])
                    .collect();
                let has_head = leaves.iter().any(|l| l.is_head);
                let pending = kind == RefKind::Local
                    && leaves.iter().any(|l| {
                        rows.repo
                            .workspace
                            .ahead_behind
                            .get(&l.name)
                            .is_some_and(|(a, b)| *a > 0 || *b > 0)
                    });
                if folder_row(
                    ui,
                    &folder.label,
                    folder.count,
                    indent,
                    open,
                    has_head,
                    pending,
                ) {
                    rows.actions
                        .push(Action::ToggleFolder(folder_key(kind, &folder.path)));
                }
                if open {
                    show_nodes(ui, &folder.children, depth + 1, kind, tree, state, rows);
                }
            }
        }
    }
}

fn folder_row(
    ui: &mut Ui,
    label: &str,
    count: usize,
    indent: f32,
    open: bool,
    has_head: bool,
    pending: bool,
) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), FOLDER_H), Sense::click());
    let painter = ui.painter_at(rect);
    if response.hovered() {
        painter.rect_filled(rect, 0.0, theme::with_alpha(Color32::WHITE, 0x08));
    }
    let x = rect.left() + 12.0 + indent;
    let c = egui::pos2(x, rect.center().y);
    let stroke = egui::Stroke::new(1.4, theme::TEXT_FAINT);
    let chevron = if open {
        [
            c + vec2(-3.5, -1.5),
            c + vec2(0.0, 2.0),
            c + vec2(3.5, -1.5),
        ]
    } else {
        [
            c + vec2(-1.5, -3.5),
            c + vec2(2.0, 0.0),
            c + vec2(-1.5, 3.5),
        ]
    };
    painter.add(egui::Shape::line(chevron.to_vec(), stroke));
    let color = if has_head {
        theme::TEXT_STRONG
    } else {
        theme::TEXT_MUTED
    };
    let count_galley = painter.layout_no_wrap(
        count.to_string(),
        FontId::proportional(11.0),
        theme::TEXT_FAINT,
    );
    let right = rect.right() - 12.0 - count_galley.size().x;
    let name = crate::graph_view::truncated(
        &painter,
        format!("{label}/"),
        FontId::new(12.5, theme::semibold()),
        color,
        (right - x - 24.0).max(0.0),
    );
    painter.galley(
        egui::pos2(x + 12.0, rect.center().y - name.size().y / 2.0),
        name,
        color,
    );
    painter.galley(
        egui::pos2(right, rect.center().y - count_galley.size().y / 2.0),
        count_galley,
        theme::TEXT_FAINT,
    );
    let marker = egui::pos2(right - 10.0, rect.center().y);
    if has_head {
        painter.circle_filled(marker, 3.0, theme::ACCENT);
    } else if pending {
        painter.circle_stroke(marker, 2.5, egui::Stroke::new(1.2, theme::ACCENT));
    }
    let hint = match (has_head, pending) {
        (true, _) => "Your current branch is in here",
        (false, true) => "Some branches in here are ahead or behind",
        _ => "",
    };
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let response = if hint.is_empty() {
        response
    } else {
        response.on_hover_text(hint)
    };
    response.clicked()
}

impl RefRows<'_> {
    fn show(
        &mut self,
        ui: &mut Ui,
        index: usize,
        display: &str,
        highlight: &[Range<usize>],
        indent: f32,
        in_pinned: bool,
    ) {
        let label = &self.labels[index];
        let kind = label.kind;
        let history = &self.repo.history;
        let selected = label.row.map(|r| Selection::Commit(r as usize)) == self.repo.selected;
        let dot = label
            .row
            .filter(|_| !label.hidden)
            .map(|r| theme::lane(history.layout.node_color(r as usize)))
            .unwrap_or(theme::TEXT_FAINT);
        let badge = match self.repo.workspace.ahead_behind.get(&label.name) {
            Some((a, b)) if kind == RefKind::Local && (*a > 0 || *b > 0) => {
                let mut s = String::new();
                if *a > 0 {
                    s.push_str(&format!("↑{a}"));
                }
                if *b > 0 {
                    s.push_str(&format!(" ↓{b}"));
                }
                Some(s.trim().to_string())
            }
            _ => None,
        };
        let tag = if label.is_head {
            Some("HEAD".to_string())
        } else {
            badge
        };
        let full_name = label.full_name();
        let pinned = self.pinned.contains(&full_name);
        let mut pull_clicked = false;
        let response = Row {
            pull: label.pull.as_ref().map(|pull| (pull, &mut pull_clicked)),
            name: display,
            highlight,
            strong: label.is_head,
            selected,
            tag: tag.as_deref(),
            dim: label.hidden,
            indent,
            pinned: pinned && !in_pinned,
            ..Row::new(dot)
        }
        .show(ui);
        if pull_clicked && let Some(pull) = &label.pull {
            self.commands.push(Command::OpenUrl(pull.url.clone()));
        }
        if !label.is_head
            && (label.hidden || ui.rect_contains_pointer(response.rect))
            && eye_toggle(ui, &response, label.hidden)
        {
            self.commands.push(Command::ToggleRef(full_name.clone()));
        }
        if response.clicked()
            && let Some(r) = label.row
        {
            self.commands
                .push(Command::Reveal(Selection::Commit(r as usize)));
        }
        if response.double_clicked() && kind != RefKind::Tag {
            if kind == RefKind::Local && !label.is_head {
                self.commands
                    .push(Command::Run(kelp_core::ops::Op::Switch(label.name.clone())));
            } else if kind == RefKind::Remote {
                self.commands
                    .push(Command::Run(kelp_core::ops::Op::SwitchTrack(
                        label.name.clone(),
                    )));
            }
        }
        let forced = kind == RefKind::Local
            && label.is_head
            && !in_pinned
            && std::env::var("KELP_OPEN_MENU").as_deref() == Ok("branch");
        let menu_ctx = self.menu_ctx;
        let filtering_view = self.filtering_view;
        let commands = &mut *self.commands;
        let actions = &mut *self.actions;
        let menu = |ui: &mut Ui| {
            menus::branch(ui, label, menu_ctx, commands);
            menus::view_items(ui, label, filtering_view, commands);
            if kind != RefKind::Tag {
                menus::separator(ui);
                let text = if pinned { "Unpin" } else { "Pin to top" };
                if menus::row(ui, Some(Icon::Pin), text, None, false) {
                    actions.push(Action::TogglePin(full_name.clone()));
                }
            }
        };
        if forced {
            egui::Popup::from_response(&response).open(true).show(menu);
        } else {
            response.context_menu(menu);
        }
    }
}

fn worktree_row(
    ui: &mut Ui,
    repo: &Repo,
    wt: &crate::repo_view::WorktreeRow,
    query: &str,
    commands: &mut Vec<Command>,
) {
    let current = wt.current;
    let branch = wt.tree.branch.clone().unwrap_or_else(|| "detached".into());
    let sub = match wt.changes {
        Some(0) | None if current => format!("{branch} · current"),
        Some(0) | None => branch,
        Some(n) => format!("{branch} · {n} changed"),
    };
    let dot = repo
        .history
        .refs
        .of_kind(RefKind::Local)
        .find(|l| Some(&l.name) == wt.tree.branch.as_ref())
        .and_then(|l| l.row)
        .map(|r| theme::lane(repo.history.layout.node_color(r as usize)))
        .unwrap_or(theme::TEXT_FAINT);
    let name = wt.tree.name();
    let highlight = ref_tree::fuzzy(&name, query).unwrap_or_default();
    let response = Row {
        name: &name,
        highlight: &highlight,
        subtitle: Some(&sub),
        strong: current,
        ..Row::new(dot)
    }
    .show(ui);
    if response.double_clicked() && !current {
        commands.push(Command::OpenRepo(wt.tree.path.clone()));
    } else if response.clicked() {
        commands.push(Command::ShowWorktrees);
    }
    response.context_menu(|ui| menus::worktree(ui, &wt.tree, commands));
}

fn section(
    ui: &mut Ui,
    title: &str,
    count: usize,
    open_by_default: bool,
    header_right: impl FnOnce(&mut Ui),
    body: impl FnOnce(&mut Ui),
) {
    let id = ui.make_persistent_id(title);
    let state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        id,
        open_by_default,
    );
    let mut title_clicked = false;
    let mut header = state.show_header(ui, |ui| {
        let title = ui
            .add(
                egui::Label::new(
                    RichText::new(title)
                        .size(11.0)
                        .family(theme::semibold())
                        .color(theme::TEXT_MUTED),
                )
                .selectable(false)
                .sense(Sense::click()),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        title_clicked = title.clicked();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(12.0);
            ui.label(
                RichText::new(count.to_string())
                    .size(11.0)
                    .color(theme::TEXT_MUTED),
            );
            header_right(ui);
        });
    });
    if title_clicked {
        header.toggle();
    }
    header.body(body);
    ui.add_space(6.0);
}

fn eye_toggle(ui: &mut Ui, row: &egui::Response, hidden: bool) -> bool {
    let rect = egui::Rect::from_center_size(
        egui::pos2(row.rect.right() - 20.0, row.rect.center().y),
        vec2(22.0, 22.0),
    );
    let response = ui
        .interact(rect, row.id.with("eye"), Sense::click())
        .on_hover_text(if hidden {
            "Show in the graph"
        } else {
            "Hide from the graph"
        })
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();
    let backdrop = if response.hovered() {
        theme::CONTROL_HOVER
    } else {
        theme::PANEL
    };
    painter.rect_filled(rect.expand2(vec2(10.0, 0.0)), 4.0, backdrop);
    let color = if response.hovered() {
        theme::TEXT_STRONG
    } else {
        theme::TEXT_MUTED
    };
    let icon = if hidden { Icon::EyeOff } else { Icon::Eye };
    crate::icons::paint(
        painter,
        crate::icons::center_square(rect, 15.0),
        icon,
        color,
    );
    response.clicked()
}

struct Row<'a> {
    name: &'a str,
    highlight: &'a [Range<usize>],
    subtitle: Option<&'a str>,
    dot: Color32,
    strong: bool,
    selected: bool,
    tag: Option<&'a str>,
    dim: bool,
    indent: f32,
    pinned: bool,
    pull: Option<(&'a Pull, &'a mut bool)>,
}

impl<'a> Row<'a> {
    fn new(dot: Color32) -> Self {
        Self {
            name: "",
            highlight: &[],
            subtitle: None,
            dot,
            strong: false,
            selected: false,
            tag: None,
            dim: false,
            indent: 0.0,
            pinned: false,
            pull: None,
        }
    }

    fn show(self, ui: &mut Ui) -> egui::Response {
        let height = if self.subtitle.is_some() { 40.0 } else { 28.0 };
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
        let painter = ui.painter_at(rect);
        if self.selected {
            painter.rect_filled(rect, 0.0, theme::SIDEBAR_SELECTED);
        } else if response.hovered() {
            painter.rect_filled(rect, 0.0, theme::with_alpha(Color32::WHITE, 0x08));
        }
        let name_y = if self.subtitle.is_some() {
            rect.top() + 13.0
        } else {
            rect.center().y
        };
        let left = rect.left() + self.indent;
        painter.circle_filled(egui::pos2(left + 12.0, name_y), 4.0, self.dot);
        let color = if self.selected || self.strong {
            theme::TEXT_STRONG
        } else if self.dim {
            theme::TEXT_FAINT
        } else {
            Color32::from_rgb(0xd5, 0xd7, 0xdc)
        };
        let font = FontId::proportional(if self.strong { 13.5 } else { 13.0 });
        let tag_galley = self
            .tag
            .map(|t| painter.layout_no_wrap(t.to_string(), FontId::proportional(11.0), self.dot));
        let pin_w = if self.pinned { 18.0 } else { 0.0 };
        let pill_w = self.pull.as_ref().map_or(0.0, |(pull, _)| {
            crate::pulls_ui::pill_width(&painter, pull) + 6.0
        });
        let reserved = tag_galley.as_ref().map_or(12.0, |g| g.size().x + 20.0) + pin_w + pill_w;
        let max_width = (rect.right() - left - 24.0 - reserved).max(0.0);
        let mut job = highlighted(self.name, self.highlight, font, color);
        job.wrap = egui::text::TextWrapping::truncate_at_width(max_width);
        let galley = painter.layout_job(job);
        let name_right = left + 24.0 + galley.size().x;
        painter.galley(
            egui::pos2(left + 24.0, name_y - galley.size().y / 2.0),
            galley,
            color,
        );
        if let Some((pull, clicked)) = self.pull {
            let pill = egui::Rect::from_min_size(
                egui::pos2(name_right + 6.0, name_y - crate::pulls_ui::PILL_H / 2.0),
                vec2(pill_w - 6.0, crate::pulls_ui::PILL_H),
            );
            let pill_response = ui
                .interact(pill, response.id.with("pull"), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_ui(|ui| crate::pulls_ui::tooltip(ui, pull));
            crate::pulls_ui::paint_pill(&painter, pill, pull, pill_response.hovered());
            *clicked = pill_response.clicked();
        }
        if let Some(sub) = self.subtitle {
            let sub = crate::graph_view::truncated(
                &painter,
                sub.to_string(),
                FontId::proportional(11.0),
                theme::TEXT_FAINT,
                rect.right() - left - 36.0,
            );
            painter.galley(
                egui::pos2(left + 24.0, rect.top() + 22.0),
                sub,
                theme::TEXT_FAINT,
            );
        }
        let mut right = rect.right() - 12.0;
        if let Some(g) = tag_galley {
            let tag_color = if self.strong {
                self.dot
            } else {
                theme::TEXT_FAINT
            };
            right -= g.size().x;
            painter.galley(egui::pos2(right, name_y - g.size().y / 2.0), g, tag_color);
            right -= 6.0;
        }
        if self.pinned {
            let pin =
                egui::Rect::from_center_size(egui::pos2(right - 7.0, name_y), vec2(12.0, 12.0));
            crate::icons::paint(&painter, pin, Icon::Pin, theme::TEXT_FAINT);
        }
        response
    }
}

fn highlighted(text: &str, ranges: &[Range<usize>], font: FontId, color: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let plain = egui::TextFormat::simple(font.clone(), color);
    let mark = egui::TextFormat {
        color: theme::ACCENT,
        background: theme::with_alpha(theme::ACCENT, 0x24),
        ..egui::TextFormat::simple(font, theme::ACCENT)
    };
    let mut at = 0;
    for range in ranges {
        let (start, end) = (range.start.min(text.len()), range.end.min(text.len()));
        if start > at {
            job.append(&text[at..start], 0.0, plain.clone());
        }
        if end > start {
            job.append(&text[start..end], 0.0, mark.clone());
        }
        at = at.max(end);
    }
    if at < text.len() {
        job.append(&text[at..], 0.0, plain);
    }
    job
}

#[cfg(test)]
mod tests {
    use eframe::egui::{self, Event, PointerButton, Pos2, RawInput, Rect, vec2};

    use super::{highlighted, manage_button, section};

    struct Probe {
        body_shown: bool,
        manage: bool,
        title: Rect,
        manage_rect: Rect,
    }

    fn frame(ctx: &egui::Context, probe: &mut Probe, events: Vec<Event>) {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(300.0, 200.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run_ui(input, |ui| {
            probe.body_shown = false;
            let top = ui.cursor().top();
            section(
                ui,
                "WORKTREES",
                3,
                true,
                |ui| probe.manage_rect = manage_button(ui, &mut probe.manage).rect,
                |ui| {
                    probe.body_shown = true;
                    ui.label("body");
                },
            );
            probe.title = Rect::from_min_size(egui::pos2(30.0, top), vec2(40.0, 18.0));
        });
    }

    fn click(ctx: &egui::Context, probe: &mut Probe, at: Pos2) {
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(ctx, probe, vec![Event::PointerMoved(at)]);
        frame(ctx, probe, vec![press(true)]);
        frame(ctx, probe, vec![press(false)]);
        for _ in 0..30 {
            frame(ctx, probe, vec![]);
        }
    }

    #[test]
    fn clicking_the_title_collapses_and_manage_fires() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut probe = Probe {
            body_shown: false,
            manage: false,
            title: Rect::NOTHING,
            manage_rect: Rect::NOTHING,
        };
        frame(&ctx, &mut probe, vec![]);
        assert!(probe.body_shown);
        let title = probe.title.center();
        click(&ctx, &mut probe, title);
        assert!(!probe.body_shown, "title click should collapse the section");
        click(&ctx, &mut probe, title);
        assert!(
            probe.body_shown,
            "second title click should expand it again"
        );
        let manage = probe.manage_rect.center();
        click(&ctx, &mut probe, manage);
        assert!(probe.manage);
        assert!(probe.body_shown, "Manage must not toggle the section");
    }
    use eframe::egui::{Color32, FontId};

    #[test]
    fn highlight_splits_text_into_marked_sections() {
        let marks: Vec<_> = std::iter::once(7..12).collect();
        let job = highlighted("feat/delighters", &marks, FontId::default(), Color32::WHITE);
        let parts: Vec<&str> = job
            .sections
            .iter()
            .map(|s| &job.text[s.byte_range.start.0..s.byte_range.end.0])
            .collect();
        assert_eq!(parts, ["feat/de", "light", "ers"]);
    }
}
