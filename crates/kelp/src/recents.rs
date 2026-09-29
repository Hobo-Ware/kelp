use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MAX_RECENTS: usize = 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recent {
    pub path: PathBuf,
    pub opened: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tip: Option<String>,
}

pub fn remember(list: &mut Vec<Recent>, path: &Path, now: i64) {
    let key = canonical(path);
    list.retain(|r| canonical(&r.path) != key);
    list.insert(
        0,
        Recent {
            path: key,
            opened: now,
            tip: None,
        },
    );
    list.truncate(MAX_RECENTS);
}

pub fn forget(list: &mut Vec<Recent>, path: &Path) {
    let key = canonical(path);
    list.retain(|r| canonical(&r.path) != key);
}

pub fn relocated(path: &Path, old: &Path, new: &Path) -> Option<PathBuf> {
    let rest = path.strip_prefix(old).ok()?;
    Some(if rest.as_os_str().is_empty() {
        new.to_path_buf()
    } else {
        new.join(rest)
    })
}

pub fn relocate(list: &mut Vec<Recent>, old: &Path, new: &Path) -> usize {
    let mut moved = 0;
    for recent in list.iter_mut() {
        if let Some(path) = relocated(&recent.path, old, new) {
            recent.path = path;
            moved += 1;
        }
    }
    let mut seen = Vec::new();
    list.retain(|r| {
        let key = canonical(&r.path);
        let fresh = !seen.contains(&key);
        seen.push(key);
        fresh
    });
    moved
}

pub fn set_tip(list: &mut [Recent], path: &Path, tip: String) {
    let key = canonical(path);
    if let Some(recent) = list.iter_mut().find(|r| canonical(&r.path) == key) {
        recent.tip = Some(tip);
    }
}

pub fn tip_of<'a>(list: &'a [Recent], path: &Path) -> Option<&'a str> {
    list.iter()
        .find(|r| r.path == path)
        .and_then(|r| r.tip.as_deref())
}

pub fn find_moved(missing: &Path, tip: Option<&str>, known: &[PathBuf]) -> Option<PathBuf> {
    if missing.exists() {
        return None;
    }
    let parent = missing.parent()?;
    let holds_tip = |repo: &Path| tip.is_none_or(|tip| has_commit(repo, tip));
    if parent.parent().is_some() && parent.join(".git").exists() && holds_tip(parent) {
        return Some(parent.to_path_buf());
    }
    let mut siblings: Vec<PathBuf> = std::fs::read_dir(parent)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join(".git").exists())
        .collect();
    siblings.sort();
    match tip {
        Some(tip) => siblings.into_iter().find(|p| has_commit(p, tip)),
        None => {
            let known: Vec<PathBuf> = known.iter().map(|p| canonical(p)).collect();
            siblings.retain(|p| !known.contains(&canonical(p)));
            (siblings.len() == 1).then(|| siblings.remove(0))
        }
    }
}

fn is_hex_sha(sha: &str) -> bool {
    !sha.is_empty() && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

fn has_commit(repo: &Path, sha: &str) -> bool {
    if !is_hex_sha(sha) {
        return false;
    }
    std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["cat-file", "-e"])
        .arg(format!("{sha}^{{commit}}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
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
                tip: None,
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
    fn relocating_rewrites_paths_inside_the_old_folder() {
        let (old, new) = (Path::new("/w/kelp/kelp"), Path::new("/w/kelp"));
        assert_eq!(
            relocated(Path::new("/w/kelp/kelp/sub"), old, new).unwrap(),
            Path::new("/w/kelp/sub")
        );
        assert_eq!(relocated(old, old, new).unwrap(), new);
        assert!(relocated(Path::new("/w/kelp/kelp2"), old, new).is_none());
        let mut list = vec![
            Recent {
                path: "/nope/kelp/kelp".into(),
                opened: 2,
                tip: None,
            },
            Recent {
                path: "/nope/kelp".into(),
                opened: 1,
                tip: None,
            },
        ];
        let (old, new) = (Path::new("/nope/kelp/kelp"), Path::new("/nope/kelp"));
        assert_eq!(relocate(&mut list, old, new), 1);
        assert_eq!(paths(&list), ["/nope/kelp"]);
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kelp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn commit_in(dir: &Path) -> String {
        std::fs::create_dir_all(dir).unwrap();
        git(dir, &["init", "-q"]);
        git(
            dir,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                dir.to_str().unwrap(),
            ],
        );
        git(dir, &["rev-parse", "HEAD"])
    }

    #[test]
    fn finds_the_surviving_parent_repository() {
        let dir = scratch("parent");
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        assert_eq!(find_moved(&dir.join("kelp"), None, &[]), Some(dir.clone()));
        assert_eq!(find_moved(&dir, None, &[]), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_enclosing_repository_further_up_is_not_a_guess() {
        let dir = scratch("enclosing");
        let tip = commit_in(&dir.join("renamed"));
        commit_in(&dir);
        let deep = dir.join("renamed").join("gone").join("repo");
        assert_eq!(find_moved(&deep, None, &[]), None);
        let beside = dir.join("old-name");
        assert_eq!(
            find_moved(&beside, Some(&tip), &[]),
            Some(dir.join("renamed"))
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn finds_a_renamed_folder_by_its_last_commit() {
        let dir = scratch("renamed");
        let tip = commit_in(&dir.join("trakt-workers"));
        let other = commit_in(&dir.join("other"));
        let old = dir.join("trakt-hyperdrive");
        let found = find_moved(&old, Some(&tip), &[]);
        assert_eq!(found, Some(dir.join("trakt-workers")));
        assert_ne!(tip, other);
        assert_eq!(find_moved(&old, None, &[]), None);
        let known = [dir.join("trakt-workers")];
        assert_eq!(find_moved(&old, Some(&tip), &known), found);
        assert_eq!(find_moved(&old, Some("--all"), &[]), None);
        assert_eq!(
            find_moved(&old, None, &[dir.join("other")]),
            Some(dir.join("trakt-workers"))
        );
        std::fs::remove_dir_all(&dir).unwrap();
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
