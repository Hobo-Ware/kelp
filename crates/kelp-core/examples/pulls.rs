fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let repo = gix::discover(&path).expect("not a git repository");
    let github = kelp_core::avatar::GitHubRepo::from_repo(&repo).expect("not a GitHub repo");
    println!("github: {}/{}", github.owner, github.name);
    let started = std::time::Instant::now();
    match kelp_core::pulls::fetch(&github) {
        Ok(list) => println!(
            "{} pull requests in {:?}, {} with checks",
            list.len(),
            started.elapsed(),
            list.iter().filter(|p| p.checks.is_some()).count()
        ),
        Err(e) => println!("failed after {:?}: {e:#}", started.elapsed()),
    }
}
