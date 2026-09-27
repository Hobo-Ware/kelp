use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewFilter {
    pub hidden: BTreeSet<String>,
    pub solo: Option<String>,
}

impl ViewFilter {
    fn file(common_dir: &Path) -> PathBuf {
        common_dir.join("kelp").join("view.json")
    }

    pub fn load(common_dir: &Path) -> Self {
        std::fs::read(Self::file(common_dir))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, common_dir: &Path) -> std::io::Result<()> {
        let file = Self::file(common_dir);
        if !self.is_filtering() {
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

    pub fn is_filtering(&self) -> bool {
        self.solo.is_some() || !self.hidden.is_empty()
    }

    pub fn shows(&self, full_name: &str) -> bool {
        match &self.solo {
            Some(solo) => solo == full_name,
            None => !self.hidden.contains(full_name),
        }
    }

    pub fn toggle(&mut self, full_name: &str) {
        self.solo = None;
        if !self.hidden.remove(full_name) {
            self.hidden.insert(full_name.to_string());
        }
    }

    pub fn solo(&mut self, full_name: &str) {
        self.solo = Some(full_name.to_string());
    }

    pub fn show_all(&mut self) {
        self.solo = None;
        self.hidden.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hiding_toggles_and_solo_wins() {
        let mut view = ViewFilter::default();
        view.toggle("refs/heads/a");
        assert!(!view.shows("refs/heads/a"));
        assert!(view.shows("refs/heads/b"));
        view.solo("refs/heads/b");
        assert!(!view.shows("refs/heads/c"));
        assert!(view.shows("refs/heads/b"));
        view.toggle("refs/heads/a");
        assert_eq!(view, ViewFilter::default());
    }

    #[test]
    fn saves_and_clears_the_file() {
        let dir = std::env::temp_dir().join(format!("kelp-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut view = ViewFilter::default();
        view.toggle("refs/remotes/origin/x");
        view.save(&dir).unwrap();
        assert_eq!(ViewFilter::load(&dir), view);
        view.show_all();
        view.save(&dir).unwrap();
        assert!(!ViewFilter::file(&dir).exists());
        assert_eq!(ViewFilter::load(&dir), ViewFilter::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
