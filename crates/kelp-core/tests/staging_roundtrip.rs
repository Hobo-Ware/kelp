mod common;

use std::collections::BTreeSet;

use common::Scratch;
use kelp_core::diff::{self, Body, LineKind};
use kelp_core::ops::Op;
use kelp_core::status::working_status;

fn numbered(lines: usize) -> String {
    (1..=lines).map(|i| format!("line {i}\n")).collect()
}

fn hunks(diff: &diff::FileDiff) -> usize {
    match &diff.body {
        Body::Text(h) => h.len(),
        _ => 0,
    }
}

#[test]
fn stage_and_unstage_a_single_hunk() {
    let repo = Scratch::new("hunks");
    repo.commit("f.txt", &numbered(30), "thirty lines");
    let edited = numbered(30)
        .replace("line 2\n", "line two\n")
        .replace("line 27\n", "line twenty-seven\n");
    std::fs::write(repo.path().join("f.txt"), edited).unwrap();

    let unstaged = diff::unstaged_file(&kelp_repo(&repo), repo.path(), "f.txt").unwrap();
    assert_eq!(hunks(&unstaged), 2);
    let patch = diff::hunk_patch(&unstaged, 1).unwrap();
    Op::ApplyToIndex {
        patch,
        reverse: false,
    }
    .run(repo.path())
    .unwrap();

    let cached = repo.git(&["diff", "--cached"]);
    assert!(
        cached.contains("+line twenty-seven") && !cached.contains("+line two\n"),
        "{cached}"
    );
    let worktree = repo.git(&["diff"]);
    assert!(
        worktree.contains("+line two") && !worktree.contains("twenty-seven"),
        "{worktree}"
    );

    let staged = diff::staged_file(&kelp_repo(&repo), "f.txt").unwrap();
    assert_eq!(hunks(&staged), 1);
    let patch = diff::hunk_patch(&staged, 0).unwrap();
    Op::ApplyToIndex {
        patch,
        reverse: true,
    }
    .run(repo.path())
    .unwrap();
    assert!(repo.git(&["diff", "--cached"]).is_empty());
}

#[test]
fn hunk_staging_handles_missing_newline_and_crlf() {
    let repo = Scratch::new("eol");
    repo.commit("n.txt", "a\nb", "no trailing newline");
    repo.commit("w.txt", "one\r\ntwo\r\nthree\r\n", "crlf");
    std::fs::write(repo.path().join("n.txt"), "a\nc").unwrap();
    std::fs::write(repo.path().join("w.txt"), "one\r\n2\r\nthree\r\n").unwrap();
    for path in ["n.txt", "w.txt"] {
        let unstaged = diff::unstaged_file(&kelp_repo(&repo), repo.path(), path).unwrap();
        let patch = diff::hunk_patch(&unstaged, 0).unwrap();
        Op::ApplyToIndex {
            patch,
            reverse: false,
        }
        .run(repo.path())
        .unwrap();
        assert!(
            repo.git(&["diff", "--", path]).is_empty(),
            "{path} still has unstaged changes"
        );
    }
}

#[test]
fn stage_unstage_files_and_all() {
    let repo = Scratch::new("files");
    std::fs::write(repo.path().join("a.txt"), "changed").unwrap();
    std::fs::write(repo.path().join("new.txt"), "new").unwrap();

    let status = working_status(repo.path()).unwrap();
    assert_eq!(status.unstaged.len(), 2);
    assert!(status.staged.is_empty());

    Op::Stage(vec!["new.txt".into()]).run(repo.path()).unwrap();
    let status = working_status(repo.path()).unwrap();
    assert_eq!(
        status
            .staged
            .iter()
            .map(|c| c.path.as_str())
            .collect::<Vec<_>>(),
        ["new.txt"]
    );

    Op::Unstage {
        paths: vec!["new.txt".into()],
        has_head: true,
    }
    .run(repo.path())
    .unwrap();
    assert!(working_status(repo.path()).unwrap().staged.is_empty());

    Op::StageAll.run(repo.path()).unwrap();
    let status = working_status(repo.path()).unwrap();
    assert_eq!((status.staged.len(), status.unstaged.len()), (2, 0));

    Op::UnstageAll { has_head: true }.run(repo.path()).unwrap();
    let status = working_status(repo.path()).unwrap();
    assert_eq!((status.staged.len(), status.unstaged.len()), (0, 2));
}

#[test]
fn commit_then_amend() {
    let repo = Scratch::new("commit");
    std::fs::write(repo.path().join("c.txt"), "c").unwrap();
    Op::Stage(vec!["c.txt".into()]).run(repo.path()).unwrap();
    Op::Commit {
        message: "feat: add c\n\nWith a body line.".into(),
        amend: false,
    }
    .run(repo.path())
    .unwrap();
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s%n%b"]).trim(),
        "feat: add c\nWith a body line."
    );
    let count = repo.git(&["rev-list", "--count", "HEAD"]);

    Op::Commit {
        message: "feat: add the letter c".into(),
        amend: true,
    }
    .run(repo.path())
    .unwrap();
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s"]).trim(),
        "feat: add the letter c"
    );
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]), count);
    assert!(working_status(repo.path()).unwrap().is_empty());
}

#[test]
fn discard_changes_and_untracked_files() {
    let repo = Scratch::new("discard");
    std::fs::write(repo.path().join("a.txt"), "oops").unwrap();
    std::fs::write(repo.path().join("junk.txt"), "junk").unwrap();
    Op::DiscardChanges(vec!["a.txt".into()])
        .run(repo.path())
        .unwrap();
    Op::DeleteUntracked(vec!["junk.txt".into()])
        .run(repo.path())
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "two"
    );
    assert!(!repo.path().join("junk.txt").exists());
    assert!(working_status(repo.path()).unwrap().is_empty());
}

#[test]
fn unstage_works_before_the_first_commit() {
    let dir = std::env::temp_dir().join(format!("kelp-test-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    kelp_core::git_cli::run(&dir, &["init", "-q"]).unwrap();
    std::fs::write(dir.join("first.txt"), "hi").unwrap();
    Op::Stage(vec!["first.txt".into()]).run(&dir).unwrap();
    Op::Unstage {
        paths: vec!["first.txt".into()],
        has_head: false,
    }
    .run(&dir)
    .unwrap();
    assert!(working_status(&dir).unwrap().staged.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

fn line_index(diff: &diff::FileDiff, kind: LineKind, text: &str) -> usize {
    let Body::Text(hunks) = &diff.body else {
        panic!("expected a text diff")
    };
    hunks
        .iter()
        .flat_map(|h| &h.lines)
        .position(|l| l.kind == kind && l.text == text)
        .unwrap_or_else(|| panic!("no {kind:?} line {text:?}"))
}

fn apply_lines(repo: &Scratch, path: &str, picks: &[(LineKind, &str)], staged: bool) {
    let git = kelp_repo(repo);
    let diff = if staged {
        diff::staged_file(&git, path).unwrap()
    } else {
        diff::unstaged_file(&git, repo.path(), path).unwrap()
    };
    let selected: BTreeSet<usize> = picks
        .iter()
        .map(|(kind, text)| line_index(&diff, *kind, text))
        .collect();
    let patch = diff::lines_patch(&diff, &selected, staged).unwrap();
    Op::ApplyToIndex {
        patch,
        reverse: staged,
    }
    .run(repo.path())
    .unwrap();
}

fn index_content(repo: &Scratch, path: &str) -> String {
    repo.git(&["show", &format!(":{path}")])
}

#[test]
fn stage_one_of_three_added_lines_then_unstage_it() {
    let repo = Scratch::new("lines-add");
    repo.commit("f.txt", "a\nb\n", "two lines");
    std::fs::write(repo.path().join("f.txt"), "a\nx\ny\nz\nb\n").unwrap();

    apply_lines(&repo, "f.txt", &[(LineKind::Added, "y")], false);
    assert_eq!(index_content(&repo, "f.txt"), "a\ny\nb\n");
    let unstaged = repo.git(&["diff"]);
    assert!(
        unstaged.contains("+x\n") && unstaged.contains("+z\n"),
        "{unstaged}"
    );

    apply_lines(&repo, "f.txt", &[(LineKind::Added, "y")], true);
    assert_eq!(index_content(&repo, "f.txt"), "a\nb\n");
}

#[test]
fn stage_a_single_removal() {
    let repo = Scratch::new("lines-remove");
    repo.commit("f.txt", "a\nb\nc\nd\n", "four lines");
    std::fs::write(repo.path().join("f.txt"), "a\nd\n").unwrap();

    apply_lines(&repo, "f.txt", &[(LineKind::Removed, "c")], false);
    assert_eq!(index_content(&repo, "f.txt"), "a\nb\nd\n");

    apply_lines(&repo, "f.txt", &[(LineKind::Removed, "c")], true);
    assert_eq!(index_content(&repo, "f.txt"), "a\nb\nc\nd\n");
}

#[test]
fn line_staging_keeps_crlf() {
    let repo = Scratch::new("lines-crlf");
    repo.commit("w.txt", "one\r\ntwo\r\nthree\r\n", "crlf");
    std::fs::write(repo.path().join("w.txt"), "one\r\n2\r\nthree\r\nfour\r\n").unwrap();

    apply_lines(&repo, "w.txt", &[(LineKind::Added, "four")], false);
    assert_eq!(
        index_content(&repo, "w.txt"),
        "one\r\ntwo\r\nthree\r\nfour\r\n"
    );
}

#[test]
fn line_staging_handles_a_missing_final_newline() {
    let repo = Scratch::new("lines-eof");
    repo.commit("n.txt", "a\nb", "no trailing newline");
    std::fs::write(repo.path().join("n.txt"), "a\nc").unwrap();

    apply_lines(&repo, "n.txt", &[(LineKind::Added, "c")], false);
    assert_eq!(index_content(&repo, "n.txt"), "a\nb\nc");

    apply_lines(&repo, "n.txt", &[(LineKind::Removed, "b")], false);
    assert_eq!(index_content(&repo, "n.txt"), "a\nc");
    assert!(repo.git(&["diff", "--", "n.txt"]).is_empty());
}

fn discard_lines(repo: &Scratch, path: &str, picks: &[(LineKind, &str)]) {
    let diff = diff::unstaged_file(&kelp_repo(repo), repo.path(), path).unwrap();
    let selected: BTreeSet<usize> = picks
        .iter()
        .map(|(kind, text)| line_index(&diff, *kind, text))
        .collect();
    let patch = diff::discard_lines_patch(&diff, &selected).unwrap();
    Op::DiscardPatch(patch).run(repo.path()).unwrap();
}

fn worktree_content(repo: &Scratch, path: &str) -> String {
    std::fs::read_to_string(repo.path().join(path)).unwrap()
}

#[test]
fn discard_one_hunk_of_two() {
    let repo = Scratch::new("discard-hunk");
    repo.commit("f.txt", &numbered(30), "thirty lines");
    let edited = numbered(30)
        .replace("line 2\n", "line two\n")
        .replace("line 27\n", "line twenty-seven\n");
    std::fs::write(repo.path().join("f.txt"), &edited).unwrap();

    let unstaged = diff::unstaged_file(&kelp_repo(&repo), repo.path(), "f.txt").unwrap();
    let patch = diff::hunk_patch(&unstaged, 0).unwrap();
    Op::DiscardPatch(patch).run(repo.path()).unwrap();
    assert_eq!(
        worktree_content(&repo, "f.txt"),
        numbered(30).replace("line 27\n", "line twenty-seven\n")
    );
    assert!(repo.git(&["diff", "--cached"]).is_empty());
}

#[test]
fn discard_one_of_three_added_lines() {
    let repo = Scratch::new("discard-add");
    repo.commit("f.txt", "a\nb\n", "two lines");
    std::fs::write(repo.path().join("f.txt"), "a\nx\ny\nz\nb\n").unwrap();
    discard_lines(&repo, "f.txt", &[(LineKind::Added, "y")]);
    assert_eq!(worktree_content(&repo, "f.txt"), "a\nx\nz\nb\n");
    assert_eq!(index_content(&repo, "f.txt"), "a\nb\n");
}

#[test]
fn discard_a_single_removal_restores_the_line() {
    let repo = Scratch::new("discard-remove");
    repo.commit("f.txt", "a\nb\nc\nd\n", "four lines");
    std::fs::write(repo.path().join("f.txt"), "a\nd\n").unwrap();
    discard_lines(&repo, "f.txt", &[(LineKind::Removed, "c")]);
    assert_eq!(worktree_content(&repo, "f.txt"), "a\nc\nd\n");
}

#[test]
fn discarding_lines_keeps_crlf_and_a_missing_final_newline() {
    let repo = Scratch::new("discard-eol");
    repo.commit("w.txt", "one\r\ntwo\r\nthree\r\n", "crlf");
    repo.commit("n.txt", "a\nb", "no trailing newline");
    std::fs::write(repo.path().join("w.txt"), "one\r\n2\r\nthree\r\nfour\r\n").unwrap();
    std::fs::write(repo.path().join("n.txt"), "a\nc").unwrap();

    discard_lines(&repo, "w.txt", &[(LineKind::Added, "four")]);
    assert_eq!(worktree_content(&repo, "w.txt"), "one\r\n2\r\nthree\r\n");

    discard_lines(
        &repo,
        "n.txt",
        &[(LineKind::Removed, "b"), (LineKind::Added, "c")],
    );
    assert_eq!(worktree_content(&repo, "n.txt"), "a\nb");
    assert!(repo.git(&["diff", "--", "n.txt"]).is_empty());
}

#[test]
fn undo_restores_discarded_lines() {
    let repo = Scratch::new("discard-undo");
    repo.commit("f.txt", "a\nb\n", "two lines");
    std::fs::write(repo.path().join("f.txt"), "a\nx\ny\nb\n").unwrap();
    let diff = diff::unstaged_file(&kelp_repo(&repo), repo.path(), "f.txt").unwrap();
    let selected = BTreeSet::from([line_index(&diff, LineKind::Added, "y")]);
    let op = Op::DiscardPatch(diff::discard_lines_patch(&diff, &selected).unwrap());
    let (result, outcome) = kelp_core::undo::run_recorded(&op, repo.path());
    result.unwrap();
    assert_eq!(worktree_content(&repo, "f.txt"), "a\nx\nb\n");
    let kelp_core::undo::Outcome::Recorded(record) = outcome else {
        panic!("discarding lines should be undoable");
    };
    kelp_core::undo::undo(repo.path(), &record).unwrap();
    assert_eq!(worktree_content(&repo, "f.txt"), "a\nx\ny\nb\n");
}

#[test]
fn stash_one_of_three_files_keeps_the_others_dirty() {
    let repo = Scratch::new("stash-files");
    repo.commit("a.txt", "a\n", "a");
    repo.commit("b.txt", "b\n", "b");
    std::fs::write(repo.path().join("a.txt"), "a2\n").unwrap();
    std::fs::write(repo.path().join("b.txt"), "b2\n").unwrap();
    std::fs::write(repo.path().join("new.txt"), "new\n").unwrap();
    Op::StashFiles {
        paths: vec!["b.txt".into(), "new.txt".into()],
        message: "just b".into(),
    }
    .run(repo.path())
    .unwrap();
    assert_eq!(worktree_content(&repo, "a.txt"), "a2\n");
    assert_eq!(worktree_content(&repo, "b.txt"), "b\n");
    assert!(!repo.path().join("new.txt").exists());
    assert!(repo.git(&["stash", "list"]).contains("just b"));
}

#[test]
fn discard_several_files_tracked_and_untracked_with_undo() {
    let repo = Scratch::new("discard-files");
    repo.commit("a.txt", "a\n", "a");
    repo.commit("b.txt", "b\n", "b");
    std::fs::write(repo.path().join("a.txt"), "a2\n").unwrap();
    std::fs::write(repo.path().join("b.txt"), "b2\n").unwrap();
    std::fs::write(repo.path().join("new.txt"), "new\n").unwrap();
    let op = Op::DiscardFiles {
        tracked: vec!["a.txt".into()],
        untracked: vec!["new.txt".into()],
    };
    assert_eq!(
        op.command_line(),
        "git restore -- a.txt && git clean -f -- new.txt"
    );
    let (result, outcome) = kelp_core::undo::run_recorded(&op, repo.path());
    result.unwrap();
    assert_eq!(worktree_content(&repo, "a.txt"), "a\n");
    assert_eq!(worktree_content(&repo, "b.txt"), "b2\n");
    assert!(!repo.path().join("new.txt").exists());
    let kelp_core::undo::Outcome::Recorded(record) = outcome else {
        panic!("discarding files should be undoable");
    };
    kelp_core::undo::undo(repo.path(), &record).unwrap();
    assert_eq!(worktree_content(&repo, "a.txt"), "a2\n");
    assert_eq!(worktree_content(&repo, "new.txt"), "new\n");
}

#[test]
fn stash_staged_only() {
    let repo = Scratch::new("stash-staged");
    repo.commit("a.txt", "a\n", "a");
    repo.commit("b.txt", "b\n", "b");
    std::fs::write(repo.path().join("a.txt"), "a2\n").unwrap();
    std::fs::write(repo.path().join("b.txt"), "b2\n").unwrap();
    repo.git(&["add", "a.txt"]);
    Op::StashStaged {
        message: String::new(),
    }
    .run(repo.path())
    .unwrap();
    assert_eq!(worktree_content(&repo, "a.txt"), "a\n");
    assert_eq!(worktree_content(&repo, "b.txt"), "b2\n");
    assert!(repo.git(&["diff", "--cached"]).is_empty());
}

fn kelp_repo(repo: &Scratch) -> gix::Repository {
    gix::discover(repo.path()).unwrap()
}
