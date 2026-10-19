use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, bail};

const CASKROOMS: [&str; 2] = ["/opt/homebrew/Caskroom", "/usr/local/Caskroom"];
const REPAIR_WAIT_SECONDS: u32 = 300;

/// Where an app's releases are published and which Homebrew cask installs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// GitHub `owner/name` whose latest release is the newest version.
    pub repo: String,
    /// Cask as `owner/tap/name`, or just `name` for a cask in a default tap.
    pub cask: String,
    /// Sent with release lookups.
    pub user_agent: String,
}

impl Source {
    /// The user agent defaults to `<cask name>-updater`.
    pub fn new(repo: impl Into<String>, cask: impl Into<String>) -> Self {
        let cask = cask.into();
        let user_agent = format!("{}-updater", cask_name(&cask));
        Self {
            repo: repo.into(),
            cask,
            user_agent,
        }
    }

    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    fn tap(&self) -> Option<&str> {
        self.cask.rsplit_once('/').map(|(tap, _)| tap)
    }
}

/// Why `brew_upgrade` did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradeError {
    /// A half-finished upgrade left this folder behind. Brew refuses to touch the cask until it is repaired.
    Stuck(PathBuf),
    Failed(String),
}

impl std::fmt::Display for UpgradeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stuck(path) => write!(
                f,
                "a previous upgrade did not finish and left {}",
                path.display()
            ),
            Self::Failed(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for UpgradeError {}

fn cask_name(cask: &str) -> &str {
    cask.rsplit('/').next().unwrap_or(cask)
}

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

pub fn latest_release(source: &Source) -> anyhow::Result<Release> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .user_agent(source.user_agent.as_str())
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let url = format!("https://github.com/{}/releases/latest", source.repo);
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

pub fn installed_with_brew(source: &Source) -> bool {
    let name = cask_name(&source.cask);
    CASKROOMS
        .iter()
        .any(|root| Path::new(root).join(name).exists())
}

/// The `<version>.upgrading` folder a killed `brew upgrade` leaves in the cask's Caskroom.
pub fn stuck_upgrade(source: &Source) -> Option<PathBuf> {
    let roots: Vec<&Path> = CASKROOMS.iter().map(Path::new).collect();
    stuck_in(&roots, cask_name(&source.cask))
}

fn stuck_in(roots: &[&Path], cask: &str) -> Option<PathBuf> {
    roots.iter().find_map(|root| {
        let mut stuck: Vec<PathBuf> = std::fs::read_dir(root.join(cask))
            .ok()?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".upgrading"))
            .map(|entry| entry.path())
            .collect();
        stuck.sort();
        stuck.into_iter().next()
    })
}

pub fn brew_upgrade(source: &Source) -> Result<(), UpgradeError> {
    if let Some(path) = stuck_upgrade(source) {
        return Err(UpgradeError::Stuck(path));
    }
    run_brew_upgrade(source).map_err(|e| UpgradeError::Failed(format!("{e:#}")))
}

fn run_brew_upgrade(source: &Source) -> anyhow::Result<()> {
    let brew = brew().context("Homebrew is not installed")?;
    if let Some(tap_name) = source.tap() {
        let tap = Command::new(&brew)
            .args(["--repository", tap_name])
            .output()?;
        let tap_dir = String::from_utf8_lossy(&tap.stdout).trim().to_string();
        if tap.status.success() && Path::new(&tap_dir).exists() {
            let args = ["-C", tap_dir.as_str(), "pull", "--quiet", "--ff-only"];
            let log = crate::console::start("git", &args, Path::new(&tap_dir));
            if let Ok(output) = Command::new("git").args(args).output() {
                log.finish_output(&output);
            }
        }
    }
    let mut upgrade = Command::new(&brew);
    upgrade
        .args(["upgrade", "--cask", source.cask.as_str()])
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_INSTALL_CLEANUP", "1");
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut upgrade, 0);
    let out = upgrade.output()?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let reason = match stderr.trim() {
            "" => out.status.to_string(),
            text => text.to_string(),
        };
        bail!("brew upgrade failed: {reason}");
    }
    Ok(())
}

/// A shell script that waits for `pid` to exit, reinstalls the cask and opens the app again.
pub fn repair_command(source: &Source, app_path: &Path, pid: u32) -> String {
    let app = app_path
        .file_stem()
        .map_or_else(|| cask_name(&source.cask).into(), |s| s.to_string_lossy());
    let cask = sh_quote(&source.cask);
    let app = sh_quote(&app);
    let limit = REPAIR_WAIT_SECONDS;
    format!(
        "export PATH=/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin\n\
         waited=0\n\
         while kill -0 {pid} 2>/dev/null; do\n\
         \x20 waited=$((waited + 1))\n\
         \x20 if [ \"$waited\" -gt {limit} ]; then echo \"the app is still running, giving up\"; exit 1; fi\n\
         \x20 sleep 1\n\
         done\n\
         HOMEBREW_NO_AUTO_UPDATE=1 brew reinstall --cask --force {cask} || exit 1\n\
         open -a {app}\n"
    )
}

/// Starts [`repair_command`] in its own process group so it outlives the app, with output going to `log`.
pub fn start_repair(source: &Source, app_path: &Path, pid: u32, log: &Path) -> std::io::Result<()> {
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let out = std::fs::File::create(log)?;
    let mut helper = Command::new("/bin/sh");
    helper
        .arg("-c")
        .arg(repair_command(source, app_path, pid))
        .stdin(std::process::Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut helper, 0);
    helper.spawn().map(drop)
}

fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
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
    fn a_source_names_its_tap_and_cask_from_the_cask_path() {
        let source = Source::new("Hobo-Ware/kelp", "hobo-ware/tap/kelp");
        assert_eq!(source.tap(), Some("hobo-ware/tap"));
        assert_eq!(cask_name(&source.cask), "kelp");
        assert_eq!(source.user_agent, "kelp-updater");
        assert_eq!(
            source.with_user_agent("kelp-git-client").user_agent,
            "kelp-git-client"
        );
        let bare = Source::new("o/r", "app");
        assert_eq!(bare.tap(), None);
        assert_eq!(cask_name(&bare.cask), "app");
    }

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

    fn caskroom(name: &str, entries: &[&str]) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("kelp-caskroom-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for entry in entries {
            std::fs::create_dir_all(root.join(entry)).unwrap();
        }
        root
    }

    #[test]
    fn an_upgrading_folder_marks_the_cask_stuck() {
        let cases: [(&str, &[&str], Option<&str>); 5] = [
            ("clean", &["app/0.1.10"], None),
            (
                "stuck",
                &["app/0.1.10.upgrading"],
                Some("app/0.1.10.upgrading"),
            ),
            (
                "both",
                &["app/0.1.9", "app/0.1.10.upgrading"],
                Some("app/0.1.10.upgrading"),
            ),
            ("other-cask", &["other/0.1.10.upgrading"], None),
            ("not-installed", &[], None),
        ];
        for (name, entries, expected) in cases {
            let root = caskroom(name, entries);
            let found = stuck_in(&[root.as_path()], "app");
            assert_eq!(found, expected.map(|e| root.join(e)), "{name}");
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn the_first_caskroom_with_a_stuck_folder_wins() {
        let clean = caskroom("first-clean", &["app/1.0.0"]);
        let stuck = caskroom("first-stuck", &["app/1.0.1.upgrading"]);
        let found = stuck_in(&[clean.as_path(), stuck.as_path()], "app");
        assert_eq!(found, Some(stuck.join("app/1.0.1.upgrading")));
        let _ = std::fs::remove_dir_all(&clean);
        let _ = std::fs::remove_dir_all(&stuck);
    }

    #[test]
    fn a_stuck_upgrade_reads_as_a_sentence() {
        let error = UpgradeError::Stuck(PathBuf::from("/tmp/x/app/1.0.upgrading"));
        assert_eq!(
            error.to_string(),
            "a previous upgrade did not finish and left /tmp/x/app/1.0.upgrading"
        );
        assert_eq!(UpgradeError::Failed("boom".into()).to_string(), "boom");
    }

    #[test]
    fn the_repair_script_waits_reinstalls_and_reopens() {
        let source = Source::new("Hobo-Ware/kelp", "hobo-ware/tap/kelp");
        let script = repair_command(&source, Path::new("/tmp/x/Kelp.app"), 4242);
        let lines: Vec<&str> = script.lines().collect();
        assert_eq!(
            lines,
            [
                "export PATH=/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
                "waited=0",
                "while kill -0 4242 2>/dev/null; do",
                "  waited=$((waited + 1))",
                "  if [ \"$waited\" -gt 300 ]; then echo \"the app is still running, giving up\"; exit 1; fi",
                "  sleep 1",
                "done",
                "HOMEBREW_NO_AUTO_UPDATE=1 brew reinstall --cask --force 'hobo-ware/tap/kelp' || exit 1",
                "open -a 'Kelp'",
            ]
        );
    }

    #[test]
    fn the_repair_script_quotes_every_path() {
        let source = Source::new("o/r", "tap's/odd cask");
        let script = repair_command(&source, Path::new("/tmp/My Apps/It's.app"), 1);
        assert!(script.contains("--force 'tap'\\''s/odd cask'"));
        assert!(script.contains("open -a 'It'\\''s'"));
        let bare = repair_command(&Source::new("o/r", "app"), Path::new("/"), 1);
        assert!(bare.contains("open -a 'app'"));
    }

    #[test]
    fn quoting_survives_the_shell() {
        for text in ["plain", "with space", "it's", "$HOME `x` \"q\"", ""] {
            let out = Command::new("/bin/sh")
                .arg("-c")
                .arg(format!("printf %s {}", sh_quote(text)))
                .output()
                .unwrap();
            assert_eq!(String::from_utf8_lossy(&out.stdout), text);
        }
    }
}
