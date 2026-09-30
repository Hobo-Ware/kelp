mod common;

use common::Scratch;
use kelp_core::commit;

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
