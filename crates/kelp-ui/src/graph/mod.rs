mod columns;
#[doc(hidden)]
pub mod counts;
mod hover;
mod labels;
mod marks;
mod rows;
mod view;

use std::path::PathBuf;

use kelp_core::refs::RefLabel;

pub use columns::{Column, GraphColumns};
pub use labels::{DropPlan, DropTarget, MenuFor};
pub use marks::{MARK_A, MARK_B, Marks, paint_mark};
pub use view::{GraphInput, GraphView, OtherWip, Preview, ROW_H, Wip};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Wip,
    Commit(usize),
}

pub enum GraphAction {
    Select(Selection),
    Compare(usize),
    ActivateRef(RefLabel),
    OpenUrl(String),
    OpenWorktree(PathBuf),
    OpenChecks(usize),
}
