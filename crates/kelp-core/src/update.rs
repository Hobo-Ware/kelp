use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, bail};

pub const REPO: &str = "Hobo-Ware/kelp";
pub const CASK: &str = "hobo-ware/tap/kelp";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub url: String,
}

pub fn parse_version(text: &str) -> Option<Vec<u64>> {
    let core = text
        .trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()?;
    core.split('.').map(|part| part.parse().ok()).collect()
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(a), Some(b)) => {
            let len = a.len().max(b.len());
            let pad = |v: &Vec<u64>| {
                (0..len)
                    .map(|i| v.get(i).copied().unwrap_or(0))
                    .collect::<Vec<_>>()
            };
            pad(&a) > pad(&b)
        }
        _ => false,
    }
}

pub fn latest_release() -> anyhow::Result<Release> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .user_agent("kelp-git-client")
        .build()
        .into();
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body = agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()?
        .into_body()
        .read_to_vec()?;
    let json: serde_json::Value = serde_json::from_slice(&body)?;
    let tag = json
        .get("tag_name")
        .and_then(|t| t.as_str())
        .context("release has no tag")?;
    let page = json
        .get("html_url")
        .and_then(|u| u.as_str())
        .unwrap_or_default();
    Ok(Release {
        version: tag.trim_start_matches('v').to_string(),
        url: page.to_string(),
    })
}

fn brew() -> Option<PathBuf> {
    ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
}

pub fn installed_with_brew() -> bool {
    ["/opt/homebrew/Caskroom/kelp", "/usr/local/Caskroom/kelp"]
        .iter()
        .any(|p| Path::new(p).exists())
}

pub fn brew_upgrade() -> anyhow::Result<()> {
    let brew = brew().context("Homebrew is not installed")?;
    let tap = Command::new(&brew)
        .args(["--repository", "hobo-ware/tap"])
        .output()?;
    let tap_dir = String::from_utf8_lossy(&tap.stdout).trim().to_string();
    if tap.status.success() && Path::new(&tap_dir).exists() {
        let args = ["-C", tap_dir.as_str(), "pull", "--quiet", "--ff-only"];
        let log = crate::console::start("git", &args, Path::new(&tap_dir));
        if let Ok(output) = Command::new("git").args(args).output() {
            log.finish_output(&output);
        }
    }
    let out = Command::new(&brew)
        .args(["upgrade", "--cask", CASK])
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_INSTALL_CLEANUP", "1")
        .output()?;
    if !out.status.success() {
        bail!(
            "brew upgrade failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

pub fn app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("0.10.0", "0.9.3"));
        assert!(is_newer("v1.0.0", "0.99.99"));
        assert!(is_newer("0.2", "0.1.9"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
        assert!(!is_newer("0.2.0-beta.1", "0.2.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }

    #[test]
    fn parses_tags_with_prefix_and_suffix() {
        assert_eq!(parse_version("v0.3.1"), Some(vec![0, 3, 1]));
        assert_eq!(parse_version("1.2.3+build.7"), Some(vec![1, 2, 3]));
        assert_eq!(parse_version("x.y"), None);
    }
}
