use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MAX_RECENTS: usize = 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recent {
    pub path: PathBuf,
    pub opened: i64,
}

pub fn remember(list: &mut Vec<Recent>, path: &Path, now: i64) {
    let key = canonical(path);
    list.retain(|r| canonical(&r.path) != key);
    list.insert(
        0,
        Recent {
            path: key,
            opened: now,
        },
    );
    list.truncate(MAX_RECENTS);
}

pub fn forget(list: &mut Vec<Recent>, path: &Path) {
    let key = canonical(path);
    list.retain(|r| canonical(&r.path) != key);
}

pub fn from_env() -> Option<Vec<Recent>> {
    let paths = std::env::var("KELP_RECENTS").ok()?;
    let now = now();
    Some(
        paths
            .split(',')
            .filter(|p| !p.trim().is_empty())
            .enumerate()
            .map(|(i, p)| Recent {
                path: PathBuf::from(p.trim()),
                opened: now - (i as i64 + 1) * 5400,
            })
            .collect(),
    )
}

pub fn branch_of(repo: &Path) -> Option<String> {
    let dot_git = repo.join(".git");
    let git_dir = if dot_git.is_file() {
        let pointer = std::fs::read_to_string(&dot_git).ok()?;
        let dir = PathBuf::from(pointer.strip_prefix("gitdir:")?.trim());
        if dir.is_absolute() {
            dir
        } else {
            repo.join(dir)
        }
    } else if dot_git.is_dir() {
        dot_git
    } else {
        repo.to_path_buf()
    };
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    match head.trim().strip_prefix("ref: ") {
        Some(reference) => Some(
            reference
                .strip_prefix("refs/heads/")
                .unwrap_or(reference)
                .to_string(),
        ),
        None => Some(format!(
            "detached at {}",
            &head.trim()[..7.min(head.trim().len())]
        )),
    }
}

pub fn tilde(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.as_deref().and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[Recent]) -> Vec<&str> {
        list.iter().map(|r| r.path.to_str().unwrap()).collect()
    }

    #[test]
    fn newest_first_without_duplicates() {
        let mut list = Vec::new();
        remember(&mut list, Path::new("/nope/a"), 1);
        remember(&mut list, Path::new("/nope/b"), 2);
        remember(&mut list, Path::new("/nope/a"), 3);
        assert_eq!(paths(&list), ["/nope/a", "/nope/b"]);
        assert_eq!(list[0].opened, 3);
    }

    #[test]
    fn the_same_folder_by_another_path_counts_once() {
        let dir = std::env::temp_dir().join(format!("kelp-recents-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let mut list = Vec::new();
        remember(&mut list, &dir, 1);
        remember(&mut list, &dir.join("sub/.."), 2);
        assert_eq!(list.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_list_is_capped() {
        let mut list = Vec::new();
        for i in 0..30 {
            remember(&mut list, Path::new(&format!("/nope/{i}")), i);
        }
        assert_eq!(list.len(), MAX_RECENTS);
        assert_eq!(list[0].path, Path::new("/nope/29"));
        assert_eq!(list.last().unwrap().path, Path::new("/nope/10"));
    }

    #[test]
    fn forgetting_removes_only_that_repo() {
        let mut list = Vec::new();
        remember(&mut list, Path::new("/nope/a"), 1);
        remember(&mut list, Path::new("/nope/b"), 2);
        forget(&mut list, Path::new("/nope/a"));
        assert_eq!(paths(&list), ["/nope/b"]);
    }

    #[test]
    fn branch_comes_from_head_and_worktree_pointers() {
        let root = std::env::temp_dir().join(format!("kelp-branch-{}", std::process::id()));
        let main = root.join("main");
        let linked = root.join("linked");
        std::fs::create_dir_all(main.join(".git/worktrees/linked")).unwrap();
        std::fs::create_dir_all(&linked).unwrap();
        std::fs::write(main.join(".git/HEAD"), "ref: refs/heads/feat/x\n").unwrap();
        std::fs::write(
            main.join(".git/worktrees/linked/HEAD"),
            "0123456789abcdef0123456789abcdef01234567\n",
        )
        .unwrap();
        std::fs::write(
            linked.join(".git"),
            format!("gitdir: {}\n", main.join(".git/worktrees/linked").display()),
        )
        .unwrap();
        assert_eq!(branch_of(&main).as_deref(), Some("feat/x"));
        assert_eq!(branch_of(&linked).as_deref(), Some("detached at 0123456"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
