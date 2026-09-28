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

    record.op().unwrap().run(repo.path()).unwrap();
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

fn rewrite(
    repo: &Scratch,
    label: &str,
    run: impl FnOnce() -> kelp_core::rebase::Outcome,
) -> Record {
    let before = undo::capture(repo.path()).unwrap();
    assert_eq!(run(), kelp_core::rebase::Outcome::Done);
    match undo::record_rewrite(repo.path(), label.into(), before) {
        Outcome::Recorded(record) => *record,
        other => panic!("expected a rewrite record, got {other:?}"),
    }
}

fn three_more(repo: &Scratch) {
    repo.commit("b.txt", "b\n", "add b");
    repo.commit("c.txt", "c\n", "add c");
    repo.commit("d.txt", "d\n", "add d");
}

#[test]
fn message_edit_of_head_is_undone_and_redone() {
    let repo = Scratch::new("undo-reword-head");
    let old = head(&repo);
    let record = rewrite(&repo, "Edit message", || {
        kelp_core::rebase::edit_message(repo.path(), "HEAD", "second, better").unwrap()
    });
    let new = head(&repo);
    assert_ne!(new, old);
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(head(&repo), old);
    assert_eq!(current_branch(&repo), "main");
    undo::redo_rewrite(repo.path(), &record).unwrap();
    assert_eq!(head(&repo), new);
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s"]).trim(),
        "second, better"
    );
}

#[test]
fn message_edit_three_back_is_undone() {
    let repo = Scratch::new("undo-reword-older");
    three_more(&repo);
    let old = head(&repo);
    let record = rewrite(&repo, "Edit message", || {
        kelp_core::rebase::edit_message(repo.path(), "HEAD~2", "add b, renamed").unwrap()
    });
    assert_ne!(head(&repo), old);
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(head(&repo), old);
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s", "HEAD~2"]).trim(),
        "add b"
    );
    assert_eq!(status(&repo), "");
}

#[test]
fn interactive_rebase_is_undone_to_the_exact_old_tip_and_redone() {
    let repo = Scratch::new("undo-rebase");
    three_more(&repo);
    let old = head(&repo);
    let mut plan = kelp_core::rebase::load(repo.path(), "HEAD~3").unwrap();
    plan.steps.swap(0, 2);
    plan.steps[2].action = kelp_core::rebase::Action::Squash;
    let steps = plan.steps.clone();
    let record = rewrite(&repo, "Interactive rebase", || {
        kelp_core::rebase::run(repo.path(), &plan, &|g| {
            kelp_core::rebase::default_combined(&steps, g)
        })
        .unwrap()
    });
    let rewritten = head(&repo);
    assert_ne!(rewritten, old);
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(head(&repo), old, "undo restores the exact old tip");
    assert!(repo.path().join("d.txt").exists());
    assert_eq!(status(&repo), "");
    undo::redo_rewrite(repo.path(), &record).unwrap();
    assert_eq!(head(&repo), rewritten);
    assert_eq!(status(&repo), "");
}

#[test]
fn rewrite_undo_refuses_after_new_work() {
    let repo = Scratch::new("undo-rebase-refuse");
    three_more(&repo);
    let record = rewrite(&repo, "Edit message", || {
        kelp_core::rebase::edit_message(repo.path(), "HEAD~1", "add c, renamed").unwrap()
    });
    repo.commit("e.txt", "e\n", "add e");
    let tip = head(&repo);
    let err = undo::undo(repo.path(), &record).unwrap_err();
    assert!(err.to_string().contains("Nothing was changed"), "{err}");
    assert_eq!(head(&repo), tip);
}

#[test]
fn rewrite_redo_refuses_after_new_work() {
    let repo = Scratch::new("undo-redo-refuse");
    three_more(&repo);
    let record = rewrite(&repo, "Edit message", || {
        kelp_core::rebase::edit_message(repo.path(), "HEAD", "add d, renamed").unwrap()
    });
    undo::undo(repo.path(), &record).unwrap();
    repo.commit("e.txt", "e\n", "add e");
    let tip = head(&repo);
    let err = undo::redo_rewrite(repo.path(), &record).unwrap_err();
    assert!(err.to_string().contains("Nothing was changed"), "{err}");
    assert_eq!(head(&repo), tip);
}

#[test]
fn undoing_a_rebase_that_dropped_a_commit_brings_the_file_back() {
    let repo = Scratch::new("undo-rebase-drop");
    three_more(&repo);
    let old = head(&repo);
    let mut plan = kelp_core::rebase::load(repo.path(), "HEAD~3").unwrap();
    plan.steps[1].action = kelp_core::rebase::Action::Drop;
    let steps = plan.steps.clone();
    let record = rewrite(&repo, "Interactive rebase", || {
        kelp_core::rebase::run(repo.path(), &plan, &|g| {
            kelp_core::rebase::default_combined(&steps, g)
        })
        .unwrap()
    });
    assert!(!repo.path().join("c.txt").exists());
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(head(&repo), old);
    assert_eq!(read(&repo, "c.txt"), "c\n");
    assert_eq!(status(&repo), "");
    undo::redo_rewrite(repo.path(), &record).unwrap();
    assert!(!repo.path().join("c.txt").exists());
    assert_eq!(status(&repo), "");
}
