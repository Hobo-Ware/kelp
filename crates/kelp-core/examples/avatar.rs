fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (email, commit, repo) = (
        &args[0],
        args.get(1).cloned().unwrap_or_default(),
        args.get(2).cloned(),
    );
    let github = repo.and_then(|r| kelp_core::avatar::GitHubRepo::from_remote_url(&r));
    let token = kelp_core::avatar::gh_token();
    println!("github: {github:?}, token: {}", token.is_some());
    let resolver = kelp_core::avatar::Resolver::new(github, token);
    let started = std::time::Instant::now();
    let found = resolver.resolve(email, &commit).map(|i| i.size);
    println!("{email}: {found:?} in {:?}", started.elapsed());
}
