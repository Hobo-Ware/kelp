# Kelp - project rules

A small, fast, low-power git client for macOS in Rust (egui/eframe UI, gitoxide for reads, the
git CLI for writes). The plan and decisions live in `PLAN.md`; progress, with measured numbers,
in `LEDGER.md`.

## Layout

```
crates/kelp-core/   git + graph logic, no UI (history, graph lanes, diff, status, ops,
                    review comments, avatars, search, update check). Unit + round-trip tests.
crates/kelp/        the egui app (graph view, sidebar, details, staging, diff/review,
                    worktrees, dialogs, menus, mascot, updater, settings)
site/               the public page (GitHub Pages, kelp.hoboware.dev)
packaging/          Info.plist template, reference cask, release docs
scripts/            bundle-macos.sh, make-icon.sh, make-site-assets.sh
```

## Commands

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo bench -p kelp-core --bench layout
./scripts/bundle-macos.sh                        # target/Kelp.app
KELP_SCREENSHOT=/tmp/x.png cargo run -p kelp -- <repo>   # render, save PNG, quit
KELP_BENCH_SCROLL=1 cargo run --release -p kelp -- <repo> # graph frame times
```

Verify UI changes with the screenshot switch instead of asking for a manual check. The other
dev switches (`KELP_OPEN_*`, `KELP_SEARCH`, `KELP_SELECT_WIP`, `KELP_FAKE_UPDATE`,
`KELP_OFFLINE`) are listed in the README. Dev runs never save settings or open tabs.

## Performance rules

- Idle CPU must stay at 0%: repaint only on input or finished background work; animations run
  only while something is happening (loading, jobs, a short hello) and then stop.
- Never block the UI thread on git, the network or disk-heavy work; use the job runner.
- The graph draws only visible rows; keep per-row work O(visible lanes).
- Re-run `KELP_BENCH_SCROLL=1` on trakt-web and git/git after graph or text changes and log
  the numbers in `LEDGER.md`. Targets are in `PLAN.md`.

## Showcase page

`site/index.html` (deployed on push to `main`) and the README's feature list are the public
face. When work is done, check both against what shipped:

- A user-visible feature, a new shortcut, or a changed number (speed, memory) gets added or
  corrected there, in the same branch.
- Removed or renamed behavior comes off; never leave a claim the app no longer backs.
- The nav version pill and the JSON-LD `softwareVersion` track `Cargo.toml` on every release.
- Screenshots come from the screenshot switch on the kelp repo or the fictional demo repo that
  `scripts/make-demo-repo.py` generates, never a private or work repo. Rebuild them with
  `scripts/make-site-assets.sh`, which compresses them (pngquant + oxipng) and checks PSNR.
- Internal refactors and fixes the page never mentioned need no change - say so in the wrap-up.

## Releases

Bump the workspace version, update the showcase page and `LEDGER.md`, then tag `vX.Y.Z` and
push the tag. `release.yml` builds, publishes and updates the Homebrew tap; installed copies
update themselves. Details: `packaging/README.md`.

## Commit standards

- Conventional Commits: `feat: ...`, `fix: ...`, `docs: ...`, `chore: ...`, optional scope.
- Clean author line: no `Co-Authored-By` trailer, no "Generated with" footer.
- Plain hyphens only in commits and GitHub prose, no em or en dashes.
- Commit or push only when asked. One logical change per commit.

## Quick checklist

- [ ] Read `LEDGER.md` + `PLAN.md` before coding; update `LEDGER.md` after.
- [ ] Tests, clippy and fmt pass.
- [ ] UI change verified with a screenshot; perf-sensitive change re-benchmarked.
- [ ] Showcase check: `site/index.html` + README still match what shipped.
