#![allow(dead_code)]

use std::path::{Path, PathBuf};

use kelp_core::git_cli::run;
use kelp_core::history::History;
use kelp_core::refs::RefKind;

pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("kelp-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = Scratch(dir);
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "test@example.com"]);
        repo.git(&["config", "user.name", "Test"]);
        repo.commit("a.txt", "one", "first");
        repo.commit("a.txt", "two", "second");
        repo
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn git(&self, args: &[&str]) -> String {
        run(&self.0, args).unwrap()
    }

    pub fn commit(&self, file: &str, content: &str, message: &str) {
        std::fs::write(self.0.join(file), content).unwrap();
        self.git(&["add", "."]);
        self.git(&["commit", "-q", "-m", message]);
    }

    pub fn branches(&self) -> Vec<String> {
        let (_, history) = History::open(&self.0).unwrap();
        history
            .refs
            .of_kind(RefKind::Local)
            .map(|l| l.name.clone())
            .collect()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
        let _ = std::fs::remove_dir_all(self.0.with_file_name(format!(
            "{}-wt",
            self.0.file_name().unwrap().to_string_lossy()
        )));
    }
}
