mod common;

use common::Scratch;
use kelp_core::history::History;
use kelp_core::view::ViewFilter;

fn with_side_branch(name: &str) -> Scratch {
    let repo = Scratch::new(name);
    repo.git(&["checkout", "-q", "-b", "feat/side"]);
    repo.commit("side.txt", "one", "side one");
    repo.commit("side.txt", "two", "side two");
    repo.git(&["checkout", "-q", "main"]);
    repo.commit("a.txt", "three", "main three");
    repo
}

fn load(repo: &Scratch, view: &ViewFilter) -> History {
    let git = gix::open(repo.path()).unwrap();
    History::load_filtered(&git, view).unwrap()
}

#[test]
fn hiding_a_branch_drops_its_commits_and_lanes() {
    let repo = with_side_branch("view-hide");
    let all = load(&repo, &ViewFilter::default());
    assert_eq!(all.len(), 5);
    assert_eq!(all.layout.lane_count(), 2);

    let mut view = ViewFilter::default();
    view.toggle("refs/heads/feat/side");
    let hidden = load(&repo, &view);
    assert_eq!(hidden.len(), 3);
    assert_eq!(hidden.layout.lane_count(), 1);
    assert_eq!(hidden.refs.hidden_count(), 1);
    assert!(
        hidden
            .refs
            .labels
            .iter()
            .all(|l| l.row.is_none() || !l.hidden)
    );
}

#[test]
fn shared_commits_stay_when_one_branch_is_hidden() {
    let repo = with_side_branch("view-shared");
    repo.git(&["branch", "copy", "feat/side"]);
    let mut view = ViewFilter::default();
    view.toggle("refs/heads/feat/side");
    assert_eq!(load(&repo, &view).len(), 5);
}

#[test]
fn solo_keeps_head_visible() {
    let repo = with_side_branch("view-solo");
    let mut view = ViewFilter::default();
    view.solo("refs/heads/feat/side");
    let history = load(&repo, &view);
    assert_eq!(history.len(), 5);
    let main = history
        .refs
        .labels
        .iter()
        .find(|l| l.name == "main")
        .unwrap();
    assert!(!main.hidden && main.is_head);

    view.solo("refs/heads/main");
    assert_eq!(load(&repo, &view).len(), 3);
}

#[test]
fn history_open_uses_the_saved_view() {
    let repo = with_side_branch("view-saved");
    let git = gix::open(repo.path()).unwrap();
    let mut view = ViewFilter::default();
    view.toggle("refs/heads/feat/side");
    view.save(git.common_dir()).unwrap();
    let (_, history) = History::open(repo.path()).unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(ViewFilter::load(git.common_dir()), view);
}
