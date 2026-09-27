mod common;

use common::Scratch;
use kelp_core::ops::Op;
use kelp_core::undo::{self, Outcome, Record};

fn recorded(repo: &Scratch, op: Op) -> Record {
    let (result, outcome) = undo::run_recorded(&op, repo.path());
    result.unwrap();
    match outcome {
        Outcome::Recorded(record) => *record,
        other => panic!("expected a record for {op:?}, got {other:?}"),
    }
}

fn head(repo: &Scratch) -> String {
    repo.git(&["rev-parse", "HEAD"]).trim().to_string()
}

fn current_branch(repo: &Scratch) -> String {
    repo.git(&["branch", "--show-current"]).trim().to_string()
}

fn read(repo: &Scratch, file: &str) -> String {
    std::fs::read_to_string(repo.path().join(file)).unwrap_or_default()
}

fn status(repo: &Scratch) -> String {
    repo.git(&["status", "--porcelain"])
}

#[test]
fn commit_is_undone_and_its_changes_stay_staged() {
    let repo = Scratch::new("undo-commit");
    let before = head(&repo);
    std::fs::write(repo.path().join("a.txt"), "three").unwrap();
    repo.git(&["add", "a.txt"]);
    let record = recorded(
        &repo,
        Op::Commit {
            message: "third".into(),
            amend: false,
        },
    );
    assert_ne!(head(&repo), before);

    let commands = undo::undo(repo.path(), &record).unwrap();
    assert_eq!(head(&repo), before);
    assert_eq!(status(&repo), "M  a.txt\n");
    assert!(commands.iter().any(|c| c.starts_with("git update-ref")));

    record.op.run(repo.path()).unwrap();
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s"]).trim(),
        "third",
        "redo runs the op again"
    );
}

#[test]
fn amend_is_undone_back_to_the_original_commit() {
    let repo = Scratch::new("undo-amend");
    let before = head(&repo);
    let record = recorded(
        &repo,
        Op::Commit {
            message: "second, reworded".into(),
            amend: true,
        },
    );
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(head(&repo), before);
    assert_eq!(status(&repo), "");
}

#[test]
fn checkout_switches_back() {
    let repo = Scratch::new("undo-checkout");
    repo.git(&["branch", "other", "HEAD~1"]);
    let record = recorded(&repo, Op::Switch("other".into()));
    assert_eq!(read(&repo, "a.txt"), "one");
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(current_branch(&repo), "main");
    assert_eq!(read(&repo, "a.txt"), "two");
    assert_eq!(status(&repo), "");
}

#[test]
fn created_branch_is_removed_and_head_returns() {
    let repo = Scratch::new("undo-create");
    let record = recorded(
        &repo,
        Op::CreateBranch {
            name: "feat/x".into(),
            start: "HEAD~1".into(),
            switch: true,
        },
    );
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(current_branch(&repo), "main");
    assert_eq!(repo.branches(), ["main"]);
    assert_eq!(read(&repo, "a.txt"), "two");
}

#[test]
fn deleted_branch_comes_back_with_its_upstream() {
    let repo = Scratch::new("undo-delete");
    repo.git(&["branch", "keep", "HEAD~1"]);
    repo.git(&["config", "branch.keep.remote", "origin"]);
    repo.git(&["config", "branch.keep.merge", "refs/heads/keep"]);
    let sha = repo.git(&["rev-parse", "keep"]);
    let record = recorded(
        &repo,
        Op::DeleteBranch {
            name: "keep".into(),
            force: true,
        },
    );
    assert_eq!(repo.branches(), ["main"]);
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(repo.git(&["rev-parse", "keep"]), sha);
    assert_eq!(repo.git(&["config", "branch.keep.remote"]).trim(), "origin");
}

#[test]
fn renamed_current_branch_is_renamed_back() {
    let repo = Scratch::new("undo-rename");
    let record = recorded(
        &repo,
        Op::RenameBranch {
            from: "main".into(),
            to: "trunk".into(),
        },
    );
    assert_eq!(current_branch(&repo), "trunk");
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(current_branch(&repo), "main");
    assert_eq!(repo.branches(), ["main"]);
}

#[test]
fn merge_is_undone() {
    let repo = Scratch::new("undo-merge");
    repo.git(&["switch", "-q", "-c", "feat"]);
    repo.commit("b.txt", "b", "feature work");
    repo.git(&["switch", "-q", "main"]);
    repo.commit("c.txt", "c", "main work");
    let before = head(&repo);
    let record = recorded(&repo, Op::Merge("feat".into()));
    assert!(repo.path().join("b.txt").exists());
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(head(&repo), before);
    assert!(!repo.path().join("b.txt").exists());
    assert_eq!(status(&repo), "");
}

#[test]
fn dropped_stash_is_stored_again() {
    let repo = Scratch::new("undo-stash-drop");
    std::fs::write(repo.path().join("a.txt"), "wip").unwrap();
    repo.git(&["stash", "push", "-q", "-m", "kelp-undo-test"]);
    let sha = repo.git(&["rev-parse", "stash@{0}"]);
    let record = recorded(&repo, Op::StashDrop("stash@{0}".into()));
    assert_eq!(repo.git(&["stash", "list"]), "");
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(repo.git(&["rev-parse", "stash@{0}"]), sha);
    assert!(repo.git(&["stash", "list"]).contains("kelp-undo-test"));
}

#[test]
fn stash_push_is_undone_into_the_working_tree() {
    let repo = Scratch::new("undo-stash-push");
    std::fs::write(repo.path().join("a.txt"), "wip").unwrap();
    let record = recorded(&repo, Op::StashPush);
    assert_eq!(read(&repo, "a.txt"), "two");
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(read(&repo, "a.txt"), "wip");
    assert_eq!(repo.git(&["stash", "list"]), "");
}

#[test]
fn discarded_changes_come_back_staged_and_unstaged() {
    let repo = Scratch::new("undo-discard");
    repo.commit("b.txt", "base", "add b");
    std::fs::write(repo.path().join("b.txt"), "staged").unwrap();
    repo.git(&["add", "b.txt"]);
    std::fs::write(repo.path().join("a.txt"), "unstaged edit").unwrap();
    let record = recorded(&repo, Op::DiscardChanges(vec!["a.txt".into()]));
    assert_eq!(read(&repo, "a.txt"), "two");
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(read(&repo, "a.txt"), "unstaged edit");
    assert_eq!(read(&repo, "b.txt"), "staged");
    assert_eq!(status(&repo), " M a.txt\nM  b.txt\n");
}

#[test]
fn deleted_untracked_file_is_written_back() {
    let repo = Scratch::new("undo-untracked");
    std::fs::write(repo.path().join("notes.md"), "draft").unwrap();
    let record = recorded(&repo, Op::DeleteUntracked(vec!["notes.md".into()]));
    assert!(!repo.path().join("notes.md").exists());
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(read(&repo, "notes.md"), "draft");
}

#[test]
fn staging_is_undone() {
    let repo = Scratch::new("undo-stage");
    std::fs::write(repo.path().join("a.txt"), "edit").unwrap();
    let record = recorded(&repo, Op::Stage(vec!["a.txt".into()]));
    assert_eq!(status(&repo), "M  a.txt\n");
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(status(&repo), " M a.txt\n");
    assert_eq!(read(&repo, "a.txt"), "edit");
}

#[test]
fn undo_refuses_when_the_repo_moved_on() {
    let repo = Scratch::new("undo-refuse");
    std::fs::write(repo.path().join("a.txt"), "three").unwrap();
    repo.git(&["add", "a.txt"]);
    let record = recorded(
        &repo,
        Op::Commit {
            message: "third".into(),
            amend: false,
        },
    );
    repo.commit("d.txt", "d", "made outside Kelp");
    let moved_on = head(&repo);
    let err = undo::undo(repo.path(), &record).unwrap_err();
    assert!(err.to_string().contains("Nothing was changed"), "{err}");
    assert_eq!(head(&repo), moved_on);
    assert_eq!(status(&repo), "");
}

#[test]
fn remote_actions_are_reported_as_not_undoable() {
    let repo = Scratch::new("undo-remote");
    assert!(undo::not_undoable_reason(&Op::Fetch).is_some());
    let push = Op::Push {
        branch: "main".into(),
        remote: "origin".into(),
        set_upstream: false,
        force_with_lease: false,
    };
    assert_eq!(
        undo::not_undoable_reason(&push),
        Some("it changed the remote")
    );
    let (_, outcome) = undo::run_recorded(&Op::Stage(vec!["a.txt".into()]), repo.path());
    assert!(
        matches!(outcome, Outcome::Unchanged),
        "a no-op records nothing"
    );
}

#[test]
fn stack_keeps_fifty_and_clears_redo_on_new_actions() {
    let repo = Scratch::new("undo-stack");
    let mut stack = undo::Stack::default();
    let mut evicted = 0;
    for i in 0..52 {
        std::fs::write(repo.path().join("a.txt"), format!("v{i}")).unwrap();
        evicted += stack
            .record(recorded(&repo, Op::Stage(vec!["a.txt".into()])))
            .len();
    }
    assert_eq!(evicted, 2);
    let last = stack.take_undo().unwrap();
    stack.undone(last);
    assert!(stack.next_redo().is_some());
    std::fs::write(repo.path().join("a.txt"), "new").unwrap();
    let cleared = stack.record(recorded(&repo, Op::Stage(vec!["a.txt".into()])));
    assert_eq!(cleared.len(), 1);
    assert!(stack.next_redo().is_none());
}
