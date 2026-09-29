use eframe::egui::{Key, KeyboardShortcut, Modifiers};

use crate::help::Page;
use crate::icons::Icon;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Run {
    App(AppAction),
    Repo(RepoAction),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppAction {
    NewTab,
    OpenRepo,
    Clone,
    NewRepository,
    ToggleSidebar,
    ToggleDetails,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    CloseTab,
    NextTab,
    PreviousTab,
    Settings,
    Shortcuts,
    CheckUpdates,
    WhatsNew,
    Help(Page),
    ToggleDescriptions,
    ToggleFade,
    ToggleAuthorColumn,
    ToggleDateColumn,
    ToggleHashColumn,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RepoAction {
    Fetch,
    Pull,
    Push,
    NewBranch,
    NewWorktree,
    Stash,
    Pop,
    Undo,
    Redo,
    Refresh,
    Search,
    ShowGraph,
    ShowWorktrees,
    ShowReflog,
    ShowConsole,
    ShowChanges,
    StageAll,
    UnstageAll,
    ShowAllBranches,
    CheckoutCommit,
    CherryPick,
    Revert,
    InteractiveRebase,
    EditMessage,
    CopyCommitHash,
    CopyBranchName,
    OpenTerminal,
    RevealRepo,
    OpenPullRequest,
    CreatePullRequest,
    ShowPulls,
    RenameBranch,
    CreateTag,
    PushTags,
    AddRemote,
    FileHistory,
    Blame,
    FilterCommits,
    CompareWithHead,
    CompareWithWorkTree,
    StopCompare,
    ResetSoft,
    ResetMixed,
    ResetHard,
    NextChange,
    PreviousChange,
    OpenFileInEditor,
    RevealFile,
    CopyFilePath,
    ShowLatestStash,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Navigation,
    Tabs,
    Diff,
    Actions,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Navigation, Group::Tabs, Group::Diff, Group::Actions];

    pub fn title(self) -> &'static str {
        match self {
            Group::Navigation => "Navigation",
            Group::Tabs => "Tabs",
            Group::Diff => "Diff",
            Group::Actions => "Actions",
        }
    }
}

pub struct Action {
    pub id: &'static str,
    pub title: &'static str,
    pub keywords: &'static str,
    pub shortcut: Option<&'static str>,
    pub icon: Option<Icon>,
    pub run: Run,
}

const fn app(
    id: &'static str,
    title: &'static str,
    keywords: &'static str,
    shortcut: Option<&'static str>,
    icon: Option<Icon>,
    action: AppAction,
) -> Action {
    Action {
        id,
        title,
        keywords,
        shortcut,
        icon,
        run: Run::App(action),
    }
}

const fn repo(
    id: &'static str,
    title: &'static str,
    keywords: &'static str,
    shortcut: Option<&'static str>,
    icon: Option<Icon>,
    action: RepoAction,
) -> Action {
    Action {
        id,
        title,
        keywords,
        shortcut,
        icon,
        run: Run::Repo(action),
    }
}

#[rustfmt::skip]
pub static ACTIONS: &[Action] = &[
    repo("fetch", "Fetch", "remote download update", Some("Cmd+Shift+R"), Some(Icon::Fetch), RepoAction::Fetch),
    repo("pull", "Pull", "merge remote update", Some("Cmd+Shift+L"), Some(Icon::Pull), RepoAction::Pull),
    repo("push", "Push", "upload remote publish", Some("Cmd+Shift+U"), Some(Icon::Push), RepoAction::Push),
    repo("new-branch", "New branch…", "create branch", Some("Cmd+Shift+B"), Some(Icon::Branch), RepoAction::NewBranch),
    repo("new-worktree", "New worktree…", "create worktree", None, Some(Icon::Worktree), RepoAction::NewWorktree),
    repo("stash", "Stash all changes", "save shelve", Some("Cmd+Shift+S"), Some(Icon::Stash), RepoAction::Stash),
    repo("pop", "Pop the latest stash", "apply unstash", None, Some(Icon::Pop), RepoAction::Pop),
    repo("undo", "Undo", "revert back", Some("Cmd+Z"), Some(Icon::Undo), RepoAction::Undo),
    repo("redo", "Redo", "again", Some("Cmd+Shift+Z"), None, RepoAction::Redo),
    repo("refresh", "Refresh", "reload status", Some("Cmd+R"), None, RepoAction::Refresh),
    repo("search", "Search commits", "find message author hash", Some("Cmd+F"), None, RepoAction::Search),
    repo("filter-commits", "Filter commits by author, path or date", "funnel narrow mine who when", Some("Cmd+Shift+F"), Some(Icon::Filter), RepoAction::FilterCommits),
    repo("compare-with-head", "Compare the selected commit with the current branch", "diff range between", None, Some(Icon::Compare), RepoAction::CompareWithHead),
    repo("compare-with-worktree", "Compare the selected commit with the working tree", "diff range uncommitted", None, Some(Icon::Compare), RepoAction::CompareWithWorkTree),
    repo("stop-compare", "Stop comparing", "close clear", None, None, RepoAction::StopCompare),
    repo("show-graph", "Go to the graph", "history back close", Some("Esc"), None, RepoAction::ShowGraph),
    repo("show-worktrees", "Manage worktrees", "worktree list", None, Some(Icon::Worktree), RepoAction::ShowWorktrees),
    repo("show-reflog", "Show reflog", "history lost recover restore undo reset", None, Some(Icon::Undo), RepoAction::ShowReflog),
    repo("git-console", "Git console", "log commands output errors debug", Some("Cmd+Opt+L"), Some(Icon::Terminal), RepoAction::ShowConsole),
    repo("show-changes", "Show uncommitted changes", "wip staging working tree commit", Some("Cmd+Shift+C"), None, RepoAction::ShowChanges),
    repo("stage-all", "Stage all changes", "add index", None, Some(Icon::Plus), RepoAction::StageAll),
    repo("unstage-all", "Unstage all changes", "reset index", None, Some(Icon::Minus), RepoAction::UnstageAll),
    repo("show-all-branches", "Show all branches", "unhide hidden solo filter", None, Some(Icon::Eye), RepoAction::ShowAllBranches),
    repo("checkout-commit", "Check out the selected commit", "detached switch", None, Some(Icon::Check), RepoAction::CheckoutCommit),
    repo("cherry-pick", "Cherry-pick the selected commit", "apply copy", None, Some(Icon::CherryPick), RepoAction::CherryPick),
    repo("revert", "Revert the selected commit", "undo inverse", None, Some(Icon::Revert), RepoAction::Revert),
    repo("interactive-rebase", "Interactive rebase from the selected commit…", "squash reword reorder fixup drop", None, Some(Icon::Rebase), RepoAction::InteractiveRebase),
    repo("edit-message", "Edit the selected commit's message…", "reword rename amend", None, Some(Icon::Pencil), RepoAction::EditMessage),
    repo("copy-hash", "Copy the selected commit hash", "sha id clipboard", None, Some(Icon::Copy), RepoAction::CopyCommitHash),
    repo("copy-branch", "Copy the current branch name", "clipboard", None, Some(Icon::Copy), RepoAction::CopyBranchName),
    repo("open-pull-request", "Open the pull request for this branch", "pr github review", None, Some(Icon::Merge), RepoAction::OpenPullRequest),
    repo("show-pulls", "Show pull requests", "pr github review list checkout", None, Some(Icon::Merge), RepoAction::ShowPulls),
    repo("create-pull-request", "Create a pull request", "pr github new", None, Some(Icon::Merge), RepoAction::CreatePullRequest),
    repo("open-terminal", "Open in Terminal", "shell console", None, Some(Icon::Terminal), RepoAction::OpenTerminal),
    repo("reveal-repo", "Reveal the repository in Finder", "folder show", None, Some(Icon::Folder), RepoAction::RevealRepo),
    repo("rename-branch", "Rename the current branch…", "branch name move", None, Some(Icon::Pencil), RepoAction::RenameBranch),
    repo("create-tag", "Create a tag on the selected commit…", "tag release version", None, Some(Icon::Tag), RepoAction::CreateTag),
    repo("push-tags", "Push all tags", "tag release upload", None, Some(Icon::Push), RepoAction::PushTags),
    repo("add-remote", "Add a remote…", "remote origin upstream fork url", None, Some(Icon::Plus), RepoAction::AddRemote),
    repo("file-history", "File history of the open file", "log commits changes over time", None, Some(Icon::Clock), RepoAction::FileHistory),
    repo("blame", "Blame the open file", "annotate who wrote line author", None, Some(Icon::Commit), RepoAction::Blame),
    repo("reset-soft", "Reset the current branch to the selected commit, keeping changes staged", "soft move back", None, Some(Icon::Reset), RepoAction::ResetSoft),
    repo("reset-mixed", "Reset the current branch to the selected commit, keeping changes unstaged", "mixed move back", None, Some(Icon::Reset), RepoAction::ResetMixed),
    repo("reset-hard", "Reset the current branch to the selected commit, discarding changes…", "hard move back throw away", None, Some(Icon::Reset), RepoAction::ResetHard),
    repo("next-change", "Next change in the diff", "hunk jump down", Some("Alt+Down"), None, RepoAction::NextChange),
    repo("previous-change", "Previous change in the diff", "hunk jump up", Some("Alt+Up"), None, RepoAction::PreviousChange),
    repo("open-file-in-editor", "Open the open file in your editor", "edit code", None, Some(Icon::Pencil), RepoAction::OpenFileInEditor),
    repo("reveal-file", "Reveal the open file in Finder", "folder show", None, Some(Icon::Folder), RepoAction::RevealFile),
    repo("copy-file-path", "Copy the open file's path", "clipboard", None, Some(Icon::Copy), RepoAction::CopyFilePath),
    repo("show-latest-stash", "Show the latest stash", "stash view changes", None, Some(Icon::Stash), RepoAction::ShowLatestStash),
    app("new-tab", "New tab", "recent repositories welcome home", Some("Cmd+T"), None, AppAction::NewTab),
    app("open-repo", "Open a folder…", "repository open folder", Some("Cmd+O"), Some(Icon::Folder), AppAction::OpenRepo),
    app("clone", "Clone a repository…", "download url remote", None, None, AppAction::Clone),
    app("new-repository", "New repository…", "init create empty", None, Some(Icon::Plus), AppAction::NewRepository),
    app("toggle-sidebar", "Show or hide the sidebar", "panel collapse branches", Some("Cmd+Opt+S"), None, AppAction::ToggleSidebar),
    app("toggle-details", "Show or hide the details panel", "panel collapse commit", Some("Cmd+Opt+D"), None, AppAction::ToggleDetails),
    app("zoom-in", "Zoom in", "bigger larger text scale", Some("Cmd+Plus"), None, AppAction::ZoomIn),
    app("zoom-out", "Zoom out", "smaller text scale", Some("Cmd+Minus"), None, AppAction::ZoomOut),
    app("zoom-reset", "Reset zoom", "actual size 100 text scale", Some("Cmd+0"), None, AppAction::ZoomReset),
    app("close-tab", "Close the tab", "close", Some("Cmd+W"), None, AppAction::CloseTab),
    app("next-tab", "Next tab", "switch", Some("Ctrl+Tab"), None, AppAction::NextTab),
    app("previous-tab", "Previous tab", "switch back", Some("Ctrl+Shift+Tab"), None, AppAction::PreviousTab),
    app("settings", "Settings", "preferences options", Some("Cmd+,"), None, AppAction::Settings),
    app("shortcuts", "Keyboard shortcuts", "keys help", Some("Cmd+/"), None, AppAction::Shortcuts),
    app("check-updates", "Check for updates", "upgrade version", None, None, AppAction::CheckUpdates),
    app("whats-new", "What's new in Kelp", "changelog release notes version", None, None, AppAction::WhatsNew),
    app("help-getting-started", "Help: Getting started", "docs guide manual website", None, None, AppAction::Help(Page::GettingStarted)),
    app("help-shortcuts", "Help: Keyboard shortcuts page", "docs keys website", None, None, AppAction::Help(Page::Shortcuts)),
    app("help-undo", "Help: What undo covers", "docs reflog website", None, None, AppAction::Help(Page::Undo)),
    app("help-faq", "Help: FAQ", "docs questions privacy website", None, None, AppAction::Help(Page::Faq)),
    app("help-changelog", "Help: Changelog", "docs release notes website", None, None, AppAction::Help(Page::Changelog)),
    app("report-bug", "Report a bug", "issue github feedback problem", None, None, AppAction::Help(Page::ReportBug)),
    app("toggle-descriptions", "Toggle commit descriptions", "view body", None, None, AppAction::ToggleDescriptions),
    app("toggle-fade", "Toggle fading outside the selected history", "view dim", None, None, AppAction::ToggleFade),
    app("toggle-author-column", "Toggle the Author column", "view columns", None, None, AppAction::ToggleAuthorColumn),
    app("toggle-date-column", "Toggle the Date column", "view columns time", None, None, AppAction::ToggleDateColumn),
    app("toggle-hash-column", "Toggle the Hash column", "view columns sha", None, None, AppAction::ToggleHashColumn),
];

#[derive(Clone, Copy, Default)]
pub struct State {
    pub repo: bool,
    pub branch: bool,
    pub workdir: bool,
    pub changes: bool,
    pub unstaged: bool,
    pub staged: bool,
    pub stashes: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub commit_selected: bool,
    pub comparing: bool,
    pub filtering: bool,
    pub tabs: usize,
    pub pull: bool,
    pub github: bool,
    pub file_open: bool,
}

impl State {
    pub fn allows(&self, run: Run) -> bool {
        match run {
            Run::App(AppAction::CloseTab) => self.tabs > 0,
            Run::App(AppAction::NextTab | AppAction::PreviousTab) => self.tabs > 1,
            Run::App(_) => true,
            Run::Repo(action) => self.repo && self.allows_repo(action),
        }
    }

    fn allows_repo(&self, action: RepoAction) -> bool {
        use RepoAction::*;
        match action {
            Pull | Push | CopyBranchName | RenameBranch => self.branch,
            NewWorktree | OpenTerminal => self.workdir,
            Stash | ShowChanges => self.changes,
            StageAll => self.unstaged,
            UnstageAll => self.staged,
            Pop => self.stashes,
            Undo => self.can_undo,
            Redo => self.can_redo,
            ShowAllBranches => self.filtering,
            OpenPullRequest => self.pull,
            FileHistory | Blame => self.file_open,
            CreatePullRequest => self.github && self.branch && !self.pull,
            ShowPulls => self.github,
            CheckoutCommit | CherryPick | Revert | InteractiveRebase | EditMessage
            | CopyCommitHash | CreateTag | CompareWithHead => self.commit_selected,
            CompareWithWorkTree => self.commit_selected && self.workdir,
            ResetSoft | ResetMixed | ResetHard => self.commit_selected && self.branch,
            NextChange | PreviousChange | OpenFileInEditor | RevealFile | CopyFilePath => {
                self.file_open
            }
            ShowLatestStash => self.stashes,
            StopCompare => self.comparing,
            Fetch | NewBranch | Refresh | Search | ShowGraph | ShowWorktrees | ShowReflog
            | ShowConsole | RevealRepo | PushTags | AddRemote | FilterCommits => true,
        }
    }
}

pub struct KeyRow {
    pub keys: &'static str,
    pub what: &'static str,
    pub group: Group,
}

const fn key(keys: &'static str, what: &'static str, group: Group) -> KeyRow {
    KeyRow { keys, what, group }
}

#[rustfmt::skip]
pub static KEYS: &[KeyRow] = &[
    key("Cmd+K / Cmd+Shift+P", "Command palette: actions, branches, commits, files and tabs", Group::Navigation),
    key("Cmd+/", "Keyboard shortcuts", Group::Navigation),
    key("F6 / Shift+F6", "Move focus to the next / previous area: sidebar, graph, details", Group::Navigation),
    key("Tab / Shift+Tab", "Move focus to the next / previous control", Group::Navigation),
    key("Enter / Space", "Activate the focused control; Enter on a branch checks it out", Group::Navigation),
    key("Shift+F10", "Open the menu of the focused row or commit", Group::Navigation),
    key("Up / Down, J / K", "Move through commits", Group::Navigation),
    key("Up / Down, Home / End", "Move through the files of a commit or of Changes; the diff follows", Group::Navigation),
    key("Up / Down, Home / End", "Move through stashed files, the reflog, file history and the console", Group::Navigation),
    key("Enter", "Show a file history commit in the graph, or expand a console entry", Group::Navigation),
    key("S / U", "Stage / unstage the open or focused file in Changes", Group::Actions),
    key("Cmd+F", "Search; Enter / Shift+Enter for next / previous", Group::Navigation),
    key("Cmd+Shift+F", "Filter commits by author, path or date; Esc closes", Group::Navigation),
    key("Cmd+Opt+F", "Filter the sidebar; Esc clears", Group::Navigation),
    key("F2", "Rename the selected branch; Enter saves, Esc cancels", Group::Actions),
    key("Esc", "Back to the graph, close search or dialogs", Group::Navigation),
    key("Cmd+T", "New tab: recent repositories, open, clone or create one", Group::Tabs),
    key("Cmd+O", "Open a folder in a new tab", Group::Tabs),
    key("Cmd+W", "Close the tab", Group::Tabs),
    key("Cmd+Opt+S / Cmd+Opt+D", "Show or hide the sidebar / details panel", Group::Navigation),
    key("Cmd+Plus / Cmd+Minus / Cmd+0", "Zoom in / out / reset", Group::Navigation),
    key("Cmd+1 ... Cmd+9", "Go to a tab (Cmd+9 is the last one)", Group::Tabs),
    key("Ctrl+Tab / Ctrl+Shift+Tab", "Next / previous tab", Group::Tabs),
    key("Middle-click a tab", "Close it", Group::Tabs),
    key("Drag a tab", "Reorder tabs", Group::Tabs),
    key("Drop a folder on the window", "Open it (a file opens the repository it is in)", Group::Tabs),
    key("Alt+Up / Alt+Down", "Previous / next change in a diff", Group::Diff),
    key("Cmd+Enter", "Save a review comment or reply", Group::Diff),
    key("Cmd+Shift+Enter", "Commit and push, from the commit box", Group::Actions),
    key("Cmd+R", "Refresh the graph, changes and worktrees", Group::Actions),
    key("Cmd+Shift+R", "Fetch", Group::Actions),
    key("Cmd+Shift+L / Cmd+Shift+U", "Pull / push", Group::Actions),
    key("Cmd+Shift+B", "New branch", Group::Actions),
    key("Cmd+Shift+S", "Stash all changes", Group::Actions),
    key("Cmd+Shift+C", "Show uncommitted changes", Group::Actions),
    key("Cmd+Z / Cmd+Shift+Z", "Undo / redo the last action", Group::Actions),
    key("Cmd+,", "Settings", Group::Actions),
    key("Cmd+Opt+L", "Git console: every command Kelp ran, with its output", Group::Actions),
    key("Right-click", "Actions for commits, branches, tags, stashes, worktrees and tabs", Group::Actions),
    key("Double-click a branch", "Check it out", Group::Actions),
    key("Double-click a graph label", "Check out that branch", Group::Actions),
    key("Cmd-click a commit", "Compare it with the selected commit; Esc stops comparing", Group::Actions),
    key("Drag a graph label onto a commit", "Merge, rebase or reset", Group::Actions),
    key("Cmd-click / Shift-click a changed file", "Pick several to stage, stash or discard", Group::Actions),
    key("P / R / E / S / F / D", "In interactive rebase: pick, reword, edit, squash, fixup or drop the hovered commit", Group::Actions),
];

pub const GLOBAL_SHORTCUTS: [&str; 6] = [
    "fetch",
    "pull",
    "push",
    "new-branch",
    "stash",
    "show-changes",
];

pub fn parse_shortcut(text: &str) -> Option<KeyboardShortcut> {
    let mut modifiers = Modifiers::NONE;
    let mut parts: Vec<&str> = text.split('+').collect();
    let key = match parts.pop()? {
        "" if text.ends_with("++") => "Plus",
        key => key,
    };
    for part in parts {
        match part {
            "Cmd" => modifiers |= Modifiers::COMMAND,
            "Shift" => modifiers |= Modifiers::SHIFT,
            "Alt" | "Opt" => modifiers |= Modifiers::ALT,
            "Ctrl" => modifiers |= Modifiers::CTRL,
            _ => return None,
        }
    }
    let key = match key {
        "Up" => Key::ArrowUp,
        "Down" => Key::ArrowDown,
        "Left" => Key::ArrowLeft,
        "Right" => Key::ArrowRight,
        "," => Key::Comma,
        "/" => Key::Slash,
        "Esc" => Key::Escape,
        other => Key::from_name(other)?,
    };
    Some(KeyboardShortcut::new(modifiers, key))
}

pub fn key_parts(keys: &str) -> impl Iterator<Item = &str> {
    keys.split(" / ")
        .flat_map(|part| part.split(", "))
        .map(str::trim)
        .filter(|k| !k.is_empty())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Route {
        Palette(&'static str),
        Keys(&'static str),
        RowMenu(RowMenu),
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum RowMenu {
        SidebarRow,
        GraphCommit,
        FileRow,
        Tab,
    }

    const KEYBOARD_MENUS: [RowMenu; 4] = [
        RowMenu::SidebarRow,
        RowMenu::GraphCommit,
        RowMenu::FileRow,
        RowMenu::Tab,
    ];

    struct Reach {
        surface: &'static str,
        routes: &'static [Route],
    }

    const fn reach(surface: &'static str, routes: &'static [Route]) -> Reach {
        Reach { surface, routes }
    }

    use Route::{Keys, Palette, RowMenu as Menu};

    #[rustfmt::skip]
    static REACH: &[Reach] = &[
        reach("Toolbar: undo, fetch, pull, push, branch, worktree, stash, pop", &[Palette("undo"), Palette("fetch"), Palette("pull"), Palette("push"), Palette("new-branch"), Palette("new-worktree"), Palette("stash"), Palette("pop"), Keys("Cmd+Shift+R"), Keys("Cmd+Shift+L")]),
        reach("Tab strip: switch, new, close, reorder", &[Keys("Cmd+T"), Keys("Cmd+W"), Keys("Ctrl+Tab"), Keys("Cmd+1 ... Cmd+9"), Palette("next-tab")]),
        reach("Tab menu: close, close others, reveal, copy path", &[Menu(RowMenu::Tab), Palette("close-tab"), Palette("reveal-repo")]),
        reach("Branch menu (sidebar and graph labels): check out, new branch, rename, merge, rebase, push, pull, delete, hide, solo, pull requests", &[Menu(RowMenu::SidebarRow), Keys("F2"), Palette("rename-branch"), Palette("open-pull-request"), Palette("create-pull-request")]),
        reach("Tag, remote branch and remote menus", &[Menu(RowMenu::SidebarRow), Palette("push-tags"), Palette("add-remote")]),
        reach("Stash, worktree and submodule menus", &[Menu(RowMenu::SidebarRow), Palette("pop"), Palette("show-latest-stash"), Palette("show-worktrees")]),
        reach("Sidebar sections: sort, hide merged, collapse folders, filter", &[Keys("Tab"), Keys("Cmd+Opt+F")]),
        reach("Commit menu: check out, branch, worktree, cherry-pick, revert, rebase, edit message, reset, tag, compare, copy", &[Menu(RowMenu::GraphCommit), Palette("checkout-commit"), Palette("cherry-pick"), Palette("revert"), Palette("interactive-rebase"), Palette("edit-message"), Palette("reset-soft"), Palette("reset-mixed"), Palette("reset-hard"), Palette("create-tag"), Palette("compare-with-head"), Palette("compare-with-worktree"), Palette("copy-hash")]),
        reach("Graph: move, search, filter, compare, columns", &[Keys("Up"), Keys("Cmd+F"), Keys("Cmd+Shift+F"), Palette("stop-compare"), Palette("toggle-author-column"), Palette("toggle-date-column"), Palette("toggle-hash-column")]),
        reach("File menus (details, tree, staging): open in editor, reveal, copy path, history, blame, stage, discard, stash", &[Menu(RowMenu::FileRow), Palette("open-file-in-editor"), Palette("reveal-file"), Palette("copy-file-path"), Palette("file-history"), Palette("blame")]),
        reach("Staging: stage all, unstage all, commit", &[Palette("stage-all"), Palette("unstage-all"), Keys("Cmd+Shift+C"), Keys("Tab")]),
        reach("Diff: modes, next or previous change, stage hunk or lines", &[Keys("Alt+Up"), Palette("next-change"), Palette("previous-change"), Keys("Tab")]),
        reach("Views: worktrees, reflog, pull requests, stash, file history, blame", &[Palette("show-worktrees"), Palette("show-reflog"), Palette("show-pulls"), Palette("show-latest-stash"), Palette("file-history"), Palette("blame"), Keys("Esc")]),
        reach("Panels and zoom", &[Keys("Cmd+Opt+S"), Keys("Cmd+Plus"), Palette("toggle-sidebar"), Palette("zoom-in")]),
        reach("App: settings, shortcuts, updates, open, clone, new repository", &[Keys("Cmd+,"), Keys("Cmd+/"), Palette("check-updates"), Palette("open-repo"), Palette("clone"), Palette("new-repository")]),
        reach("Dialogs and confirms", &[Keys("Tab"), Keys("Enter"), Keys("Esc")]),
    ];

    #[test]
    fn every_action_is_listed_once() {
        let ids: Vec<&str> = ACTIONS.iter().map(|a| a.id).collect();
        let unique: HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "duplicate action ids");
        let expected = [
            "fetch",
            "pull",
            "push",
            "new-branch",
            "new-worktree",
            "stash",
            "pop",
            "undo",
            "redo",
            "refresh",
            "search",
            "filter-commits",
            "compare-with-head",
            "compare-with-worktree",
            "stop-compare",
            "show-graph",
            "show-worktrees",
            "show-reflog",
            "git-console",
            "show-changes",
            "stage-all",
            "unstage-all",
            "show-all-branches",
            "checkout-commit",
            "cherry-pick",
            "revert",
            "interactive-rebase",
            "edit-message",
            "copy-hash",
            "copy-branch",
            "open-pull-request",
            "show-pulls",
            "create-pull-request",
            "open-terminal",
            "reveal-repo",
            "rename-branch",
            "create-tag",
            "push-tags",
            "add-remote",
            "file-history",
            "blame",
            "reset-soft",
            "reset-mixed",
            "reset-hard",
            "next-change",
            "previous-change",
            "open-file-in-editor",
            "reveal-file",
            "copy-file-path",
            "show-latest-stash",
            "new-tab",
            "open-repo",
            "clone",
            "new-repository",
            "toggle-sidebar",
            "toggle-details",
            "zoom-in",
            "zoom-out",
            "zoom-reset",
            "close-tab",
            "next-tab",
            "previous-tab",
            "settings",
            "shortcuts",
            "check-updates",
            "whats-new",
            "help-getting-started",
            "help-shortcuts",
            "help-undo",
            "help-faq",
            "help-changelog",
            "report-bug",
            "toggle-descriptions",
            "toggle-fade",
            "toggle-author-column",
            "toggle-date-column",
            "toggle-hash-column",
        ];
        assert_eq!(unique, expected.into_iter().collect::<HashSet<_>>());
    }

    #[test]
    fn every_action_shortcut_is_on_the_sheet() {
        let sheet: HashSet<&str> = KEYS.iter().flat_map(|k| key_parts(k.keys)).collect();
        for action in ACTIONS {
            if let Some(shortcut) = action.shortcut {
                assert!(
                    sheet.contains(shortcut),
                    "{} ({shortcut}) is missing",
                    action.id
                );
            }
        }
    }

    #[test]
    fn readme_keys_table_matches_the_sheet() {
        let readme = include_str!("../../../README.md");
        let table = readme
            .split("## Keys")
            .nth(1)
            .and_then(|rest| rest.split("\n## ").next())
            .expect("README has a Keys section");
        let rows: Vec<(String, String)> = table
            .lines()
            .filter(|l| l.starts_with('|') && !l.starts_with("|---") && !l.starts_with("| Key |"))
            .map(|l| {
                let cells: Vec<&str> = l.trim_matches('|').split(" | ").map(str::trim).collect();
                (cells[0].to_string(), cells[1].to_string())
            })
            .collect();
        let sheet: Vec<(String, String)> = KEYS
            .iter()
            .map(|k| (k.keys.to_string(), k.what.to_string()))
            .collect();
        assert_eq!(rows, sheet);
    }

    #[test]
    fn every_route_on_the_checklist_exists() {
        let sheet: HashSet<&str> = KEYS.iter().flat_map(|k| key_parts(k.keys)).collect();
        for entry in REACH {
            assert!(!entry.routes.is_empty(), "{} has no route", entry.surface);
            for route in entry.routes {
                match route {
                    Route::Palette(id) => assert!(
                        ACTIONS.iter().any(|a| a.id == *id),
                        "{}: no palette action {id}",
                        entry.surface
                    ),
                    Route::Keys(k) => {
                        assert!(
                            sheet.contains(k),
                            "{}: {k} is not on the sheet",
                            entry.surface
                        )
                    }
                    Route::RowMenu(menu) => assert!(KEYBOARD_MENUS.contains(menu)),
                }
            }
        }
    }

    fn normalized(shortcut: KeyboardShortcut) -> (bool, bool, bool, bool, Key) {
        let m = shortcut.modifiers;
        (
            m.command || m.mac_cmd,
            m.shift,
            m.alt,
            m.ctrl,
            shortcut.logical_key,
        )
    }

    #[test]
    fn no_two_actions_share_a_shortcut() {
        let mut seen = std::collections::HashMap::new();
        for action in ACTIONS {
            let Some(text) = action.shortcut else {
                continue;
            };
            let parsed = parse_shortcut(text).unwrap_or_else(|| panic!("{text} does not parse"));
            if let Some(other) = seen.insert(normalized(parsed), action.id) {
                panic!("{} and {other} both use {text}", action.id);
            }
        }
        let mut rows = std::collections::HashMap::new();
        for row in KEYS {
            for part in key_parts(row.keys).filter(|p| p.contains('+')) {
                let Some(parsed) = parse_shortcut(part) else {
                    continue;
                };
                if let Some(other) = rows.insert(normalized(parsed), row.what) {
                    panic!("{part} is on two rows: {} and {other}", row.what);
                }
            }
        }
    }

    #[test]
    fn shortcuts_avoid_macos_system_keys() {
        let reserved = [
            "Cmd+Q",
            "Cmd+H",
            "Cmd+M",
            "Cmd+Tab",
            "Cmd+Space",
            "Cmd+Shift+3",
            "Cmd+Shift+4",
            "Cmd+Shift+5",
            "Cmd+Shift+Q",
            "Cmd+Opt+Esc",
            "Cmd+Opt+H",
            "Cmd+Shift+/",
        ]
        .map(|k| normalized(parse_shortcut(k).unwrap()));
        for action in ACTIONS {
            if let Some(parsed) = action.shortcut.and_then(parse_shortcut) {
                assert!(
                    !reserved.contains(&normalized(parsed)),
                    "{} uses a system key",
                    action.id
                );
            }
        }
    }

    #[test]
    fn global_shortcuts_parse_and_belong_to_actions() {
        for id in GLOBAL_SHORTCUTS {
            let action = ACTIONS.iter().find(|a| a.id == id).expect(id);
            assert!(action.shortcut.and_then(parse_shortcut).is_some(), "{id}");
        }
        let fetch = parse_shortcut("Cmd+Shift+R").unwrap();
        assert_eq!(fetch.logical_key, Key::R);
        assert!(fetch.modifiers.shift && fetch.modifiers.command);
        assert_eq!(parse_shortcut("Cmd+,").unwrap().logical_key, Key::Comma);
        assert_eq!(
            parse_shortcut("Alt+Down").unwrap().logical_key,
            Key::ArrowDown
        );
    }

    #[test]
    fn key_parts_keep_punctuation_keys() {
        let parts: Vec<&str> = key_parts("Up / Down, J / K").collect();
        assert_eq!(parts, ["Up", "Down", "J", "K"]);
        assert_eq!(key_parts("Cmd+,").collect::<Vec<_>>(), ["Cmd+,"]);
        assert_eq!(key_parts("Cmd+/").collect::<Vec<_>>(), ["Cmd+/"]);
    }

    #[test]
    fn selection_actions_need_a_selected_commit() {
        let mut state = State {
            repo: true,
            ..State::default()
        };
        assert!(!state.allows(Run::Repo(RepoAction::CherryPick)));
        state.commit_selected = true;
        assert!(state.allows(Run::Repo(RepoAction::CherryPick)));
        assert!(!State::default().allows(Run::Repo(RepoAction::Fetch)));
        assert!(State::default().allows(Run::App(AppAction::OpenRepo)));
    }

    fn escape_html(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    fn key_html(part: &str) -> String {
        if part.contains(' ') {
            part.split(' ')
                .map(|word| match word.contains('+') {
                    true => format!("<kbd>{}</kbd>", escape_html(word)),
                    false => escape_html(word),
                })
                .collect::<Vec<_>>()
                .join(" ")
        } else if part.contains("-click") {
            escape_html(part)
        } else {
            format!("<kbd>{}</kbd>", escape_html(part))
        }
    }

    fn keys_html(keys: &str) -> String {
        keys.split(" / ")
            .map(|alternative| {
                alternative
                    .split(", ")
                    .map(key_html)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect::<Vec<_>>()
            .join(" / ")
    }

    fn shortcut_tables() -> String {
        Group::ALL
            .iter()
            .map(|group| {
                let rows: String = KEYS
                    .iter()
                    .filter(|k| k.group == *group)
                    .map(|k| {
                        format!(
                            "      <tr><td>{}</td><td>{}</td></tr>\n",
                            keys_html(k.keys),
                            escape_html(k.what)
                        )
                    })
                    .collect();
                format!(
                    "  <h2 id=\"{}\">{}</h2>\n  <table>\n    <thead><tr><th>Keys</th><th>What it does</th></tr></thead>\n    <tbody>\n{rows}    </tbody>\n  </table>\n",
                    group.title().to_lowercase(),
                    group.title()
                )
            })
            .collect()
    }

    #[test]
    fn shortcuts_page_matches_the_sheet() {
        const OPEN: &str = "<div class=\"keys\">\n";
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../site/docs/shortcuts.html");
        let page = std::fs::read_to_string(&path).expect("site/docs/shortcuts.html");
        let start = page.find(OPEN).expect("shortcuts page has a keys block") + OPEN.len();
        let end = start + page[start..].find("</div>").expect("keys block is closed");
        let expected = shortcut_tables();
        if std::env::var_os("KELP_WRITE_DOCS").is_some() {
            std::fs::write(
                &path,
                format!("{}{expected}{}", &page[..start], &page[end..]),
            )
            .expect("write shortcuts page");
            return;
        }
        assert!(
            page[start..end] == expected,
            "site/docs/shortcuts.html is out of date; run KELP_WRITE_DOCS=1 cargo test -p kelp shortcuts_page"
        );
    }

    #[test]
    fn shortcut_keys_render_as_kbd() {
        assert_eq!(
            keys_html("Up / Down, J / K"),
            "<kbd>Up</kbd> / <kbd>Down</kbd>, <kbd>J</kbd> / <kbd>K</kbd>"
        );
        assert_eq!(
            keys_html("Cmd+1 ... Cmd+9"),
            "<kbd>Cmd+1</kbd> ... <kbd>Cmd+9</kbd>"
        );
        assert_eq!(
            keys_html("Cmd-click / Shift-click a changed file"),
            "Cmd-click / Shift-click a changed file"
        );
        assert_eq!(keys_html("Drag a tab"), "Drag a tab");
    }
}
