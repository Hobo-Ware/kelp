mod common;

use common::Scratch;
use kelp_core::ops::Op;
use kelp_core::status::{UntrackedMode, working_status, working_status_with};
use kelp_core::undo::{self, Outcome};

fn write(repo: &Scratch, path: &str, content: &str) {
    let full = repo.path().join(path);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(full, content).unwrap();
}

fn messy(repo: &Scratch) {
    for i in 0..40 {
        write(repo, &format!(".agent/run/part{}/f{i}.txt", i % 4), "x");
    }
    write(repo, "tools/one.sh", "1");
    write(repo, "tools/deep/two.sh", "2");
    write(repo, "notes.md", "draft");
    write(repo, "build/out.o", "ignored");
    write(repo, ".gitignore", "build/\n");
    repo.git(&["add", ".gitignore"]);
    repo.git(&["commit", "-q", "-m", "ignore"]);
    write(repo, "src/tracked.rs", "fn main() {}");
    repo.git(&["add", "src"]);
    repo.git(&["commit", "-q", "-m", "src"]);
    write(repo, "src/fresh.rs", "fn fresh() {}");
}

#[test]
fn an_untracked_folder_is_one_row_with_its_file_count() {
    let repo = Scratch::new("untracked-folder");
    messy(&repo);
    let status = working_status(repo.path()).unwrap();
    let rows: Vec<&str> = status.unstaged.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(
        rows,
        [".agent/", "notes.md", "src/fresh.rs", "tools/"],
        "folders collapse, single files and files in tracked folders stay"
    );
    assert_eq!(status.untracked, rows);
    let counts: Vec<(&str, usize)> = status
        .folders
        .iter()
        .map(|f| (f.path.as_str(), f.files))
        .collect();
    assert_eq!(counts, [(".agent/", 40), ("tools/", 2)]);
    assert_eq!(status.file_count(".agent/"), 40);
    assert_eq!(status.file_count("notes.md"), 1);
    assert_eq!(status.all().len(), 4);
}

#[test]
fn files_mode_still_lists_every_file() {
    let repo = Scratch::new("untracked-files");
    messy(&repo);
    let status = working_status_with(repo.path(), UntrackedMode::Files).unwrap();
    assert_eq!(status.unstaged.len(), 40 + 2 + 1 + 1);
    assert!(status.folders.is_empty());
    assert!(status.unstaged.iter().all(|c| !c.path.ends_with('/')));
}

#[test]
fn the_count_matches_git_status_on_a_large_folder() {
    let repo = Scratch::new("untracked-large");
    for i in 0..600 {
        write(&repo, &format!("big/d{}/f{i}", i % 7), "x");
    }
    let status = working_status(repo.path()).unwrap();
    assert_eq!(status.unstaged.len(), 1);
    assert_eq!(status.file_count("big/"), 600);
    let git = repo.git(&["status", "--porcelain=v1", "-uall"]);
    assert_eq!(git.lines().count(), 600);
}

#[test]
fn an_ignored_only_folder_and_an_empty_one_do_not_show() {
    let repo = Scratch::new("untracked-ignored");
    write(&repo, ".gitignore", "cache/\n");
    repo.git(&["add", ".gitignore"]);
    repo.git(&["commit", "-q", "-m", "ignore"]);
    write(&repo, "cache/blob", "x");
    std::fs::create_dir_all(repo.path().join("hollow/inner")).unwrap();
    assert!(working_status(repo.path()).unwrap().is_empty());
}

#[test]
fn a_folder_with_spaces_and_unicode_is_counted() {
    let repo = Scratch::new("untracked-names");
    write(&repo, "my stuff/caf\u{e9} 1.txt", "x");
    write(&repo, "my stuff/b.txt", "x");
    let status = working_status(repo.path()).unwrap();
    assert_eq!(status.file_count("my stuff/"), 2);
}

#[test]
fn staging_a_folder_row_stages_every_file_inside() {
    let repo = Scratch::new("untracked-stage");
    messy(&repo);
    Op::Stage(vec![".agent/".into()]).run(repo.path()).unwrap();
    let status = working_status(repo.path()).unwrap();
    assert_eq!(status.staged.len(), 40);
    let rows: Vec<&str> = status.unstaged.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(rows, ["notes.md", "src/fresh.rs", "tools/"]);
}

#[test]
fn deleting_a_folder_row_is_undone_file_by_file() {
    let repo = Scratch::new("untracked-undo");
    messy(&repo);
    let (result, outcome) =
        undo::run_recorded(&Op::DeleteUntracked(vec!["tools/".into()]), repo.path());
    result.unwrap();
    let Outcome::Recorded(record) = outcome else {
        panic!("expected a record, got {outcome:?}");
    };
    assert!(!repo.path().join("tools").exists());
    undo::undo(repo.path(), &record).unwrap();
    assert_eq!(
        std::fs::read_to_string(repo.path().join("tools/deep/two.sh")).unwrap(),
        "2"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("tools/one.sh")).unwrap(),
        "1"
    );
    assert_eq!(working_status(repo.path()).unwrap().file_count("tools/"), 2);
}

#[test]
fn a_nested_repository_counts_as_one_entry() {
    let repo = Scratch::new("untracked-nested");
    let inner = repo.path().join("vendor/lib");
    std::fs::create_dir_all(&inner).unwrap();
    kelp_core::git_cli::run(&inner, &["init", "-q"]).unwrap();
    write(&repo, "vendor/lib/a.txt", "x");
    write(&repo, "vendor/lib/b.txt", "x");
    let status = working_status(repo.path()).unwrap();
    assert_eq!(status.unstaged.len(), 1);
    assert_eq!(status.unstaged[0].path, "vendor/");
    assert_eq!(status.file_count("vendor/"), 1);
}
