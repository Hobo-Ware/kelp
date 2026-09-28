use std::path::PathBuf;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let started = Instant::now();
    let (repo, history) = kelp_core::history::History::open(&path)?;
    let total = started.elapsed();
    let with_repo = resident_mb();
    drop(repo);
    let t = history.timings;
    println!("{}", path.display());
    println!("  commits : {}", history.len());
    println!("  lanes   : {}", history.layout.lane_count());
    println!("  refs    : {}", history.refs.labels.len());
    println!("  walk    : {:?}", t.walk);
    println!("  sort    : {:?}", t.sort);
    println!("  layout  : {:?}", t.layout);
    println!("  total   : {total:?}");
    println!(
        "  memory  : {with_repo} MB with the repo open, {} MB history only",
        resident_mb()
    );
    Ok(())
}

fn resident_mb() -> u64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok();
    out.and_then(|o| {
        String::from_utf8_lossy(&o.stdout)
            .trim()
            .parse::<u64>()
            .ok()
    })
    .map_or(0, |kb| kb / 1024)
}
