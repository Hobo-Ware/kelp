# Kelp - ledger

A running log of what got done, newest first. The plan lives in
[PLAN.md](PLAN.md).

## Status

- **Current milestone:** M1 (graph and commit view), about half done.
- **Done in M1:** history loading, lane layout, graph drawing, branch
  sidebar, commit details with changed files, keyboard navigation.
- **Next up in M1:** diff view for a changed file, uncommitted changes
  row, resizable/scrollable graph column for very wide histories.

## 2026-09-26

- Picked the stack: egui/eframe for the UI, gitoxide for reading git,
  the git CLI for writes.
- Designed five screens: graph view, file review with inline comments,
  branches and worktrees, new worktree dialog, name options.
- Picked the name **Kelp**.
- Reworked the graph design: glow under lane lines, avatars inside lane
  color rings, hollow dots for merges, halo on HEAD, dimming for lanes
  outside the selected commit's history, pill branch labels.
- Designed avatars in priority order: GitHub, then Gravatar, then
  generated initials. Replaced the name board with an avatars board.
- Created the repo with PLAN.md and LEDGER.md.
- Graph redesign, round 2: the S-curves looked stretched next to
  GitKraken. Switched to straight lanes joined by tight rounded corners,
  tinted row bands in the lane color with a lane strip at the message,
  denser 32px rows and 24px avatars. Kept the glow, halo and dimming.
- Added speed targets and the approach for large repos to PLAN.md, and
  made the M1 check a benchmark against those targets.
- Graph redesign, round 3: rendered the graph locally and compared it
  with GitKraken. Found five problems: the glow read as an outline,
  always-on fading made colors muddy, merge dots looked like glitches,
  dots were too big for the row, and bands stuck out left of the dots.
  Fixed all five: no glow, fading off by default, avatars on every
  commit, 20px avatars on 30px rows with 24px lanes, bands start at the
  dot's center.
- Graph corners: swapped the quadratic curve corners (pinched, 10px) for
  true circular arcs with a half-row radius (15px), matching GitKraken.
- **M0 done.** Cargo workspace (`kelp-core` + `kelp`), eframe 0.35 window
  in the design's palette, CI (fmt, clippy, test). Idle CPU 0.0% after
  startup, about 118 MB memory (mostly the graphics context).
- **M1 started.**
  - Lane layout in `kelp-core/src/graph.rs`: one pass, each row stores
    only the line pieces crossing it. 6 unit tests.
  - History loading with gix: walks all branches, remotes and tags,
    sorts children-first and newest-first, uses the commit-graph file.
  - Graph view in egui drawn to the design rules; sidebar; details panel
    with changed files; up/down and j/k navigation.
  - Dev switches: `KELP_SCREENSHOT=file.png` saves the window and quits;
    `KELP_BENCH_SCROLL=1` prints graph frame times.

### Numbers (release build, M-series Mac)

| What | Result | Target |
|---|---|---|
| Layout, 100k generated commits | 1.3 ms | - |
| Layout, 1M generated commits | 14 ms | - |
| Load trakt-boxed (5.7k commits) | 7 ms | - |
| Load trakt-web (5.9k commits) | 21 ms | - |
| Load git/git (86k), with commit-graph | 66 ms (example tool), 151 ms in app | under 200 ms for 100k |
| Load git/git (86k), without commit-graph | 642 ms | under 200 ms for 100k |
| Graph frame, smooth scroll, trakt-web | 0.09 ms avg, 0.20 ms p95 | under 2 ms |
| Graph frame, smooth scroll, git/git | 0.39 ms avg, 0.81 ms p95 | under 2 ms |
| Graph frame, random jumps, git/git | 1.88 ms avg, 3.0 ms p95 | under 2 ms |
| Idle CPU with trakt-web open | 0.0% | 0% |
| Memory with trakt-web open | about 133 MB | - |

Notes:
- Without a commit-graph file, loading is 10x slower. See the open
  question in PLAN.md about writing one in the background.
- Random jumps (like dragging the scrollbar far) are dominated by text
  layout, because every row on screen is new. Smooth scrolling reuses
  most rows and is far under target.
- git/git has up to 282 lanes open at once; the graph column is capped
  at 14 lanes for now, so far-right lines are cut off.
- Commit summaries load on the UI thread, on demand, for rows on screen.
  Cheap so far (see random-jump numbers); move to a worker if that
  changes.
- History still loads in one go, not in pages. Fine up to git/git size;
  needed before trying a 1M-commit repo.
