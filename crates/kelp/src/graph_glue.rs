use kelp_core::commit::{self, Summary};
use kelp_core::history::History;
use kelp_ui::graph::{GraphAction, Preview};

use crate::commands::Command;
use crate::ref_labels;

pub fn load_summary(repo: &gix::Repository, history: &History, row: usize) -> Summary {
    commit::summary(repo, history.id(row)).unwrap_or_else(|_| Summary {
        title: history.id(row).to_hex_with_len(7).to_string(),
        body_preview: String::new(),
        author: String::new(),
        email: String::new(),
        time: 0,
    })
}

pub fn github_owners(repo: &gix::Repository) -> Vec<(String, String)> {
    repo.remote_names()
        .iter()
        .filter_map(|name| {
            let name = name.to_string();
            let remote = repo.find_remote(name.as_str()).ok()?;
            let url = remote
                .url(gix::remote::Direction::Fetch)?
                .to_bstring()
                .to_string();
            let github = kelp_core::avatar::GitHubRepo::from_remote_url(&url)?;
            Some((name, github.owner))
        })
        .collect()
}

pub struct DevPreview {
    hover_row: Option<usize>,
    open_ref_list: Option<String>,
    open_drop: Option<String>,
    open_commit_menu: bool,
    open_label_menu: Option<String>,
}

impl DevPreview {
    pub fn from_env() -> Self {
        Self {
            hover_row: std::env::var("KELP_HOVER_ROW")
                .ok()
                .and_then(|v| v.parse().ok()),
            open_ref_list: std::env::var("KELP_OPEN_REFS").ok(),
            open_drop: std::env::var("KELP_OPEN_DROP").ok(),
            open_commit_menu: std::env::var("KELP_OPEN_MENU").as_deref() == Ok("commit"),
            open_label_menu: std::env::var("KELP_OPEN_LABEL_MENU").ok(),
        }
    }

    pub fn preview(&self) -> Preview<'_> {
        Preview {
            hover_row: self.hover_row,
            open_ref_list: self.open_ref_list.as_deref(),
            open_drop: self.open_drop.as_deref(),
            open_commit_menu: self.open_commit_menu,
            open_label_menu: self.open_label_menu.as_deref(),
        }
    }
}

pub fn command_for(
    action: &GraphAction,
    commit_id: impl Fn(usize) -> kelp_core::ObjectId,
) -> Option<Command> {
    match action {
        GraphAction::ActivateRef(label) => ref_labels::checkout_op(label).map(Command::Run),
        GraphAction::OpenUrl(url) => Some(Command::OpenUrl(url.clone())),
        GraphAction::OpenWorktree(path) => Some(Command::OpenWorktree(path.clone())),
        GraphAction::OpenChecks(row) => Some(Command::OpenChecks(commit_id(*row))),
        GraphAction::Select(_) | GraphAction::Compare(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use kelp_core::ops::Op;
    use kelp_core::refs::{RefKind, RefLabel};

    use super::*;

    fn label(name: &str, is_head: bool) -> RefLabel {
        RefLabel {
            name: name.into(),
            kind: RefKind::Local,
            target: gix::ObjectId::null(gix::hash::Kind::Sha1),
            row: Some(0),
            is_head,
            has_remote: false,
            hidden: false,
            pull: None,
        }
    }

    #[test]
    fn activating_a_label_checks_out_its_branch_but_not_the_current_one() {
        let id = |_| gix::ObjectId::null(gix::hash::Kind::Sha1);
        let other = GraphAction::ActivateRef(label("feat/zzz", false));
        assert!(matches!(
            command_for(&other, id),
            Some(Command::Run(Op::Switch(name))) if name == "feat/zzz"
        ));
        let head = GraphAction::ActivateRef(label("main", true));
        assert!(command_for(&head, id).is_none());
    }
}
