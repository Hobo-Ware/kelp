mod common;

use common::Scratch;
use kelp_core::commit;
use kelp_core::filter::{self, Filter, Period};

fn head(repo: &Scratch) -> gix::ObjectId {
    let hex = repo.git(&["rev-parse", "HEAD"]);
    gix::ObjectId::from_hex(hex.trim().as_bytes()).unwrap()
}

#[test]
fn an_amended_commit_keeps_its_author_date_and_shows_the_committer_date() {
    let repo = Scratch::new("commit-dates");
    repo.commit("a.txt", "one", "first");
    repo.git(&[
        "commit",
        "-q",
        "--amend",
        "-m",
        "amended",
        "--date=2020-01-01T00:00:00Z",
    ]);
    let id = head(&repo);
    let gix_repo = gix::open(repo.path()).unwrap();

    let details = commit::details(&gix_repo, id).unwrap();
    assert_eq!(details.time, 1_577_836_800);
    assert!(details.commit_time > details.time);
    let summary = commit::summary(&gix_repo, id).unwrap();
    assert_eq!(summary.time, details.commit_time);
}

#[test]
fn the_date_filter_reads_the_committer_date() {
    let repo = Scratch::new("filter-dates");
    repo.commit("a.txt", "one", "first");
    repo.git(&[
        "commit",
        "-q",
        "--amend",
        "-m",
        "amended",
        "--date=2020-01-01T00:00:00Z",
    ]);
    let id = head(&repo);
    let commit_time = commit::details(&gix::open(repo.path()).unwrap(), id)
        .unwrap()
        .commit_time;
    let day = Filter {
        period: Period::Day,
        ..Filter::default()
    };

    let rows = filter::matching_rows(repo.path(), &[id], &day, commit_time, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(rows, [true]);
}
