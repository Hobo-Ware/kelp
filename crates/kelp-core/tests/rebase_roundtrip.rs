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
    let plan = rebase::load(repo.path(), "HEAD~3").unwrap();
    assert!(plan.has_merges());
}

fn merge_repo(name: &str) -> Scratch {
    let repo = Scratch::new(name);
    repo.git(&["checkout", "-q", "-b", "side"]);
    repo.commit("s.txt", "s\n", "side work");
    repo.git(&["checkout", "-q", "main"]);
    repo.commit("m.txt", "m\n", "main work");
    repo.git(&["merge", "-q", "--no-ff", "-m", "merge side", "side"]);
    repo.commit("after.txt", "after\n", "after merge");
    repo
}

fn parents(repo: &Scratch, rev: &str) -> usize {
    repo.git(&["rev-list", "--parents", "-n", "1", rev])
        .split_whitespace()
        .count()
        - 1
}

#[test]
fn rebase_merges_keeps_the_merge() {
    let repo = merge_repo("rebase-merges");
    let mut plan = rebase::load(repo.path(), "HEAD~3").unwrap();
    assert!(plan.steps.iter().any(|s| s.merge));
    let find =
        |plan: &Plan, name: &str| plan.steps.iter().position(|s| s.summary() == name).unwrap();
    let main_work = find(&plan, "main work");
    plan.steps[main_work].action = Action::Reword;
    plan.steps[main_work].message = "main work, reworded".into();
    let after = find(&plan, "after merge");
    plan.steps[after].action = Action::Drop;
    assert_eq!(run(&repo, &plan), Outcome::Done);
    assert_eq!(repo.git(&["log", "-1", "--format=%s"]).trim(), "merge side");
    assert_eq!(parents(&repo, "HEAD"), 2, "the merge survives");
    let log = repo.git(&["log", "--format=%s"]);
    assert!(log.contains("main work, reworded"), "{log}");
    assert!(log.contains("side work"), "{log}");
    assert!(!repo.path().join("after.txt").exists());
}

#[test]
fn merge_plans_refuse_squash() {
    let repo = merge_repo("rebase-merges-squash");
    let mut plan = rebase::load(repo.path(), "HEAD~3").unwrap();
    let last = plan.steps.len() - 1;
    plan.steps[last].action = Action::Squash;
    let combined = |g: &Group| rebase::default_combined(&plan.steps, g);
    let err = rebase::run(repo.path(), &plan, &combined).unwrap_err();
    assert!(err.to_string().contains("merge commits"), "{err}");
}

#[test]
fn edit_stops_then_amend_and_continue() {
    let repo = repo("rebase-edit");
    let mut plan = plan(&repo);
    plan.steps[1].action = Action::Edit;
    let stopped = run(&repo, &plan);
    let Outcome::Editing(sha) = stopped else {
        panic!("expected an edit stop, got {stopped:?}");
    };
    assert_eq!(sha, plan.steps[1].id.to_hex_with_len(7).to_string());
    let git_dir = repo.path().join(".git");
    let progress = kelp_core::conflict::in_progress(&git_dir).unwrap();
    assert_eq!(progress.editing.as_deref(), Some(sha.as_str()));
    assert!(progress.title().contains("for editing"));
    std::fs::write(repo.path().join("c.txt"), "c amended\n").unwrap();
    repo.git(&["add", "c.txt"]);
    repo.git(&["commit", "-q", "--amend", "-m", "add c, amended"]);
    kelp_core::conflict::run_step(
        repo.path(),
        kelp_core::conflict::Operation::Rebase,
        kelp_core::conflict::Step::Continue,
    )
    .unwrap();
    assert_eq!(
        subjects(&repo),
        ["add d", "add c, amended", "add b", "second", "first"]
    );
    assert!(kelp_core::conflict::in_progress(&git_dir).is_none());
}

#[test]
fn edit_message_of_head_keeps_the_index() {
    let repo = repo("edit-head");
    let tree = repo.git(&["rev-parse", "HEAD^{tree}"]);
    std::fs::write(repo.path().join("b.txt"), "staged\n").unwrap();
    repo.git(&["add", "b.txt"]);
    let outcome =
        rebase::edit_message(repo.path(), "HEAD", "add d, better\n\nWith a body.").unwrap();
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(
        repo.git(&["log", "-1", "--format=%B"]).trim_end(),
        "add d, better\n\nWith a body."
    );
    assert_eq!(
        repo.git(&["rev-parse", "HEAD^{tree}"]),
        tree,
        "the commit's files are untouched"
    );
    assert_eq!(
        repo.git(&["diff", "--cached", "--name-only"]).trim(),
        "b.txt"
    );
}

#[test]
fn edit_message_three_back_rewords_through_a_rebase() {
    let repo = repo("edit-older");
    let outcome = rebase::edit_message(repo.path(), "HEAD~2", "add b, renamed").unwrap();
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(
        subjects(&repo),
        ["add d", "add c", "add b, renamed", "second", "first"]
    );
    assert!(repo.path().join("d.txt").exists());
}

#[test]
fn edit_message_refuses_empty_and_off_branch() {
    let repo = repo("edit-refuse");
    assert!(rebase::edit_message(repo.path(), "HEAD", "  ").is_err());
    repo.git(&["checkout", "-q", "-b", "side", "HEAD~1"]);
    repo.commit("e.txt", "e\n", "add e");
    let side = repo.git(&["rev-parse", "HEAD"]);
    repo.git(&["checkout", "-q", "main"]);
    let err = rebase::edit_message(repo.path(), side.trim(), "x").unwrap_err();
    assert!(
        err.to_string().contains("not on the current branch"),
        "{err}"
    );
}
