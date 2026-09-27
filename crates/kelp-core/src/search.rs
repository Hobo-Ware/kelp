use std::path::Path;

use gix::ObjectId;

pub const MAX_RESULTS: usize = 5_000;

pub fn matching_rows(dir: &Path, ids: &[ObjectId], query: &str) -> anyhow::Result<Vec<usize>> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let looks_like_hash = needle.len() >= 4 && needle.chars().all(|c| c.is_ascii_hexdigit());
    let repo = gix::discover(dir)?;
    let mut rows = Vec::new();
    for (row, id) in ids.iter().enumerate() {
        if looks_like_hash && id.to_hex().to_string().starts_with(&needle) {
            rows.push(row);
        } else if let Ok(commit) = repo.find_commit(*id) {
            let message = commit.message_raw_sloppy().to_string().to_lowercase();
            let author = commit
                .author()
                .map(|a| format!("{} {}", a.name, a.email).to_lowercase())
                .unwrap_or_default();
            if message.contains(&needle) || author.contains(&needle) {
                rows.push(row);
            }
        }
        if rows.len() >= MAX_RESULTS {
            break;
        }
    }
    Ok(rows)
}
