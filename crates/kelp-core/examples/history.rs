use std::sync::atomic::AtomicBool;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().expect("usage: history <repo> <path>"));
    let path = args.next().expect("usage: history <repo> <path>");
    let started = Instant::now();
    let mut first = None;
    let mut total = 0;
    kelp_core::file_history::stream(&dir, &path, &AtomicBool::new(false), |batch| {
        first.get_or_insert_with(|| started.elapsed());
        total += batch.len();
    })
    .expect("file history");
    println!(
        "history: {total} commits, first rows after {:?}, all after {:?}",
        first.unwrap_or_default(),
        started.elapsed()
    );
    let started = Instant::now();
    let blame = kelp_core::blame::run(&dir, None, &path, &AtomicBool::new(false))
        .expect("blame")
        .expect("not cancelled");
    println!(
        "blame: {} lines from {} commits in {:?}",
        blame.lines.len(),
        blame.origins.len(),
        started.elapsed()
    );
}
