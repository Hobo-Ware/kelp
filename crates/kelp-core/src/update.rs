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
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let url = format!("https://github.com/{REPO}/releases/latest");
    let response = agent.get(&url).call()?;
    let location = response
        .headers()
        .get("location")
        .and_then(|l| l.to_str().ok())
        .with_context(|| {
            format!(
                "GitHub answered {} without a release link",
                response.status()
            )
        })?;
    let version = version_from_release_url(location).context("the latest release has no tag")?;
    Ok(Release {
        version,
        url: location.to_string(),
    })
}

pub fn version_from_release_url(url: &str) -> Option<String> {
    let tag = url.rsplit_once("/releases/tag/")?.1;
    let tag = tag.split(['?', '#']).next()?;
    parse_version(tag)
        .is_some()
        .then(|| tag.trim_start_matches('v').to_string())
}

pub fn installed_version() -> Option<String> {
    let plist = std::fs::read_to_string(app_bundle()?.join("Contents/Info.plist")).ok()?;
    plist_version(&plist)
}

pub fn plist_version(plist: &str) -> Option<String> {
    let after = plist.split_once("<key>CFBundleShortVersionString</key>")?.1;
    let value = after.split_once("<string>")?.1.split_once("</string>")?.0;
    Some(value.trim().to_string())
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
    fn the_release_redirect_names_the_version() {
        assert_eq!(
            version_from_release_url("https://github.com/Hobo-Ware/kelp/releases/tag/v0.8.4")
                .as_deref(),
            Some("0.8.4")
        );
        assert_eq!(
            version_from_release_url("https://github.com/Hobo-Ware/kelp/releases"),
            None
        );
        assert_eq!(version_from_release_url("https://github.com/login"), None);
    }

    #[test]
    fn the_bundle_plist_names_the_installed_version() {
        let plist = "<dict>\n  <key>CFBundleName</key>\n  <string>Kelp</string>\n  <key>CFBundleShortVersionString</key>\n  <string>0.8.6</string>\n</dict>";
        assert_eq!(plist_version(plist).as_deref(), Some("0.8.6"));
        assert_eq!(plist_version("<dict></dict>"), None);
    }

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
