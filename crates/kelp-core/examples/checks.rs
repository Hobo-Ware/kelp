use std::time::Instant;

use kelp_core::pulls::ListTab;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let (repo, history) = kelp_core::history::History::open(std::path::Path::new(&path))
        .expect("not a git repository");
    let github = kelp_core::avatar::GitHubRepo::from_repo(&repo).expect("not a GitHub repo");
    let oids: Vec<String> = history
        .ids()
        .iter()
        .take(100)
        .map(|id| id.to_string())
        .collect();

    let started = Instant::now();
    match kelp_core::checks::fetch(&github, &oids) {
        Ok(found) => {
            let with = found.iter().filter(|(_, s)| s.is_some()).count();
            let failing = found
                .iter()
                .filter(|(_, s)| {
                    s.as_ref()
                        .is_some_and(|s| s.state == kelp_core::checks::State::Failure)
                })
                .count();
            println!(
                "checks: {} commits in {:?} ({with} with checks, {failing} failing)",
                found.len(),
                started.elapsed()
            );
        }
        Err(e) => println!("checks failed after {:?}: {e}", started.elapsed()),
    }

    for tab in [ListTab::Open, ListTab::Mine, ListTab::ReviewRequested] {
        let started = Instant::now();
        match kelp_core::pulls::list_open(&github, tab) {
            Ok(list) => println!(
                "{tab:?}: {} pull requests in {:?}",
                list.len(),
                started.elapsed()
            ),
            Err(e) => println!("{tab:?} failed after {:?}: {e:#}", started.elapsed()),
        }
    }
}
