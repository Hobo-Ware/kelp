mod common;

use std::path::Path;
use std::process::Command;

use common::Scratch;
use kelp_core::diff::{self, Body};
use kelp_core::ops::Op;
use kelp_core::signing::{self, Format, Scope, Signature};
use kelp_core::submodules::{self, State};

fn add_submodule(outer: &Scratch, lib: &Scratch, at: &str) {
    outer.git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        "-q",
        &lib.path().to_string_lossy(),
        at,
    ]);
    outer.git(&["commit", "-q", "-m", "add lib"]);
    for (key, value) in [("user.email", "test@example.com"), ("user.name", "Test")] {
        kelp_core::git_cli::run(&outer.path().join(at), &["config", key, value]).unwrap();
    }
}

#[test]
fn submodules_are_listed_and_their_changes_diffed() {
    let lib = Scratch::new("modules-lib");
    let outer = Scratch::new("modules-outer");
    add_submodule(&outer, &lib, "libs/lib");

    let listed = submodules::list(outer.path()).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].path, "libs/lib");
    assert_eq!(listed[0].state, State::Clean);
    let recorded = listed[0].recorded.unwrap();

    let inner = outer.path().join("libs/lib");
    std::fs::write(inner.join("b.txt"), "new").unwrap();
    kelp_core::git_cli::run(&inner, &["add", "."]).unwrap();
    kelp_core::git_cli::run(&inner, &["commit", "-q", "-m", "teach the lib a trick"]).unwrap();

    let listed = submodules::list(outer.path()).unwrap();
    assert_eq!(listed[0].state, State::NewCommits);
    assert_eq!(listed[0].recorded, Some(recorded));
    assert_ne!(listed[0].checked_out, Some(recorded));

    let repo = gix::open(outer.path()).unwrap();
    let working = diff::unstaged_file(&repo, outer.path(), "libs/lib").unwrap();
    let Body::Submodule(change) = &working.body else {
        panic!("expected a submodule diff, got {:?}", working.body);
    };
    assert_eq!(change.old, Some(recorded));
    assert_eq!(change.new_title.as_deref(), Some("teach the lib a trick"));
    assert_eq!(change.old_title.as_deref(), Some("second"));

    outer.git(&["add", "libs/lib"]);
    let staged = diff::staged_file(&repo, "libs/lib").unwrap();
    assert!(matches!(staged.body, Body::Submodule(_)));
    outer.git(&["commit", "-q", "-m", "bump lib"]);
    let head = repo.head_id().unwrap().detach();
    let committed = diff::commit_file(&gix::open(outer.path()).unwrap(), head, "libs/lib").unwrap();
    let Body::Submodule(change) = committed.body else {
        panic!("expected a submodule diff");
    };
    assert_eq!(change.old, Some(recorded));
    assert_eq!(change.new_title.as_deref(), Some("teach the lib a trick"));
}

#[test]
fn uninitialized_submodules_can_be_initialized() {
    let lib = Scratch::new("modules-init-lib");
    let outer = Scratch::new("modules-init-outer");
    add_submodule(&outer, &lib, "vendor/lib");
    let copy = outer.path().with_file_name(format!(
        "{}-copy",
        outer.path().file_name().unwrap().to_string_lossy()
    ));
    let _ = std::fs::remove_dir_all(&copy);
    kelp_core::git_cli::run(
        outer.path().parent().unwrap(),
        &[
            "clone",
            "-q",
            &outer.path().to_string_lossy(),
            &copy.to_string_lossy(),
        ],
    )
    .unwrap();
    assert_eq!(
        submodules::list(&copy).unwrap()[0].state,
        State::NotInitialized
    );
    let init = Op::SubmoduleUpdate {
        path: None,
        init: true,
    };
    let ran = Command::new("git")
        .current_dir(&copy)
        .args(init.args())
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "protocol.file.allow")
        .env("GIT_CONFIG_VALUE_0", "always")
        .output()
        .unwrap();
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert_eq!(submodules::list(&copy).unwrap()[0].state, State::Clean);
    let _ = std::fs::remove_dir_all(&copy);
}

#[test]
fn ssh_signing_round_trip() {
    if Command::new("ssh-keygen").arg("-?").output().is_err() {
        return;
    }
    let repo = Scratch::new("signing-ssh");
    let key = repo.path().join("signing_key");
    let made = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", "kelp-test", "-f"])
        .arg(&key)
        .status()
        .unwrap();
    assert!(made.success());
    let public = key.with_extension("pub");
    signing::apply(
        repo.path(),
        Scope::Repo,
        &signing::Config {
            sign: true,
            format: Format::Ssh,
            key: public.to_string_lossy().into_owned(),
        },
    )
    .unwrap();
    let config = signing::current(repo.path());
    assert!(config.sign);
    assert_eq!(config.format, Format::Ssh);

    let allowed = repo.path().join("allowed_signers");
    let public_key = std::fs::read_to_string(&public).unwrap();
    std::fs::write(&allowed, format!("test@example.com {public_key}")).unwrap();
    repo.git(&[
        "config",
        "gpg.ssh.allowedSignersFile",
        &allowed.to_string_lossy(),
    ]);

    std::fs::write(repo.path().join("signed.txt"), "yes").unwrap();
    repo.git(&["add", "signed.txt"]);
    Op::Commit {
        message: "signed commit".into(),
        amend: false,
    }
    .run(repo.path())
    .unwrap();
    let signature = signing::read(repo.path(), "HEAD").unwrap();
    assert!(
        matches!(signature, Signature::Verified { .. }),
        "{signature:?}"
    );
    assert_eq!(
        signing::read(repo.path(), "HEAD~1").unwrap(),
        Signature::Unsigned
    );

    signing::apply(
        repo.path(),
        Scope::Repo,
        &signing::Config {
            sign: false,
            format: Format::Ssh,
            key: String::new(),
        },
    )
    .unwrap();
    assert!(!signing::current(repo.path()).sign);
    assert_eq!(signing::current(repo.path()).key, "");
}

fn lfs_pointer(content: &[u8]) -> (String, String) {
    use sha2::{Digest, Sha256};
    let oid: String = Sha256::digest(content)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let pointer = format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{oid}\nsize {}\n",
        content.len()
    );
    (oid, pointer)
}

#[test]
fn lfs_pointers_show_their_content_when_the_object_is_local() {
    let repo = Scratch::new("lfs-local");
    let content = b"\x89PNG not really but binary\x00\x01";
    let (oid, pointer) = lfs_pointer(content);
    repo.commit("art.png", &pointer, "add art");
    let git = gix::open(repo.path()).unwrap();
    let head = git.head_id().unwrap().detach();

    let without = diff::commit_file(&git, head, "art.png").unwrap();
    assert_eq!(
        without.lfs.new.as_ref().map(|p| p.oid.as_str()),
        Some(oid.as_str())
    );
    assert!(matches!(without.body, Body::Text(_)));

    let object = kelp_core::lfs::object_path(Path::new(git.common_dir()), &oid);
    std::fs::create_dir_all(object.parent().unwrap()).unwrap();
    std::fs::write(&object, content).unwrap();
    let with = diff::commit_file(&git, head, "art.png").unwrap();
    assert!(matches!(with.body, Body::Binary));
    assert_eq!(
        with.preview.and_then(|p| p.new).as_deref(),
        Some(&content[..])
    );
    assert_eq!(with.lfs.new.map(|p| p.size), Some(content.len() as u64));
}
