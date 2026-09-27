mod common;

use common::Scratch;
use kelp_core::diff::{self, Body};
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

fn kelp_repo(repo: &Scratch) -> gix::Repository {
    gix::discover(repo.path()).unwrap()
}
