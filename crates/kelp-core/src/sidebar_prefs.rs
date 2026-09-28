use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::git_cli;
use crate::ref_tree::Sort;

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct SidebarPrefs {
    pub pinned: BTreeSet<String>,
    pub open_folders: BTreeSet<String>,
    pub hide_merged: bool,
    pub local_sort: Sort,
    pub remote_sort: Sort,
    pub tag_sort: Sort,
}

impl SidebarPrefs {
    fn file(common_dir: &Path) -> PathBuf {
        common_dir.join("kelp").join("sidebar.json")
    }

    pub fn load(common_dir: &Path) -> Self {
        std::fs::read(Self::file(common_dir))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, common_dir: &Path) -> std::io::Result<()> {
        let file = Self::file(common_dir);
        if *self == Self::default() {
            return match std::fs::remove_file(&file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(file, json)
    }

    pub fn toggle_pin(&mut self, full_name: &str) {
        if !self.pinned.remove(full_name) {
            self.pinned.insert(full_name.to_string());
        }
    }

    pub fn toggle_folder(&mut self, key: &str) {
        if !self.open_folders.remove(key) {
            self.open_folders.insert(key.to_string());
        }
    }
}

/// The ref that "merged" is measured against: the current branch's
/// upstream, else a local `main` or `master`, else HEAD.
pub fn merge_base_ref(dir: &Path) -> String {
    if let Ok(upstream) = git_cli::run(dir, &["rev-parse", "--abbrev-ref", "@{upstream}"]) {
        let upstream = upstream.trim();
        if !upstream.is_empty() {
            return upstream.to_string();
        }
    }
    ["main", "master"]
        .into_iter()
        .find(|b| {
            git_cli::run(
                dir,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/{b}"),
                ],
            )
            .is_ok()
        })
        .unwrap_or("HEAD")
        .to_string()
}

/// Local branches whose tips are already contained in `base`.
pub fn merged_into(dir: &Path, base: &str) -> anyhow::Result<HashSet<String>> {
    let out = git_cli::run(
        dir,
        &["branch", "--merged", base, "--format=%(refname:short)"],
    )?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_prefs_leave_no_file() {
        let dir = std::env::temp_dir().join(format!("kelp-sidebar-prefs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut prefs = SidebarPrefs::default();
        prefs.toggle_pin("refs/heads/main");
        prefs.save(&dir).unwrap();
        assert_eq!(SidebarPrefs::load(&dir), prefs);
        prefs.toggle_pin("refs/heads/main");
        prefs.save(&dir).unwrap();
        assert!(!dir.join("kelp").join("sidebar.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
