use std::path::PathBuf;

use kelp_core::ops::Op;

use crate::dialogs::Dialog;
use crate::repo_view::Selection;

pub enum Command {
    Run(Op),
    Open(Dialog),
    Copy(String),
    Reveal(Selection),
    ShowWorktrees,
    OpenRepo(PathBuf),
    OpenTerminal(PathBuf),
}
