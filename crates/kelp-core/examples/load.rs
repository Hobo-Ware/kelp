use std::path::PathBuf;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let started = Instant::now();
    let (_repo, history) = kelp_core::history::History::open(&path)?;
    let total = started.elapsed();
    let t = history.timings;
    println!("{}", path.display());
    println!("  commits : {}", history.len());
    println!("  lanes   : {}", history.layout.lane_count());
    println!("  refs    : {}", history.refs.labels.len());
    println!("  walk    : {:?}", t.walk);
    println!("  sort    : {:?}", t.sort);
    println!("  layout  : {:?}", t.layout);
    println!("  total   : {total:?}");
    Ok(())
}
