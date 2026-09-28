mod common;

use common::Scratch;
use gix::ObjectId;
use kelp_core::commit::ChangeKind;
use kelp_core::compare::{Side, changed_files};
use kelp_core::diff::{self, Body};
use kelp_core::filter::{self, Filter};
use kelp_core::history::History;

fn head(repo: &Scratch) -> ObjectId {
    ObjectId::from_hex(repo.git(&["rev-parse", "HEAD"]).trim().as_bytes()).unwrap()
}

fn rename_repo(name: &str) -> (Scratch, ObjectId, ObjectId) {
    let repo = Scratch::new(name);
    std::fs::create_dir_all(repo.path().join("old")).unwrap();
    repo.commit(
        "old/notes.txt",
        "alpha\nbeta\ngamma\ndelta\nepsilon\n",
        "add notes",
    );
    let base = head(&repo);
    std::fs::create_dir_all(repo.path().join("new")).unwrap();
    repo.git(&["mv", "old/notes.txt", "new/notes.txt"]);
    std::fs::write(
        repo.path().join("new/notes.txt"),
        "alpha\nbeta\ngamma\ndelta\nepsilon\nzeta\n",
    )
    .unwrap();
    repo.commit("b.txt", "bee", "move notes, add b");
    let target = head(&repo);
    (repo, base, target)
}

#[test]
fn range_lists_renames_and_diffs_across_them() {
    let (repo, base, target) = rename_repo("compare-range");
    let changes = changed_files(repo.path(), base, Side::Commit(target)).unwrap();
    let moved = changes.iter().find(|c| c.path == "new/notes.txt").unwrap();
    assert_eq!(moved.kind, ChangeKind::Renamed);
    assert_eq!(moved.old_path.as_deref(), Some("old/notes.txt"));
    assert!(
        changes
            .iter()
            .any(|c| c.path == "b.txt" && c.kind == ChangeKind::Added)
    );

    let git = gix::open(repo.path()).unwrap();
    let file = diff::range_file(
        &git,
        Some(repo.path()),
        base,
        Some(target),
        "new/notes.txt",
        moved.old_path.as_deref(),
    )
    .unwrap();
    assert_eq!((file.added, file.removed), (1, 0));
    assert!(matches!(file.body, Body::Text(_)));
}

#[test]
fn range_against_the_working_tree_sees_unsaved_edits() {
    let (repo, base, _) = rename_repo("compare-worktree");
    std::fs::write(repo.path().join("b.txt"), "bee\nand more\n").unwrap();
    let changes = changed_files(repo.path(), base, Side::WorkTree).unwrap();
    assert!(changes.iter().any(|c| c.path == "b.txt"));
    let git = gix::open(repo.path()).unwrap();
    let file = diff::range_file(&git, Some(repo.path()), base, None, "b.txt", None).unwrap();
    assert_eq!(file.new_text.as_deref(), Some("bee\nand more\n"));
    assert_eq!(file.old_text.as_deref(), Some(""));
}

#[test]
fn path_filter_follows_both_sides_of_a_rename() {
    let (repo, base, target) = rename_repo("compare-paths");
    let (_, history) = History::open(repo.path()).unwrap();
    let under = |path: &str| {
        let filter = Filter {
            path: path.into(),
            ..Filter::default()
        };
        let rows = filter::matching_rows(repo.path(), history.ids(), &filter, 0, &|| false)
            .unwrap()
            .unwrap();
        history
            .ids()
            .iter()
            .zip(rows)
            .filter_map(|(id, keep)| keep.then_some(*id))
            .collect::<Vec<_>>()
    };
    assert_eq!(under("old/"), vec![target, base]);
    assert_eq!(under("new/"), vec![target]);
    assert!(under("nowhere/").is_empty());
}

#[test]
fn path_filter_keeps_the_merge_that_brings_a_change_in() {
    let repo = Scratch::new("compare-merge");
    repo.git(&["checkout", "-q", "-b", "side"]);
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    repo.commit("src/lib.rs", "fn side() {}\n", "side work");
    let side = head(&repo);
    repo.git(&["checkout", "-q", "main"]);
    repo.commit("docs.txt", "docs\n", "main work");
    repo.git(&["merge", "-q", "--no-ff", "-m", "merge side", "side"]);
    let merge = head(&repo);
    let (_, history) = History::open(repo.path()).unwrap();
    let filter = Filter {
        path: "src".into(),
        ..Filter::default()
    };
    let rows = filter::matching_rows(repo.path(), history.ids(), &filter, 0, &|| false)
        .unwrap()
        .unwrap();
    let kept: Vec<ObjectId> = history
        .ids()
        .iter()
        .zip(rows)
        .filter_map(|(id, keep)| keep.then_some(*id))
        .collect();
    assert!(kept.contains(&side));
    assert!(kept.contains(&merge));
    assert_eq!(kept.len(), 2);
}

#[test]
fn filters_combine_and_can_be_cancelled() {
    let (repo, _, target) = rename_repo("compare-combined");
    let (_, history) = History::open(repo.path()).unwrap();
    let filter = Filter {
        author: "test".into(),
        path: "new/".into(),
        ..Filter::default()
    };
    let rows = filter::matching_rows(repo.path(), history.ids(), &filter, 0, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(history.ids()[rows.iter().position(|&k| k).unwrap()], target);
    assert_eq!(rows.iter().filter(|&&k| k).count(), 1);
    let cancelled =
        filter::matching_rows(repo.path(), history.ids(), &filter, 0, &|| true).unwrap();
    assert!(cancelled.is_none());
    assert_eq!(
        filter::authors(repo.path(), history.ids()).unwrap(),
        vec!["Test".to_string()]
    );
    assert_eq!(
        filter::user_email(repo.path()).as_deref(),
        Some("test@example.com")
    );
}
