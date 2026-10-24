mod columns;
#[doc(hidden)]
pub mod counts;
mod flow;
mod highlight;
mod hover;
mod labels;
mod marks;
mod notes;
mod rows;
mod style;
mod view;

use std::path::PathBuf;

use kelp_core::refs::RefLabel;

pub use columns::{Column, GraphColumns};
pub use highlight::Highlight;
pub use labels::{DropPlan, DropTarget, MenuFor};
pub use marks::{MARK_A, MARK_B, Marks, paint_mark};
pub use style::{GraphStyle, RefPlacement};
pub use view::{GraphInput, GraphView, OtherWip, Preview, ROW_H, Wip};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// What the graph shows as selected: a commit row, or the uncommitted changes row.
pub enum Selection {
    Wip,
    Commit(usize),
}

/// What the user asked for in the graph. The host decides what each one does.
pub enum GraphAction {
    /// A row was clicked.
    Select(Selection),
    /// A commit row was clicked while picking a comparison (or with Cmd held).
    Compare(usize),
    /// A branch or tag chip was double-clicked.
    ActivateRef(RefLabel),
    /// A pull request pill on a chip was clicked.
    OpenUrl(String),
    /// The Open button on another worktree's uncommitted row was clicked.
    OpenWorktree(PathBuf),
    /// The check dot on a commit row was clicked.
    OpenChecks(usize),
}
