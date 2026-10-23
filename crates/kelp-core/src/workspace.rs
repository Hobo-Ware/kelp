use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::git_cli;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stash {
    pub name: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: String,
    pub branch: Option<String>,
    pub is_main: bool,
    pub locked: bool,
    pub prunable: bool,
}

impl Worktree {
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.display().to_string())
    }
}

pub fn upstream_remote(repo: &gix::Repository, branch: &str) -> Option<String> {
    repo.config_snapshot()
        .string(format!("branch.{branch}.remote").as_str())
        .map(|r| r.to_string())
}

pub fn upstreams<'a>(
    repo: &gix::Repository,
    branches: impl IntoIterator<Item = &'a str>,
) -> Vec<(String, String)> {
    let config = repo.config_snapshot();
    branches
        .into_iter()
        .filter_map(|branch| {
            let remote = config.string(format!("branch.{branch}.remote").as_str())?;
            let merge = config.string(format!("branch.{branch}.merge").as_str())?;
            let merge = merge.to_string();
            let target = merge.strip_prefix("refs/heads/").unwrap_or(&merge);
            Some((branch.to_string(), format!("{remote}/{target}")))
        })
        .collect()
}

/// The remote the tag dialogs start on: the one last used for a tag here, if it still exists,
/// else the default push remote, else the first remote.
pub fn tag_remote(remotes: &[String], last: Option<&str>) -> Option<String> {
    last.and_then(|last| remotes.iter().find(|r| *r == last).cloned())
        .or_else(|| default_push_remote(remotes))
        .or_else(|| remotes.first().cloned())
}

fn last_tag_remote_file(common_dir: &Path) -> PathBuf {
    common_dir.join("kelp").join("tag-remote")
}

pub fn last_tag_remote(common_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(last_tag_remote_file(common_dir)).ok()?;
    Some(text.trim().to_string()).filter(|r| !r.is_empty())
}

pub fn save_last_tag_remote(common_dir: &Path, remote: &str) -> std::io::Result<()> {
    let file = last_tag_remote_file(common_dir);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, remote)
}

pub fn default_push_remote(remotes: &[String]) -> Option<String> {
    match remotes {
        [only] => Some(only.clone()),
        many => many.iter().find(|r| *r == "origin").cloned(),
    }
}

pub fn stashes(dir: &Path) -> anyhow::Result<Vec<Stash>> {
    let out = git_cli::run(dir, &["stash", "list", "--format=%gd%x1f%s"])?;
    Ok(parse_stashes(&out))
}

pub fn parse_stashes(out: &str) -> Vec<Stash> {
    out.lines()
        .filter_map(|line| line.split_once('\u{1f}'))
        .map(|(name, message)| Stash {
            name: name.to_string(),
            message: message.to_string(),
        })
        .collect()
}

pub fn worktrees(dir: &Path) -> anyhow::Result<Vec<Worktree>> {
    let out = git_cli::run(dir, &["worktree", "list", "--porcelain"])?;
    Ok(parse_worktrees(&out))
}

pub fn parse_worktrees(out: &str) -> Vec<Worktree> {
    out.split("\n\n")
        .filter(|block| !block.trim().is_empty())
        .enumerate()
        .filter_map(|(i, block)| {
            let mut tree = Worktree {
                path: PathBuf::new(),
                head: String::new(),
                branch: None,
                is_main: i == 0,
                locked: false,
                prunable: false,
            };
            for line in block.lines() {
                let (key, value) = line.split_once(' ').unwrap_or((line, ""));
                match key {
                    "worktree" => tree.path = PathBuf::from(value),
                    "HEAD" => tree.head = value.to_string(),
                    "branch" => {
                        tree.branch = Some(value.trim_start_matches("refs/heads/").to_string())
                    }
                    "locked" => tree.locked = true,
                    "prunable" => tree.prunable = true,
                    _ => {}
                }
            }
            (!tree.path.as_os_str().is_empty()).then_some(tree)
        })
        .collect()
}

/// Ahead/behind counts for every local branch that tracks an upstream, from one
/// `for-each-ref` call. Branches whose upstream is gone are left out.
pub fn ahead_behind(dir: &Path) -> HashMap<String, (usize, usize)> {
    git_cli::run(
        dir,
        &[
            "for-each-ref",
            "--format=%(refname:lstrip=2)%00%(upstream)%00%(upstream:track,nobracket)",
            "refs/heads",
        ],
    )
    .map(|out| parse_ahead_behind(&out))
    .unwrap_or_default()
}

pub fn parse_ahead_behind(out: &str) -> HashMap<String, (usize, usize)> {
    out.lines()
        .filter_map(|line| {
            let mut fields = line.split('\0');
            let (branch, upstream, track) = (fields.next()?, fields.next()?, fields.next()?);
            if upstream.is_empty() || track == "gone" {
                return None;
            }
            let count = |word: &str| {
                track
                    .split(", ")
                    .find_map(|part| part.strip_prefix(word)?.trim().parse().ok())
                    .unwrap_or(0)
            };
            Some((branch.to_string(), (count("ahead"), count("behind"))))
        })
        .collect()
}

pub fn change_count(dir: &Path) -> Option<usize> {
    let out = git_cli::run(
        dir,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=normal",
        ],
    )
    .ok()?;
    Some(crate::status::parse_porcelain(&out).all().len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_start_on_the_last_used_remote_then_fall_back() {
        let names = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let both = names(&["origin", "fork"]);
        assert_eq!(tag_remote(&both, Some("fork")).as_deref(), Some("fork"));
        assert_eq!(tag_remote(&both, Some("gone")).as_deref(), Some("origin"));
        assert_eq!(tag_remote(&both, None).as_deref(), Some("origin"));
        let odd = names(&["a", "b"]);
        assert_eq!(tag_remote(&odd, None).as_deref(), Some("a"));
        assert_eq!(tag_remote(&[], Some("fork")), None);
    }

    #[test]
    fn the_last_tag_remote_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("kelp-tag-remote-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(last_tag_remote(&dir), None);
        save_last_tag_remote(&dir, "fork").unwrap();
        assert_eq!(last_tag_remote(&dir).as_deref(), Some("fork"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn push_goes_to_the_only_remote_or_origin() {
        let names = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            default_push_remote(&names(&["fork"])).as_deref(),
            Some("fork")
        );
        assert_eq!(
            default_push_remote(&names(&["fork", "origin"])).as_deref(),
            Some("origin")
        );
        assert_eq!(default_push_remote(&names(&["fork", "upstream"])), None);
        assert_eq!(default_push_remote(&[]), None);
    }

    #[test]
    fn parses_worktree_porcelain() {
        let out = "worktree /repo/kelp\nHEAD aaa\nbranch refs/heads/main\n\n\
                   worktree /repo/kelp-review\nHEAD bbb\nbranch refs/heads/feat/inline-review\nlocked\n\n\
                   worktree /repo/kelp-old\nHEAD ccc\ndetached\nprunable gitdir file points to non-existent location\n";
        let trees = parse_worktrees(out);
        assert_eq!(trees.len(), 3);
        assert!(trees[0].is_main && trees[0].branch.as_deref() == Some("main"));
        assert_eq!(trees[1].name(), "kelp-review");
        assert!(trees[1].locked);
        assert_eq!(trees[1].branch.as_deref(), Some("feat/inline-review"));
        assert!(trees[2].branch.is_none() && trees[2].prunable);
    }

    #[test]
    fn parses_ahead_behind_counts() {
        let out = "main\0refs/remotes/origin/main\0behind 4\n\
                   feat/a\0refs/remotes/origin/feat/a\0ahead 3, behind 2\n\
                   feat/b\0refs/remotes/origin/feat/b\0\n\
                   feat/gone\0refs/remotes/origin/feat/gone\0gone\n\
                   local\0\0\n";
        let counts = parse_ahead_behind(out);
        assert_eq!(counts.len(), 3);
        assert_eq!(counts["main"], (0, 4));
        assert_eq!(counts["feat/a"], (3, 2));
        assert_eq!(counts["feat/b"], (0, 0));
        assert!(parse_ahead_behind("").is_empty());
    }

    #[test]
    fn parses_stash_list() {
        let stashes =
            parse_stashes("stash@{0}\u{1f}WIP on main: abc fix\nstash@{1}\u{1f}On feat: tweak\n");
        assert_eq!(
            stashes[1],
            Stash {
                name: "stash@{1}".into(),
                message: "On feat: tweak".into()
            }
        );
    }
}
