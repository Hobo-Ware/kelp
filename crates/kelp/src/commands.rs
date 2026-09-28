use std::path::PathBuf;

use kelp_core::ops::Op;

use crate::dialogs::Dialog;
use crate::rebase_view;
use crate::repo_view::Selection;

pub enum Command {
    Run(Op),
    Push(String),
    ResetHard(String),
    Open(Dialog),
    Copy(String),
    Reveal(Selection),
    ShowWorktrees,
    OpenRepo(PathBuf),
    OpenTerminal(PathBuf),
    OpenInEditor(String),
    RevealFile(String),
    Undo,
    OpenRebase(String),
    EditMessage(String),
    StartRebase(rebase_view::Start),
    ToggleRef(String),
    SoloRef(String),
    ShowAllRefs,
    TogglePanel(crate::panels::Side),
    OpenUrl(String),
    OpenChecks(gix::ObjectId),
    ShowPulls,
    CreatePullRequest(String),
    StartRename(String),
    ShowStash(String),
    ShowReflog(String),
    ShowConsole(Option<u64>),
    FileHistory(String),
    Blame(String),
    ShowCommit(gix::ObjectId),
    Compare {
        base: String,
        target: Option<String>,
    },
    PickCompare(String),
    ClearFilter,
}
