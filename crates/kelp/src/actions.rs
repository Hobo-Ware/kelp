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
    repo("fetch", "Fetch", "remote download update", None, Some(Icon::Fetch), RepoAction::Fetch),
    repo("pull", "Pull", "merge remote update", None, Some(Icon::Pull), RepoAction::Pull),
    repo("push", "Push", "upload remote publish", None, Some(Icon::Push), RepoAction::Push),
    repo("new-branch", "New branch…", "create branch", None, Some(Icon::Branch), RepoAction::NewBranch),
    repo("new-worktree", "New worktree…", "create worktree", None, Some(Icon::Worktree), RepoAction::NewWorktree),
    repo("stash", "Stash all changes", "save shelve", None, Some(Icon::Stash), RepoAction::Stash),
    repo("pop", "Pop the latest stash", "apply unstash", None, Some(Icon::Pop), RepoAction::Pop),
    repo("undo", "Undo", "revert back", Some("Cmd+Z"), Some(Icon::Undo), RepoAction::Undo),
    repo("redo", "Redo", "again", Some("Cmd+Shift+Z"), None, RepoAction::Redo),
    repo("refresh", "Refresh", "reload status", Some("Cmd+R"), None, RepoAction::Refresh),
    repo("search", "Search commits", "find message author hash", Some("Cmd+F"), None, RepoAction::Search),
    repo("show-graph", "Go to the graph", "history back close", Some("Esc"), None, RepoAction::ShowGraph),
    repo("show-worktrees", "Manage worktrees", "worktree list", None, Some(Icon::Worktree), RepoAction::ShowWorktrees),
    repo("show-changes", "Show uncommitted changes", "wip staging working tree", None, None, RepoAction::ShowChanges),
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
    repo("create-pull-request", "Create a pull request", "pr github new", None, Some(Icon::Merge), RepoAction::CreatePullRequest),
    repo("open-terminal", "Open in Terminal", "shell console", None, Some(Icon::Terminal), RepoAction::OpenTerminal),
    repo("reveal-repo", "Reveal the repository in Finder", "folder show", None, Some(Icon::Folder), RepoAction::RevealRepo),
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
    pub filtering: bool,
    pub tabs: usize,
    pub pull: bool,
    pub github: bool,
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
            Pull | Push | CopyBranchName => self.branch,
            NewWorktree | OpenTerminal => self.workdir,
            Stash | ShowChanges => self.changes,
            StageAll => self.unstaged,
            UnstageAll => self.staged,
            Pop => self.stashes,
            Undo => self.can_undo,
            Redo => self.can_redo,
            ShowAllBranches => self.filtering,
            OpenPullRequest => self.pull,
            CreatePullRequest => self.github && self.branch && !self.pull,
            CheckoutCommit | CherryPick | Revert | InteractiveRebase | EditMessage
            | CopyCommitHash => self.commit_selected,
            Fetch | NewBranch | Refresh | Search | ShowGraph | ShowWorktrees | RevealRepo => true,
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
    key("Up / Down, J / K", "Move through commits", Group::Navigation),
    key("Cmd+F", "Search; Enter / Shift+Enter for next / previous", Group::Navigation),
    key("Cmd+Opt+F", "Filter the sidebar; Esc clears", Group::Navigation),
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
    key("Cmd+R", "Refresh the graph, changes and worktrees", Group::Actions),
    key("Cmd+Z / Cmd+Shift+Z", "Undo / redo the last action", Group::Actions),
    key("Cmd+,", "Settings", Group::Actions),
    key("Right-click", "Actions for commits, branches, tags, stashes, worktrees and tabs", Group::Actions),
    key("Double-click a branch", "Check it out", Group::Actions),
    key("Double-click a graph label", "Check out that branch", Group::Actions),
    key("Drag a graph label onto a commit", "Merge, rebase or reset", Group::Actions),
    key("Cmd-click / Shift-click a changed file", "Pick several to stage, stash or discard", Group::Actions),
    key("P / R / E / S / F / D", "In interactive rebase: pick, reword, edit, squash, fixup or drop the hovered commit", Group::Actions),
];

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
            "show-graph",
            "show-worktrees",
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
            "create-pull-request",
            "open-terminal",
            "reveal-repo",
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
}
