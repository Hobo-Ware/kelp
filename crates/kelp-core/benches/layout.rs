use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use kelp_core::graph::layout;

fn synthetic_history(commits: usize, branches: usize) -> Vec<Vec<u32>> {
    let mut seed: u64 = 0x5eed;
    let mut next = move |n: u64| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) % n
    };
    let mut parents = vec![Vec::new(); commits];
    let mut open: Vec<usize> = Vec::new();
    for row in (0..commits.saturating_sub(1)).rev() {
        let parent = row + 1;
        if open.len() < branches && next(8) == 0 {
            open.push(parent);
        }
        if !open.is_empty() && next(6) == 0 {
            let lane = next(open.len() as u64) as usize;
            let branch_parent = open.swap_remove(lane);
            parents[row] = vec![parent as u32, branch_parent as u32];
            continue;
        }
        parents[row] = vec![parent as u32];
    }
    parents
}

fn bench_layout(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout");
    group.sample_size(10);
    for commits in [100_000usize, 1_000_000] {
        let parents = synthetic_history(commits, 12);
        group.bench_with_input(
            BenchmarkId::from_parameter(commits),
            &parents,
            |b, parents| {
                b.iter(|| layout(parents.len(), |r| &parents[r], 8));
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_layout);
criterion_main!(benches);
