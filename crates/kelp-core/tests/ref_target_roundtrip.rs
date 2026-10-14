mod common;

use common::Scratch;
use kelp_core::history::History;
use kelp_core::refs::RefKind;

#[test]
fn an_annotated_tag_label_targets_the_commit_not_the_tag_object() {
    let repo = Scratch::new("tag-target");
    repo.git(&["tag", "-a", "v1", "-m", "release"]);
    repo.git(&["branch", "side"]);
    let commit = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    let tag_object = repo.git(&["rev-parse", "v1"]).trim().to_string();
    assert_ne!(commit, tag_object);

    let (_, history) = History::open(repo.path()).unwrap();
    let target = |kind, name: &str| {
        history
            .refs
            .of_kind(kind)
            .find(|l| l.name == name)
            .unwrap()
            .target
            .to_string()
    };
    assert_eq!(target(RefKind::Tag, "v1"), commit);
    assert_eq!(target(RefKind::Local, "side"), commit);
}
