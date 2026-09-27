use std::path::{Path, PathBuf};

use kelp_core::git_cli::run;
use kelp_core::history::History;
use kelp_core::ops::Op;
use kelp_core::refs::RefKind;
use kelp_core::{search, workspace};

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("kelp-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = Scratch(dir);
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "test@example.com"]);
        repo.git(&["config", "user.name", "Test"]);
        repo.commit("a.txt", "one", "first");
        repo.commit("a.txt", "two", "second");
        repo
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn git(&self, args: &[&str]) -> String {
        run(&self.0, args).unwrap()
    }

    fn commit(&self, file: &str, content: &str, message: &str) {
        std::fs::write(self.0.join(file), content).unwrap();
        self.git(&["add", "."]);
        self.git(&["commit", "-q", "-m", message]);
    }

    fn branches(&self) -> Vec<String> {
        let (_, history) = History::open(&self.0).unwrap();
        history
            .refs
            .of_kind(RefKind::Local)
            .map(|l| l.name.clone())
            .collect()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
        let _ = std::fs::remove_dir_all(self.0.with_file_name(format!(
            "{}-wt",
            self.0.file_name().unwrap().to_string_lossy()
        )));
    }
}

#[test]
fn branch_lifecycle() {
    let repo = Scratch::new("branches");
    Op::CreateBranch {
        name: "feat/x".into(),
        start: "main".into(),
        switch: true,
    }
    .run(repo.path())
    .unwrap();
    repo.commit("b.txt", "b", "on feature");
    assert_eq!(repo.git(&["branch", "--show-current"]).trim(), "feat/x");

    Op::Switch("main".into()).run(repo.path()).unwrap();
    Op::Merge("feat/x".into()).run(repo.path()).unwrap();
    assert!(repo.path().join("b.txt").exists());

    Op::RenameBranch {
        from: "feat/x".into(),
        to: "feat/y".into(),
    }
    .run(repo.path())
    .unwrap();
    assert!(repo.branches().contains(&"feat/y".to_string()));

    Op::DeleteBranch {
        name: "feat/y".into(),
        force: false,
    }
    .run(repo.path())
    .unwrap();
    assert_eq!(repo.branches(), ["main"]);
}

#[test]
fn unmerged_branch_needs_force_to_delete() {
    let repo = Scratch::new("force");
    Op::CreateBranch {
        name: "wip".into(),
        start: "main".into(),
        switch: true,
    }
    .run(repo.path())
    .unwrap();
    repo.commit("c.txt", "c", "unmerged");
    Op::Switch("main".into()).run(repo.path()).unwrap();
    assert!(
        Op::DeleteBranch {
            name: "wip".into(),
            force: false
        }
        .run(repo.path())
        .is_err()
    );
    Op::DeleteBranch {
        name: "wip".into(),
        force: true,
    }
    .run(repo.path())
    .unwrap();
}

#[test]
fn stash_push_and_pop() {
    let repo = Scratch::new("stash");
    std::fs::write(repo.path().join("a.txt"), "dirty").unwrap();
    std::fs::write(repo.path().join("new.txt"), "untracked").unwrap();
    Op::StashPush.run(repo.path()).unwrap();
    assert_eq!(workspace::stashes(repo.path()).unwrap().len(), 1);
    assert!(!repo.path().join("new.txt").exists());
    Op::StashPop.run(repo.path()).unwrap();
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "dirty"
    );
    assert!(workspace::stashes(repo.path()).unwrap().is_empty());
}

#[test]
fn worktree_add_list_remove_prune() {
    let repo = Scratch::new("worktrees");
    let folder = format!(
        "../{}-wt",
        repo.path().file_name().unwrap().to_string_lossy()
    );
    Op::WorktreeAdd {
        path: folder.clone(),
        new_branch: Some("feat/wt".into()),
        start: "main".into(),
    }
    .run(repo.path())
    .unwrap();
    let trees = workspace::worktrees(repo.path()).unwrap();
    assert_eq!(trees.len(), 2);
    assert_eq!(trees[1].branch.as_deref(), Some("feat/wt"));
    assert_eq!(workspace::change_count(&trees[1].path), Some(0));

    let (_, from_worktree) = History::open(&trees[1].path).unwrap();
    assert_eq!(from_worktree.refs.head_branch.as_deref(), Some("feat/wt"));

    Op::WorktreeRemove {
        path: trees[1].path.display().to_string(),
        force: false,
    }
    .run(repo.path())
    .unwrap();
    Op::WorktreePrune.run(repo.path()).unwrap();
    assert_eq!(workspace::worktrees(repo.path()).unwrap().len(), 1);
}

#[test]
fn ahead_behind_against_upstream() {
    let origin = Scratch::new("origin");
    let clone_dir = origin.path().with_file_name(format!(
        "{}-clone",
        origin.path().file_name().unwrap().to_string_lossy()
    ));
    let _ = std::fs::remove_dir_all(&clone_dir);
    run(
        origin.path().parent().unwrap(),
        &[
            "clone",
            "-q",
            origin.path().to_str().unwrap(),
            clone_dir.to_str().unwrap(),
        ],
    )
    .unwrap();
    let clone = Scratch(clone_dir);
    clone.git(&["config", "user.email", "test@example.com"]);
    clone.git(&["config", "user.name", "Test"]);
    clone.commit("d.txt", "d", "local only");
    origin.commit("e.txt", "e", "remote only");
    Op::Fetch.run(clone.path()).unwrap();
    assert_eq!(workspace::ahead_behind(clone.path(), "main"), Some((1, 1)));
}

#[test]
fn search_matches_message_author_and_hash_prefix() {
    let repo = Scratch::new("search");
    repo.commit("f.txt", "f", "feat(graph): draw merge curves");
    let (_, history) = History::open(repo.path()).unwrap();
    let rows = |q: &str| search::matching_rows(repo.path(), history.ids(), q).unwrap();
    assert_eq!(rows("MERGE curves"), [0]);
    assert_eq!(rows("test@example.com").len(), 3);
    let prefix = history.id(2).to_hex().to_string()[..8].to_string();
    assert_eq!(rows(&prefix), [2]);
    assert!(rows("nothing like this").is_empty());
}
