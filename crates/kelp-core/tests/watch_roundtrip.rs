mod common;

use std::sync::mpsc;
use std::time::Duration;

use common::Scratch;
use kelp_core::watch::{Change, Layout, Watcher};

fn watch(repo: &Scratch) -> (Watcher, mpsc::Receiver<Change>) {
    let (tx, rx) = mpsc::channel();
    let git = gix::open(repo.path()).unwrap();
    let watcher = Watcher::start(Layout::of(&git), move |c| {
        let _ = tx.send(c);
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    while rx.try_recv().is_ok() {}
    (watcher, rx)
}

fn next(rx: &mpsc::Receiver<Change>) -> Option<Change> {
    rx.recv_timeout(Duration::from_secs(5)).ok()
}

#[test]
fn cli_commit_reloads_history() {
    let repo = Scratch::new("watch-commit");
    let (_w, rx) = watch(&repo);
    repo.commit("b.txt", "three", "third");
    let change = next(&rx).expect("no event after a commit");
    assert!(change.history);
}

#[test]
fn edited_file_refreshes_status_only() {
    let repo = Scratch::new("watch-edit");
    let (_w, rx) = watch(&repo);
    std::fs::write(repo.path().join("a.txt"), "edited").unwrap();
    let change = next(&rx).expect("no event after an edit");
    assert!(change.status);
    assert!(!change.history);
}

#[test]
fn ignored_files_stay_quiet() {
    let repo = Scratch::new("watch-ignored");
    repo.commit(".gitignore", "target/\n*.log\n", "ignore");
    let (_w, rx) = watch(&repo);
    std::fs::create_dir_all(repo.path().join("target/debug")).unwrap();
    for i in 0..50 {
        std::fs::write(repo.path().join(format!("target/debug/{i}.o")), "x").unwrap();
    }
    std::fs::write(repo.path().join("build.log"), "x").unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(2)).ok(), None);
}
