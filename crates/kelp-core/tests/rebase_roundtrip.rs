mod common;

use common::Scratch;
use kelp_core::rebase::{self, Action, Group, Outcome, Plan};

fn repo(name: &str) -> Scratch {
    let repo = Scratch::new(name);
    repo.commit("b.txt", "b\n", "add b");
    repo.commit("c.txt", "c\n", "add c");
    repo.commit("d.txt", "d\n", "add d");
    repo
}

fn plan(repo: &Scratch) -> Plan {
    rebase::load(repo.path(), "HEAD~3").unwrap()
}

fn run(repo: &Scratch, plan: &Plan) -> Outcome {
    let combined = |g: &Group| rebase::default_combined(&plan.steps, g);
    rebase::run(repo.path(), plan, &combined).unwrap()
}

fn subjects(repo: &Scratch) -> Vec<String> {
    repo.git(&["log", "--format=%s", "-5"])
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn loads_oldest_first() {
    let repo = repo("rebase-load");
    let plan = plan(&repo);
    let names: Vec<&str> = plan.steps.iter().map(|s| s.summary()).collect();
    assert_eq!(names, ["add b", "add c", "add d"]);
}

#[test]
fn reorders_and_drops() {
    let repo = repo("rebase-reorder");
    let mut plan = plan(&repo);
    plan.steps.swap(0, 2);
    plan.steps[1].action = Action::Drop;
    assert_eq!(run(&repo, &plan), Outcome::Done);
    assert_eq!(subjects(&repo), ["add b", "add d", "second", "first"]);
    assert!(!repo.path().join("c.txt").exists());
}

#[test]
fn rewords_with_awkward_text() {
    let repo = repo("rebase-reword");
    let mut plan = plan(&repo);
    let message =
        "Fix \"quotes\", 'single' and `ticks` $HOME\n\nBody with ünïcødé 🚀\nand a second line";
    plan.steps[1].action = Action::Reword;
    plan.steps[1].message = message.into();
    assert_eq!(run(&repo, &plan), Outcome::Done);
    let body = repo.git(&["log", "-1", "--format=%B", "HEAD~1"]);
    assert_eq!(body.trim_end(), message);
    assert_eq!(subjects(&repo)[0], "add d");
}

#[test]
fn squash_combines_messages_and_changes() {
    let repo = repo("rebase-squash");
    let mut plan = plan(&repo);
    plan.steps[1].action = Action::Squash;
    assert_eq!(run(&repo, &plan), Outcome::Done);
    assert_eq!(subjects(&repo), ["add d", "add b", "second", "first"]);
    let body = repo.git(&["log", "-1", "--format=%B", "HEAD~1"]);
    assert_eq!(body.trim_end(), "add b\n\nadd c");
    let files = repo.git(&["show", "--name-only", "--format=", "HEAD~1"]);
    assert_eq!(files.lines().collect::<Vec<_>>(), ["b.txt", "c.txt"]);
}

#[test]
fn fixup_keeps_the_first_message() {
    let repo = repo("rebase-fixup");
    let mut plan = plan(&repo);
    plan.steps[2].action = Action::Fixup;
    assert_eq!(run(&repo, &plan), Outcome::Done);
    assert_eq!(subjects(&repo), ["add c", "add b", "second", "first"]);
    let body = repo.git(&["log", "-1", "--format=%B"]);
    assert_eq!(body.trim_end(), "add c");
}

#[test]
fn edited_combined_message_wins() {
    let repo = repo("rebase-squash-edit");
    let mut plan = plan(&repo);
    plan.steps[2].action = Action::Squash;
    let combined = |_: &Group| "c and d together".to_string();
    assert_eq!(
        rebase::run(repo.path(), &plan, &combined).unwrap(),
        Outcome::Done
    );
    assert_eq!(subjects(&repo)[0], "c and d together");
}

#[test]
fn refuses_a_dirty_tree() {
    let repo = repo("rebase-dirty");
    std::fs::write(repo.path().join("b.txt"), "changed\n").unwrap();
    let plan = plan(&repo);
    let combined = |g: &Group| rebase::default_combined(&plan.steps, g);
    let err = rebase::run(repo.path(), &plan, &combined).unwrap_err();
    assert!(err.to_string().contains("uncommitted changes"), "{err}");
    assert_eq!(subjects(&repo)[0], "add d");
}

#[test]
fn conflicts_stop_with_the_rebase_in_progress() {
    let repo = Scratch::new("rebase-conflict");
    repo.commit("a.txt", "three", "third");
    repo.commit("a.txt", "four", "fourth");
    let mut plan = rebase::load(repo.path(), "HEAD~2").unwrap();
    plan.steps.swap(0, 1);
    assert_eq!(run(&repo, &plan), Outcome::Conflicts);
    let status = repo.git(&["status"]);
    assert!(status.contains("rebase in progress"), "{status}");
    repo.git(&["rebase", "--abort"]);
}

#[test]
fn refuses_non_ancestors_and_merges() {
    let repo = repo("rebase-refuse");
    let head = repo.git(&["rev-parse", "HEAD"]);
    let err = rebase::load(repo.path(), head.trim()).unwrap_err();
    assert!(err.to_string().contains("latest commit"), "{err}");
    repo.git(&["checkout", "-q", "-b", "side", "HEAD~1"]);
    repo.commit("e.txt", "e\n", "add e");
    let side = repo.git(&["rev-parse", "HEAD"]);
    repo.git(&["checkout", "-q", "main"]);
    let err = rebase::load(repo.path(), side.trim()).unwrap_err();
    assert!(
        err.to_string().contains("not part of the current branch"),
        "{err}"
    );
    repo.git(&["merge", "-q", "--no-ff", "-m", "merge side", "side"]);
    let err = rebase::load(repo.path(), "HEAD~3").unwrap_err();
    assert!(err.to_string().contains("merge commit"), "{err}");
}
