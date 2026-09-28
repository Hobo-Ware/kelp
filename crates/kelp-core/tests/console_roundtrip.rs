mod common;

use common::Scratch;
use kelp_core::console::{self, Kind};
use kelp_core::git_cli;
use kelp_core::ops::Op;

#[test]
fn a_failing_command_is_recorded_with_its_exit_code_and_stderr() {
    let repo = Scratch::new("console-fail");
    let missing = "definitely-not-a-ref-4711";
    assert!(git_cli::run(repo.path(), &["rev-parse", "--verify", missing]).is_err());
    let entry = console::entries()
        .into_iter()
        .find(|e| e.args.iter().any(|a| a == missing))
        .expect("the failed command is in the console");
    assert_eq!(entry.program, "git");
    assert_eq!(entry.exit, Some(128));
    assert!(entry.failed());
    assert!(entry.stderr.text.contains("fatal"), "{}", entry.stderr.text);
    assert_eq!(entry.kind, Kind::Background);
    assert_eq!(entry.dir, repo.path());
}

#[test]
fn ops_are_recorded_as_actions_with_their_output() {
    let repo = Scratch::new("console-op");
    Op::CreateBranch {
        name: "console-probe".into(),
        start: "HEAD".into(),
        switch: false,
    }
    .run(repo.path())
    .unwrap();
    let entry = console::entries()
        .into_iter()
        .find(|e| e.args.iter().any(|a| a == "console-probe"))
        .expect("the op is in the console");
    assert_eq!(entry.kind, Kind::Action);
    assert_eq!(entry.exit, Some(0));
    assert!(!entry.failed());
}

#[test]
fn a_recent_failure_can_be_found_for_an_error_toast() {
    let repo = Scratch::new("console-toast");
    let _ = git_cli::run(repo.path(), &["checkout", "no-such-branch-for-toast"]);
    let id = console::latest_failure_within(std::time::Duration::from_secs(5))
        .expect("a failure was just recorded");
    let entry = console::entries().into_iter().find(|e| e.id == id).unwrap();
    assert!(entry.failed());
}
