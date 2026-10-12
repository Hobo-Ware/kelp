use kelp_core::avatar::GitHubRepo;
use kelp_core::pulls::{self, Pull};

pub fn load(github: &GitHubRepo, force: bool) -> Vec<Pull> {
    if let Some(path) = std::env::var_os("KELP_FAKE_PULLS") {
        return std::fs::read_to_string(path)
            .ok()
            .and_then(|json| pulls::parse_gh(&json).ok())
            .unwrap_or_default();
    }
    let cache = pulls::cache_file(github);
    let cached = pulls::load_cached(&cache);
    if let Some((list, true)) = &cached
        && !force
    {
        return list.clone();
    }
    match pulls::fetch(github) {
        Ok(list) => {
            pulls::save_cached(&cache, &list);
            list
        }
        Err(_) => cached.map(|(list, _)| list).unwrap_or_default(),
    }
}

pub fn open_url(url: &str) -> std::io::Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(url)
        .spawn()
        .map(|_| ())
}
