mod common;

use common::Scratch;
use kelp_core::ops::{Op, ResetMode};
use kelp_core::reflog::{self, Action};
use kelp_core::undo::{self, Outcome};

fn head(repo: &Scratch) -> String {
    repo.git(&["rev-parse", "HEAD"]).trim().to_string()
}

fn oid(hex: &str) -> gix::ObjectId {
    gix::ObjectId::from_hex(hex.as_bytes()).unwrap()
}

#[test]
fn a_reset_commit_shows_as_lost_and_can_be_restored_then_undone() {
    let repo = Scratch::new("reflog-lost");
    repo.commit("b.txt", "three", "third");
    let lost = head(&repo);
    repo.git(&["reset", "-q", "--hard", "HEAD~1"]);
    let before_restore = head(&repo);

    let entries = reflog::load(repo.path(), "HEAD").unwrap();
    assert_eq!(entries[0].action, Action::Reset);
    assert_eq!(entries[0].message, "moving to HEAD~1");
    assert!(entries[0].time > 0);
    let third = entries
        .iter()
        .find(|e| e.id == oid(&lost))
        .expect("the lost commit is in the reflog");
    assert_eq!(third.title, "third");
    assert!(!third.reachable);
    assert!(
        entries
            .iter()
            .any(|e| e.id == oid(&before_restore) && e.reachable)
    );

    let op = Op::Reset {
        commit: lost.clone(),
        mode: ResetMode::Keep,
    };
    let (result, outcome) = undo::run_recorded(&op, repo.path());
    result.unwrap();
    assert_eq!(head(&repo), lost);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("b.txt")).unwrap(),
        "three"
    );
    let reloaded = reflog::load(repo.path(), "HEAD").unwrap();
    assert!(
        reloaded
            .iter()
            .filter(|e| e.id == oid(&lost))
            .all(|e| e.reachable)
    );

    let Outcome::Recorded(record) = outcome else {
        panic!("restore should be undoable");
    };
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(head(&repo), before_restore);
    assert!(!repo.path().join("b.txt").exists());
}

#[test]
fn restore_with_keep_refuses_to_overwrite_local_edits() {
    let repo = Scratch::new("reflog-keep");
    repo.commit("a.txt", "three", "third");
    let lost = head(&repo);
    repo.git(&["reset", "-q", "--hard", "HEAD~1"]);
    std::fs::write(repo.path().join("a.txt"), "local edit").unwrap();
    let op = Op::Reset {
        commit: lost,
        mode: ResetMode::Keep,
    };
    assert!(op.run(repo.path()).is_err());
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "local edit"
    );
}

#[test]
fn branch_reflogs_and_checkouts_are_listed() {
    let repo = Scratch::new("reflog-branch");
    repo.git(&["checkout", "-q", "-b", "feat/x"]);
    repo.commit("c.txt", "c", "on feat");
    repo.git(&["checkout", "-q", "main"]);
    let head_log = reflog::load(repo.path(), "HEAD").unwrap();
    assert_eq!(head_log[0].action, Action::Checkout);
    assert_eq!(head_log[0].message, "moving from feat/x to main");
    let branch_log = reflog::load(repo.path(), "feat/x").unwrap();
    assert_eq!(branch_log[0].action, Action::Commit);
    assert_eq!(branch_log[0].title, "on feat");
    let mut branches = reflog::local_branches(repo.path());
    branches.sort();
    assert_eq!(branches, ["feat/x", "main"]);
}

#[test]
fn untracked_files_in_a_stash_are_listed() {
    let repo = Scratch::new("reflog-stash-u");
    std::fs::write(repo.path().join("a.txt"), "edited").unwrap();
    std::fs::create_dir_all(repo.path().join("notes")).unwrap();
    std::fs::write(repo.path().join("notes/new.txt"), "fresh").unwrap();
    repo.git(&["stash", "push", "-q", "-u", "-m", "with untracked"]);
    let (id, files) = reflog::stash_untracked(repo.path(), "stash@{0}").unwrap();
    assert_eq!(files, ["notes/new.txt"]);
    assert_eq!(repo.git(&["show", &format!("{id}:notes/new.txt")]), "fresh");
    std::fs::write(repo.path().join("a.txt"), "again").unwrap();
    repo.git(&["stash", "push", "-q", "-m", "tracked only"]);
    assert!(reflog::stash_untracked(repo.path(), "stash@{0}").is_none());
}
