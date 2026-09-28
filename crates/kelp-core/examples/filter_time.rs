fn main() {
    let path = std::env::args().nth(1).expect("repo path");
    let (_, history) = kelp_core::history::History::open(std::path::Path::new(&path)).unwrap();
    for spec in [("Junio", ""), ("", "builtin/"), ("Junio", "builtin/")] {
        let filter = kelp_core::filter::Filter {
            author: spec.0.into(),
            path: spec.1.into(),
            ..Default::default()
        };
        let started = std::time::Instant::now();
        let rows = kelp_core::filter::matching_rows(
            std::path::Path::new(&path),
            history.ids(),
            &filter,
            0,
            &|| false,
        )
        .unwrap()
        .unwrap();
        println!(
            "author={:?} path={:?}: {} of {} in {:?}",
            spec.0,
            spec.1,
            rows.iter().filter(|&&k| k).count(),
            rows.len(),
            started.elapsed()
        );
    }
}
