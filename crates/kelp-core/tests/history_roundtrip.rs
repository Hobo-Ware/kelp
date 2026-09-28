mod common;

use std::sync::atomic::AtomicBool;

use common::Scratch;
use kelp_core::{blame, file_history};

fn history(repo: &Scratch, path: &str) -> Vec<file_history::Entry> {
    let mut all = Vec::new();
    file_history::stream(repo.path(), path, &AtomicBool::new(false), |batch| {
        all.extend(batch)
    })
    .unwrap();
    all
}

#[test]
fn history_follows_a_file_through_two_renames() {
    let repo = Scratch::new("history-renames");
    repo.commit("lexer.rs", "fn lex() {}\n", "add a lexer");
    repo.git(&["mv", "lexer.rs", "parse.rs"]);
    repo.git(&["commit", "-q", "-m", "rename to parse"]);
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    repo.git(&["mv", "parse.rs", "src/parse.rs"]);
    repo.git(&["commit", "-q", "-m", "move into src"]);
    repo.commit("src/parse.rs", "fn lex() {}\nfn parse() {}\n", "add parse");

    let entries = history(&repo, "src/parse.rs");
    let titles: Vec<&str> = entries.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "add parse",
            "move into src",
            "rename to parse",
            "add a lexer"
        ]
    );
    assert_eq!(entries[0].path, "src/parse.rs");
    assert_eq!((entries[0].added, entries[0].removed), (Some(1), Some(0)));
    assert_eq!(entries[1].renamed_from.as_deref(), Some("parse.rs"));
    assert_eq!(entries[2].renamed_from.as_deref(), Some("lexer.rs"));
    assert_eq!(entries[3].path, "lexer.rs");
}

#[test]
fn cancelled_history_stops_quietly() {
    let repo = Scratch::new("history-cancel");
    let mut got = 0;
    file_history::stream(repo.path(), "a.txt", &AtomicBool::new(true), |b| {
        got += b.len()
    })
    .unwrap();
    assert_eq!(got, 0);
}

#[test]
fn blame_attributes_lines_to_both_authors_and_the_work_tree() {
    let repo = Scratch::new("blame-authors");
    repo.commit("list.rs", "one\ntwo\nthree\n", "add the list");
    std::fs::write(repo.path().join("list.rs"), "one\nTWO\nthree\n").unwrap();
    repo.git(&["add", "."]);
    repo.git(&[
        "-c",
        "user.name=Sam",
        "-c",
        "user.email=sam@example.com",
        "commit",
        "-q",
        "-m",
        "shout two",
    ]);
    std::fs::write(repo.path().join("list.rs"), "one\nTWO\nthree\nfour\n").unwrap();

    let head = gix::open(repo.path()).unwrap().head_id().unwrap().detach();
    let at_head = blame::run(repo.path(), Some(head), "list.rs", &AtomicBool::new(false))
        .unwrap()
        .unwrap();
    let authors: Vec<&str> = (0..3)
        .map(|i| at_head.origin(i).unwrap().author.as_str())
        .collect();
    assert_eq!(authors, ["Test", "Sam", "Test"]);
    assert_eq!(at_head.origin(1).unwrap().summary, "shout two");
    assert!(at_head.origin(1).unwrap().previous.is_some());

    let working = blame::run(repo.path(), None, "list.rs", &AtomicBool::new(false))
        .unwrap()
        .unwrap();
    assert_eq!(working.lines.len(), 4);
    assert!(working.origin(3).unwrap().is_uncommitted());
    assert_eq!(working.lines[3].text, "four");

    let (before, path) = at_head.origin(1).unwrap().previous.clone().unwrap();
    let earlier = blame::run(repo.path(), Some(before), &path, &AtomicBool::new(false))
        .unwrap()
        .unwrap();
    assert_eq!(earlier.lines[1].text, "two");
}

#[test]
fn cancelled_blame_returns_nothing() {
    let repo = Scratch::new("blame-cancel");
    assert!(
        blame::run(repo.path(), None, "a.txt", &AtomicBool::new(true))
            .unwrap()
            .is_none()
    );
}
