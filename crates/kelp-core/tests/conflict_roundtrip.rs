mod common;

use common::Scratch;
use kelp_core::conflict::{self, Choice, Operation, Side, Step};
use kelp_core::status;

fn try_git(repo: &Scratch, args: &[&str]) {
    let _ = kelp_core::git_cli::run(repo.path(), args);
}

fn conflicted_merge(name: &str) -> Scratch {
    let repo = Scratch::new(name);
    repo.git(&["checkout", "-q", "-b", "feat/x"]);
    repo.commit("a.txt", "theirs\n", "theirs");
    repo.git(&["checkout", "-q", "main"]);
    repo.commit("a.txt", "ours\n", "ours");
    try_git(&repo, &["merge", "feat/x"]);
    repo
}

fn git_dir(repo: &Scratch) -> std::path::PathBuf {
    repo.path().join(".git")
}

fn read(repo: &Scratch) -> String {
    std::fs::read_to_string(repo.path().join("a.txt")).unwrap()
}

fn resolve_with(repo: &Scratch, choice: Choice) -> String {
    let parsed = conflict::parse(&read(repo));
    assert_eq!(parsed.conflict_count(), 1);
    let text = parsed.render(&[Some(choice)]);
    conflict::mark_resolved(repo.path(), "a.txt", Some(&text)).unwrap();
    text
}

#[test]
fn merge_is_detected_with_its_branch() {
    let repo = conflicted_merge("conflict-detect");
    let found = conflict::in_progress(&git_dir(&repo)).unwrap();
    assert_eq!(found.operation, Operation::Merge);
    assert_eq!(found.subject, "feat/x");
    let working = status::working_status(repo.path()).unwrap();
    assert_eq!(working.conflicted, vec!["a.txt".to_string()]);
}

#[test]
fn resolve_each_way_then_continue() {
    for (choice, expected) in [
        (Choice::Ours, "ours\n"),
        (Choice::Theirs, "theirs\n"),
        (Choice::Both, "ours\ntheirs\n"),
    ] {
        let repo = conflicted_merge(&format!("conflict-{choice:?}"));
        assert_eq!(resolve_with(&repo, choice), expected);
        assert!(
            status::working_status(repo.path())
                .unwrap()
                .conflicted
                .is_empty()
        );
        conflict::run_step(repo.path(), Operation::Merge, Step::Continue).unwrap();
        assert!(conflict::in_progress(&git_dir(&repo)).is_none());
        assert_eq!(read(&repo), expected);
        let parents = repo.git(&["log", "-1", "--format=%P"]);
        assert_eq!(parents.split_whitespace().count(), 2);
    }
}

#[test]
fn whole_file_sides() {
    let repo = conflicted_merge("conflict-sides");
    conflict::take_side(repo.path(), "a.txt", Side::Theirs).unwrap();
    assert_eq!(read(&repo), "theirs\n");
    assert!(
        status::working_status(repo.path())
            .unwrap()
            .conflicted
            .is_empty()
    );
}

#[test]
fn abort_restores_the_branch() {
    let repo = conflicted_merge("conflict-abort");
    conflict::run_step(repo.path(), Operation::Merge, Step::Abort).unwrap();
    assert!(conflict::in_progress(&git_dir(&repo)).is_none());
    assert_eq!(read(&repo), "ours\n");
    assert!(
        status::working_status(repo.path())
            .unwrap()
            .all()
            .is_empty()
    );
}

#[test]
fn deleted_on_one_side_takes_the_deletion() {
    let repo = Scratch::new("conflict-delete");
    repo.git(&["checkout", "-q", "-b", "feat/x"]);
    repo.git(&["rm", "-q", "a.txt"]);
    repo.git(&["commit", "-q", "-m", "drop"]);
    repo.git(&["checkout", "-q", "main"]);
    repo.commit("a.txt", "changed\n", "change");
    try_git(&repo, &["merge", "feat/x"]);
    let stages = conflict::stages(repo.path(), "a.txt").unwrap();
    assert!(stages.ours && !stages.theirs);
    conflict::take_side(repo.path(), "a.txt", Side::Theirs).unwrap();
    assert!(!repo.path().join("a.txt").exists());
    conflict::run_step(repo.path(), Operation::Merge, Step::Continue).unwrap();
}

fn conflicted_rebase(name: &str) -> Scratch {
    let repo = Scratch::new(name);
    repo.git(&["checkout", "-q", "-b", "feat/x"]);
    repo.commit("a.txt", "feature\n", "feature change");
    repo.commit("b.txt", "extra\n", "feature extra");
    repo.git(&["checkout", "-q", "main"]);
    repo.commit("a.txt", "main\n", "main change");
    repo.git(&["checkout", "-q", "feat/x"]);
    try_git(&repo, &["rebase", "main"]);
    repo
}

#[test]
fn rebase_continue_after_resolving() {
    let repo = conflicted_rebase("conflict-rebase");
    let found = conflict::in_progress(&git_dir(&repo)).unwrap();
    assert_eq!(found.operation, Operation::Rebase);
    assert_eq!(found.subject, "feat/x");
    resolve_with(&repo, Choice::Theirs);
    conflict::run_step(repo.path(), Operation::Rebase, Step::Continue).unwrap();
    assert!(conflict::in_progress(&git_dir(&repo)).is_none());
    assert_eq!(read(&repo), "feature\n");
    assert!(repo.path().join("b.txt").exists());
}

#[test]
fn rebase_skip_drops_the_commit() {
    let repo = conflicted_rebase("conflict-skip");
    conflict::run_step(repo.path(), Operation::Rebase, Step::Skip).unwrap();
    assert!(conflict::in_progress(&git_dir(&repo)).is_none());
    assert_eq!(read(&repo), "main\n");
    let log = repo.git(&["log", "--format=%s"]);
    assert!(!log.contains("feature change"));
    assert!(log.contains("feature extra"));
}

#[test]
fn cherry_pick_and_revert_are_detected() {
    let repo = Scratch::new("conflict-pick");
    repo.git(&["checkout", "-q", "-b", "feat/x"]);
    repo.commit("a.txt", "picked\n", "pick me");
    let pick = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.git(&["checkout", "-q", "main"]);
    repo.commit("a.txt", "main\n", "main change");
    try_git(&repo, &["cherry-pick", &pick]);
    let found = conflict::in_progress(&git_dir(&repo)).unwrap();
    assert_eq!(found.operation, Operation::CherryPick);
    assert_eq!(found.subject, pick[..7]);
    conflict::run_step(repo.path(), Operation::CherryPick, Step::Abort).unwrap();

    repo.commit("a.txt", "later\n", "later change");
    let target = repo.git(&["rev-parse", "HEAD~1"]).trim().to_string();
    try_git(&repo, &["revert", "--no-edit", &target]);
    let found = conflict::in_progress(&git_dir(&repo)).unwrap();
    assert_eq!(found.operation, Operation::Revert);
    resolve_with(&repo, Choice::Theirs);
    conflict::run_step(repo.path(), Operation::Revert, Step::Continue).unwrap();
    assert!(conflict::in_progress(&git_dir(&repo)).is_none());
}
