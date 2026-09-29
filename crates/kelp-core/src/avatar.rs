use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

pub const SIZE: u32 = 64;
const MISS_TTL: Duration = Duration::from_secs(7 * 24 * 3600);
const HIT_TTL: Duration = Duration::from_secs(30 * 24 * 3600);

pub fn initials(name: &str) -> String {
    let mut words = name
        .split_whitespace()
        .filter(|w| w.chars().next().is_some_and(char::is_alphanumeric));
    let first = words.next().and_then(|w| w.chars().next());
    let last = words.next_back().and_then(|w| w.chars().next());
    match (first, last) {
        (Some(a), Some(b)) => format!("{a}{b}").to_uppercase(),
        (Some(a), None) => a.to_uppercase().to_string(),
        _ => "?".into(),
    }
}

pub fn color_index(email: &str, colors: usize) -> usize {
    let hash = normalize(email)
        .bytes()
        .fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ b as u64).wrapping_mul(0x100000001b3)
        });
    (hash % colors as u64) as usize
}

fn normalize(email: &str) -> String {
    email.trim().to_lowercase()
}

fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn github_noreply_url(email: &str) -> Option<String> {
    let email = normalize(email);
    let user = email.strip_suffix("@users.noreply.github.com")?;
    match user.split_once('+') {
        Some((id, _)) if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) => Some(format!(
            "https://avatars.githubusercontent.com/u/{id}?s={SIZE}&v=4"
        )),
        _ if !user.is_empty() => Some(github_owner_url(user)),
        _ => None,
    }
}

pub fn github_owner_url(owner: &str) -> String {
    format!("https://github.com/{owner}.png?size={SIZE}")
}

pub fn gravatar_url(email: &str) -> String {
    format!(
        "https://gravatar.com/avatar/{}?s={SIZE}&d=404",
        sha256_hex(&normalize(email))
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubRepo {
    pub owner: String,
    pub name: String,
}

impl GitHubRepo {
    pub fn from_remote_url(url: &str) -> Option<Self> {
        let rest = url
            .strip_prefix("git@github.com:")
            .or_else(|| url.strip_prefix("ssh://git@github.com/"))
            .or_else(|| url.strip_prefix("https://github.com/"))
            .or_else(|| url.strip_prefix("http://github.com/"))?;
        let rest = rest.trim_end_matches('/');
        let rest = rest.strip_suffix(".git").unwrap_or(rest);
        let (owner, name) = rest.split_once('/')?;
        (!owner.is_empty() && !name.is_empty() && !name.contains('/')).then(|| Self {
            owner: owner.to_string(),
            name: name.to_string(),
        })
    }

    pub fn from_repo(repo: &gix::Repository) -> Option<Self> {
        let remote = repo.find_remote("origin").ok()?;
        let url = remote.url(gix::remote::Direction::Fetch)?;
        Self::from_remote_url(&url.to_bstring().to_string())
    }
}

pub struct Image {
    pub size: u32,
    pub rgba: Vec<u8>,
}

pub struct Resolver {
    cache_dir: PathBuf,
    github: Option<GitHubRepo>,
    token: Option<String>,
    agent: ureq::Agent,
}

impl Resolver {
    pub fn new(github: Option<GitHubRepo>, token: Option<String>) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(8)))
            .user_agent("kelp-git-client")
            .build()
            .into();
        Self {
            cache_dir: cache_dir().join("avatars"),
            github,
            token,
            agent,
        }
    }

    pub fn resolve(&self, email: &str, commit: &str) -> Option<Image> {
        self.cached(&normalize(email), || self.download(email, commit))
    }

    pub fn resolve_owner(&self, owner: &str) -> Option<Image> {
        self.cached(&format!("github-owner:{}", normalize(owner)), || match self
            .get(&github_owner_url(owner))
        {
            Ok(Some(bytes)) => Lookup::Found(bytes),
            Ok(None) => Lookup::NotFound,
            Err(()) => Lookup::Failed,
        })
    }

    fn cached(&self, name: &str, download: impl FnOnce() -> Lookup) -> Option<Image> {
        let key = sha256_hex(name);
        let hit = self.cache_dir.join(format!("{key}.img"));
        let miss = self.cache_dir.join(format!("{key}.none"));
        if fresh(&hit, HIT_TTL)
            && let Some(image) = std::fs::read(&hit).ok().and_then(|b| decode_round(&b))
        {
            return Some(image);
        }
        if fresh(&miss, MISS_TTL) {
            return None;
        }
        let bytes = match download() {
            Lookup::Found(bytes) => bytes,
            Lookup::NotFound => {
                let _ = std::fs::create_dir_all(&self.cache_dir);
                let _ = std::fs::write(&miss, b"");
                return None;
            }
            Lookup::Failed => return None,
        };
        let image = decode_round(&bytes)?;
        let _ = std::fs::create_dir_all(&self.cache_dir);
        let _ = std::fs::write(&hit, &bytes);
        let _ = std::fs::remove_file(&miss);
        Some(image)
    }

    fn download(&self, email: &str, commit: &str) -> Lookup {
        let mut failed = false;
        let mut attempt = |result: Fetched| match result {
            Ok(Some(bytes)) => Some(bytes),
            Ok(None) => None,
            Err(()) => {
                failed = true;
                None
            }
        };
        let found = github_noreply_url(email)
            .and_then(|url| attempt(self.get(&url)))
            .or_else(|| match self.github_api_avatar(commit) {
                Ok(Some(url)) => attempt(self.get(&url)),
                other => attempt(other.map(|_| None)),
            })
            .or_else(|| attempt(self.get(&gravatar_url(email))));
        match found {
            Some(bytes) => Lookup::Found(bytes),
            None if failed => Lookup::Failed,
            None => Lookup::NotFound,
        }
    }

    fn github_api_avatar(&self, commit: &str) -> Result<Option<String>, ()> {
        let Some(repo) = self.github.as_ref() else {
            return Ok(None);
        };
        let url = format!(
            "https://api.github.com/repos/{}/{}/commits/{commit}",
            repo.owner, repo.name
        );
        let mut request = self
            .agent
            .get(&url)
            .header("Accept", "application/vnd.github+json");
        if let Some(token) = &self.token {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }
        let Some(body) = self.body(request.call())? else {
            return Ok(None);
        };
        let json: serde_json::Value = serde_json::from_slice(&body).map_err(|_| ())?;
        let Some(avatar) = json
            .get("author")
            .and_then(|a| a.get("avatar_url"))
            .and_then(|a| a.as_str())
        else {
            return Ok(None);
        };
        let separator = if avatar.contains('?') { '&' } else { '?' };
        Ok(Some(format!("{avatar}{separator}s={SIZE}")))
    }

    fn get(&self, url: &str) -> Fetched {
        self.body(self.agent.get(url).call())
    }

    fn body(&self, response: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Fetched {
        match response {
            Ok(response) => response.into_body().read_to_vec().map(Some).map_err(|_| ()),
            Err(ureq::Error::StatusCode(404 | 410 | 422)) => Ok(None),
            Err(_) => Err(()),
        }
    }
}

type Fetched = Result<Option<Vec<u8>>, ()>;

enum Lookup {
    Found(Vec<u8>),
    NotFound,
    Failed,
}

pub fn gh_token() -> Option<String> {
    let log = crate::console::start("gh", &["auth", "token"], Path::new(""));
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()?;
    log.finish(output.status.code(), b"(token hidden)", &output.stderr);
    let token = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (output.status.success() && !token.is_empty()).then_some(token)
}

fn fresh(path: &Path, ttl: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < ttl)
}

pub fn cache_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        home.join("Library/Caches/kelp")
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".cache"))
            .join("kelp")
    }
}

pub fn decode_round(bytes: &[u8]) -> Option<Image> {
    let image = image::load_from_memory(bytes).ok()?;
    let square = image
        .resize_to_fill(SIZE, SIZE, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let mut rgba = square.into_raw();
    let r = SIZE as f32 / 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 + 0.5 - r;
            let dy = y as f32 + 0.5 - r;
            let coverage = (r - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
            let i = ((y * SIZE + x) * 4 + 3) as usize;
            rgba[i] = (rgba[i] as f32 * coverage) as u8;
        }
    }
    Some(Image { size: SIZE, rgba })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_use_first_and_last_word() {
        assert_eq!(initials("Vlad Jerca"), "VJ");
        assert_eq!(initials("Mara Elena Ionescu"), "MI");
        assert_eq!(initials("dependabot[bot]"), "D");
        assert_eq!(initials("  "), "?");
    }

    #[test]
    fn same_email_always_gets_the_same_color() {
        assert_eq!(
            color_index("Vlad@Trakt.tv ", 8),
            color_index("vlad@trakt.tv", 8)
        );
    }

    #[test]
    fn github_noreply_emails_map_to_avatar_urls() {
        assert_eq!(
            github_noreply_url("12345+octo@users.noreply.github.com").as_deref(),
            Some("https://avatars.githubusercontent.com/u/12345?s=64&v=4")
        );
        assert_eq!(
            github_noreply_url("octo@users.noreply.github.com").as_deref(),
            Some("https://github.com/octo.png?size=64")
        );
        assert_eq!(github_noreply_url("vlad@trakt.tv"), None);
    }

    #[test]
    fn gravatar_uses_sha256_of_normalized_email_and_404s_on_miss() {
        assert_eq!(
            gravatar_url(" Test@Example.com "),
            "https://gravatar.com/avatar/973dfe463ec85785f5f95af5ba3906eedb2d931c24e69824a89ea65dba4e813b?s=64&d=404"
        );
    }

    #[test]
    fn parses_github_remote_urls() {
        let expected = Some(GitHubRepo {
            owner: "trakt".into(),
            name: "trakt-web".into(),
        });
        for url in [
            "git@github.com:trakt/trakt-web.git",
            "https://github.com/trakt/trakt-web",
            "https://github.com/trakt/trakt-web.git/",
            "ssh://git@github.com/trakt/trakt-web.git",
        ] {
            assert_eq!(GitHubRepo::from_remote_url(url), expected, "{url}");
        }
        assert_eq!(GitHubRepo::from_remote_url("git@gitlab.com:a/b.git"), None);
    }

    #[test]
    fn round_mask_clears_corners_and_keeps_center() {
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 100, 50, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let img = decode_round(&png).unwrap();
        let alpha = |x: u32, y: u32| img.rgba[((y * SIZE + x) * 4 + 3) as usize];
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(SIZE / 2, SIZE / 2), 255);
    }
}
