mod common;

use common::Scratch;
use kelp_core::sidebar_prefs::{SidebarPrefs, merge_base_ref, merged_into};

#[test]
fn merged_branches_are_found_against_main() {
    let repo = Scratch::new("sidebar-merged");
    repo.git(&["branch", "done/old"]);
    repo.git(&["switch", "-q", "-c", "feat/open"]);
    repo.commit("b.txt", "new", "open work");
    repo.git(&["switch", "-q", "main"]);
    let base = merge_base_ref(repo.path());
    assert_eq!(base, "main");
    let merged = merged_into(repo.path(), &base).unwrap();
    assert!(merged.contains("done/old"));
    assert!(merged.contains("main"));
    assert!(!merged.contains("feat/open"));
}

#[test]
fn prefs_survive_a_reload() {
    let repo = Scratch::new("sidebar-prefs");
    let common = repo.path().join(".git");
    let mut prefs = SidebarPrefs::default();
    prefs.toggle_pin("refs/heads/feat/a");
    prefs.toggle_folder("local:feat");
    prefs.hide_merged = true;
    prefs.save(&common).unwrap();
    assert_eq!(SidebarPrefs::load(&common), prefs);
}
