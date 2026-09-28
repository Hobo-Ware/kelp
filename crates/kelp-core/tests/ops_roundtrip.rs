mod common;

use common::Scratch;
use kelp_core::git_cli::run;
use kelp_core::history::History;
use kelp_core::ops::{Op, ResetMode, push_rejected};
use kelp_core::{search, workspace};

#[test]
fn checkout_and_merge_lands_on_the_target_branch() {
    let repo = Scratch::new("checkout-merge");
    repo.git(&["branch", "release"]);
    Op::CreateBranch {
        name: "feat/x".into(),
        start: "main".into(),
        switch: true,
    }
    .run(repo.path())
    .unwrap();
    repo.commit("x.txt", "x", "feature work");
    let op = Op::CheckoutAndMerge {
        branch: "release".into(),
        source: "feat/x".into(),
    };
    assert_eq!(op.command_line(), "git switch release && git merge feat/x");
    op.run(repo.path()).unwrap();
    assert_eq!(repo.git(&["branch", "--show-current"]).trim(), "release");
    assert!(repo.path().join("x.txt").exists());
}

#[test]
fn branch_lifecycle() {
    let repo = Scratch::new("branches");
    Op::CreateBranch {
        name: "feat/x".into(),
        start: "main".into(),
        switch: true,
    }
    .run(repo.path())
    .unwrap();
    repo.commit("b.txt", "b", "on feature");
    assert_eq!(repo.git(&["branch", "--show-current"]).trim(), "feat/x");

    Op::Switch("main".into()).run(repo.path()).unwrap();
    Op::Merge("feat/x".into()).run(repo.path()).unwrap();
    assert!(repo.path().join("b.txt").exists());

    Op::RenameBranch {
        from: "feat/x".into(),
        to: "feat/y".into(),
    }
    .run(repo.path())
    .unwrap();
    assert!(repo.branches().contains(&"feat/y".to_string()));

    Op::DeleteBranch {
        name: "feat/y".into(),
        force: false,
    }
    .run(repo.path())
    .unwrap();
    assert_eq!(repo.branches(), ["main"]);
}

#[test]
fn unmerged_branch_needs_force_to_delete() {
    let repo = Scratch::new("force");
    Op::CreateBranch {
        name: "wip".into(),
        start: "main".into(),
        switch: true,
    }
    .run(repo.path())
    .unwrap();
    repo.commit("c.txt", "c", "unmerged");
    Op::Switch("main".into()).run(repo.path()).unwrap();
    assert!(
        Op::DeleteBranch {
            name: "wip".into(),
            force: false
        }
        .run(repo.path())
        .is_err()
    );
    Op::DeleteBranch {
        name: "wip".into(),
        force: true,
    }
    .run(repo.path())
    .unwrap();
}

#[test]
fn stash_push_and_pop() {
    let repo = Scratch::new("stash");
    std::fs::write(repo.path().join("a.txt"), "dirty").unwrap();
    std::fs::write(repo.path().join("new.txt"), "untracked").unwrap();
    Op::StashPush.run(repo.path()).unwrap();
    assert_eq!(workspace::stashes(repo.path()).unwrap().len(), 1);
    assert!(!repo.path().join("new.txt").exists());
    Op::StashPop.run(repo.path()).unwrap();
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "dirty"
    );
    assert!(workspace::stashes(repo.path()).unwrap().is_empty());
}

#[test]
fn worktree_add_list_remove_prune() {
    let repo = Scratch::new("worktrees");
    let folder = format!(
        "../{}-wt",
        repo.path().file_name().unwrap().to_string_lossy()
    );
    Op::WorktreeAdd {
        path: folder.clone(),
        new_branch: Some("feat/wt".into()),
        start: "main".into(),
    }
    .run(repo.path())
    .unwrap();
    let trees = workspace::worktrees(repo.path()).unwrap();
    assert_eq!(trees.len(), 2);
    assert_eq!(trees[1].branch.as_deref(), Some("feat/wt"));
    assert_eq!(workspace::change_count(&trees[1].path), Some(0));

    let (_, from_worktree) = History::open(&trees[1].path).unwrap();
    assert_eq!(from_worktree.refs.head_branch.as_deref(), Some("feat/wt"));

    Op::WorktreeRemove {
        path: trees[1].path.display().to_string(),
        force: false,
    }
    .run(repo.path())
    .unwrap();
    Op::WorktreePrune.run(repo.path()).unwrap();
    assert_eq!(workspace::worktrees(repo.path()).unwrap().len(), 1);
}

#[test]
fn ahead_behind_against_upstream() {
    let origin = Scratch::new("origin");
    let clone_dir = origin.path().with_file_name(format!(
        "{}-clone",
        origin.path().file_name().unwrap().to_string_lossy()
    ));
    let _ = std::fs::remove_dir_all(&clone_dir);
    run(
        origin.path().parent().unwrap(),
        &[
            "clone",
            "-q",
            origin.path().to_str().unwrap(),
            clone_dir.to_str().unwrap(),
        ],
    )
    .unwrap();
    let clone = Scratch(clone_dir);
    clone.git(&["config", "user.email", "test@example.com"]);
    clone.git(&["config", "user.name", "Test"]);
    clone.commit("d.txt", "d", "local only");
    origin.commit("e.txt", "e", "remote only");
    Op::Fetch.run(clone.path()).unwrap();
    assert_eq!(workspace::ahead_behind(clone.path(), "main"), Some((1, 1)));
}

#[test]
fn search_matches_message_author_and_hash_prefix() {
    let repo = Scratch::new("search");
    repo.commit("f.txt", "f", "feat(graph): draw merge curves");
    let (_, history) = History::open(repo.path()).unwrap();
    let rows = |q: &str| search::matching_rows(repo.path(), history.ids(), q).unwrap();
    assert_eq!(rows("MERGE curves"), [0]);
    assert_eq!(rows("test@example.com").len(), 3);
    let prefix = history.id(2).to_hex().to_string()[..8].to_string();
    assert_eq!(rows(&prefix), [2]);
    assert!(rows("nothing like this").is_empty());
}

fn head(repo: &Scratch) -> String {
    repo.git(&["rev-parse", "HEAD"]).trim().to_string()
}

#[test]
fn cherry_pick_and_revert() {
    let repo = Scratch::new("pick");
    Op::CreateBranch {
        name: "side".into(),
        start: "main".into(),
        switch: true,
    }
    .run(repo.path())
    .unwrap();
    repo.commit("side.txt", "side", "on side");
    let picked = head(&repo);
    Op::Switch("main".into()).run(repo.path()).unwrap();
    Op::CherryPick {
        commit: picked,
        merge: false,
    }
    .run(repo.path())
    .unwrap();
    assert!(repo.path().join("side.txt").exists());

    Op::Revert {
        commit: head(&repo),
        merge: false,
    }
    .run(repo.path())
    .unwrap();
    assert!(!repo.path().join("side.txt").exists());
    let subject = repo.git(&["log", "-1", "--format=%s"]);
    assert!(subject.starts_with("Revert \"on side\""), "{subject}");
}

#[test]
fn reset_modes_keep_or_drop_changes() {
    let repo = Scratch::new("reset");
    let target = head(&repo);
    repo.commit("a.txt", "three", "third");
    let status = |repo: &Scratch| repo.git(&["status", "--porcelain"]).trim_end().to_string();
    let reset = |mode| {
        Op::Reset {
            commit: target.clone(),
            mode,
        }
        .run(repo.path())
        .unwrap()
    };

    reset(ResetMode::Soft);
    assert_eq!(head(&repo), target);
    assert_eq!(status(&repo), "M  a.txt");

    reset(ResetMode::Mixed);
    assert_eq!(status(&repo), " M a.txt");

    reset(ResetMode::Hard);
    assert_eq!(status(&repo), "");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "two"
    );
}

#[test]
fn push_sets_upstream_then_forces_with_lease_after_a_rewrite() {
    let repo = Scratch::new("push");
    let bare_dir = repo.path().with_file_name(format!(
        "{}-remote.git",
        repo.path().file_name().unwrap().to_string_lossy()
    ));
    let _ = std::fs::remove_dir_all(&bare_dir);
    run(
        repo.path().parent().unwrap(),
        &["init", "-q", "--bare", bare_dir.to_str().unwrap()],
    )
    .unwrap();
    let _bare = Scratch(bare_dir.clone());
    repo.git(&["remote", "add", "origin", bare_dir.to_str().unwrap()]);
    let push = |set_upstream, force_with_lease| Op::Push {
        branch: "main".into(),
        remote: "origin".into(),
        set_upstream,
        force_with_lease,
    };

    push(true, false).run(repo.path()).unwrap();
    let opened = gix::open(repo.path()).unwrap();
    assert_eq!(
        workspace::upstream_remote(&opened, "main").as_deref(),
        Some("origin")
    );

    repo.git(&["commit", "-q", "--amend", "-m", "second, reworded"]);
    let rejected = push(false, false).run(repo.path()).unwrap_err();
    assert!(push_rejected(&format!("{rejected:#}")), "{rejected:#}");

    push(false, true).run(repo.path()).unwrap();
    assert_eq!(workspace::ahead_behind(repo.path(), "main"), Some((0, 0)));
}

fn with_bare_remote(repo: &Scratch, name: &str) -> Scratch {
    let bare_dir = repo.path().with_file_name(format!(
        "{}-{name}.git",
        repo.path().file_name().unwrap().to_string_lossy()
    ));
    let _ = std::fs::remove_dir_all(&bare_dir);
    run(
        repo.path().parent().unwrap(),
        &["init", "-q", "--bare", bare_dir.to_str().unwrap()],
    )
    .unwrap();
    repo.git(&["remote", "add", name, bare_dir.to_str().unwrap()]);
    Scratch(bare_dir)
}

#[test]
fn renaming_a_remote_branch_moves_it_and_the_upstream() {
    let repo = Scratch::new("rename-remote-branch");
    let bare = with_bare_remote(&repo, "origin");
    repo.git(&["switch", "-q", "-c", "feat/old"]);
    repo.git(&["push", "-q", "-u", "origin", "feat/old"]);
    let op = Op::RenameRemoteBranch {
        remote: "origin".into(),
        from: "feat/old".into(),
        to: "feat/new".into(),
        tracking: Some("feat/old".into()),
    };
    assert_eq!(
        op.command_line(),
        "git push origin refs/remotes/origin/feat/old:refs/heads/feat/new \
         && git push origin --delete feat/old \
         && git branch --set-upstream-to=origin/feat/new feat/old"
    );
    op.run(repo.path()).unwrap();
    let remote_heads = run(bare.path(), &["branch", "--format=%(refname:short)"]).unwrap();
    assert!(remote_heads.lines().any(|l| l == "feat/new"));
    assert!(!remote_heads.lines().any(|l| l == "feat/old"));
    assert_eq!(
        repo.git(&["rev-parse", "--abbrev-ref", "feat/old@{upstream}"])
            .trim(),
        "origin/feat/new"
    );
}

#[test]
fn renaming_a_stash_keeps_its_changes() {
    let repo = Scratch::new("stash-rename");
    std::fs::write(repo.path().join("a.txt"), "draft").unwrap();
    Op::StashPush.run(repo.path()).unwrap();
    let sha = repo.git(&["rev-parse", "stash@{0}"]).trim().to_string();
    Op::StashRename {
        stash: "stash@{0}".into(),
        message: "half-done export".into(),
    }
    .run(repo.path())
    .unwrap();
    let list = repo.git(&["stash", "list", "--format=%H %gs"]);
    assert_eq!(list.trim(), format!("{sha} half-done export"));
}

#[test]
fn moving_a_worktree() {
    let repo = Scratch::new("worktree-move");
    let name = repo
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let from = repo.path().with_file_name(format!("{name}-wt"));
    let to = repo.path().with_file_name(format!("{name}-moved"));
    let _ = std::fs::remove_dir_all(&to);
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "side",
        from.to_str().unwrap(),
    ]);
    Op::WorktreeMove {
        from: from.to_string_lossy().into(),
        to: to.to_string_lossy().into(),
    }
    .run(repo.path())
    .unwrap();
    assert!(to.join("a.txt").exists());
    assert!(!from.exists());
    let _ = std::fs::remove_dir_all(&to);
}

#[test]
fn tags_are_created_pushed_and_deleted() {
    let repo = Scratch::new("tags");
    let bare = with_bare_remote(&repo, "origin");
    Op::CreateTag {
        name: "v1".into(),
        commit: "HEAD".into(),
        message: None,
    }
    .run(repo.path())
    .unwrap();
    Op::CreateTag {
        name: "v2".into(),
        commit: "HEAD~1".into(),
        message: Some("Second \"release\"".into()),
    }
    .run(repo.path())
    .unwrap();
    assert_eq!(repo.git(&["cat-file", "-t", "v2"]).trim(), "tag");
    assert_eq!(
        repo.git(&["tag", "-l", "--format=%(contents:subject)", "v2"])
            .trim(),
        "Second \"release\""
    );
    Op::PushTag {
        remote: "origin".into(),
        name: "v1".into(),
    }
    .run(repo.path())
    .unwrap();
    assert_eq!(run(bare.path(), &["tag"]).unwrap().trim(), "v1");
    Op::PushTags("origin".into()).run(repo.path()).unwrap();
    assert_eq!(run(bare.path(), &["tag"]).unwrap().lines().count(), 2);
    Op::DeleteRemoteTag {
        remote: "origin".into(),
        name: "v1".into(),
    }
    .run(repo.path())
    .unwrap();
    assert_eq!(run(bare.path(), &["tag"]).unwrap().trim(), "v2");
    Op::DeleteTag("v2".into()).run(repo.path()).unwrap();
    assert_eq!(repo.git(&["tag"]).trim(), "v1");
}

#[test]
fn remotes_are_added_renamed_repointed_fetched_and_removed() {
    let repo = Scratch::new("remotes");
    let bare = with_bare_remote(&repo, "scratch");
    repo.git(&["push", "-q", "scratch", "main"]);
    repo.git(&["remote", "remove", "scratch"]);
    let url = bare.path().to_string_lossy().to_string();
    Op::AddRemote {
        name: "upstream".into(),
        url: url.clone(),
    }
    .run(repo.path())
    .unwrap();
    Op::FetchRemote("upstream".into()).run(repo.path()).unwrap();
    assert!(repo.git(&["branch", "-r"]).contains("upstream/main"));
    Op::RenameRemote {
        from: "upstream".into(),
        to: "mirror".into(),
    }
    .run(repo.path())
    .unwrap();
    assert!(repo.git(&["branch", "-r"]).contains("mirror/main"));
    Op::SetRemoteUrl {
        name: "mirror".into(),
        url: format!("{url}/"),
    }
    .run(repo.path())
    .unwrap();
    assert_eq!(
        repo.git(&["remote", "get-url", "mirror"]).trim(),
        format!("{url}/")
    );
    Op::PruneRemote("mirror".into()).run(repo.path()).unwrap();
    Op::RemoveRemote("mirror".into()).run(repo.path()).unwrap();
    assert!(repo.git(&["remote"]).trim().is_empty());
}
