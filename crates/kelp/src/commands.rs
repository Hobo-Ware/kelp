use std::path::PathBuf;

use kelp_core::ops::Op;

use crate::dialogs::Dialog;
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
}
