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

pub fn ahead_behind(dir: &Path, branch: &str) -> Option<(usize, usize)> {
    let range = format!("{branch}...{branch}@{{upstream}}");
    let out = git_cli::run(dir, &["rev-list", "--left-right", "--count", &range]).ok()?;
    parse_ahead_behind(&out)
}

pub fn parse_ahead_behind(out: &str) -> Option<(usize, usize)> {
    let mut parts = out.split_whitespace().map(|n| n.parse::<usize>().ok());
    Some((parts.next()??, parts.next()??))
}

pub fn change_count(dir: &Path) -> Option<usize> {
    let out = git_cli::run(
        dir,
        &["status", "--porcelain=v1", "-z", "--untracked-files=normal"],
    )
    .ok()?;
    Some(crate::status::parse_porcelain(&out).len())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(parse_ahead_behind("3\t2\n"), Some((3, 2)));
        assert_eq!(parse_ahead_behind(""), None);
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
