# Kelp - ledger

A running log of what got done, newest first. The plan lives in
[PLAN.md](PLAN.md).

## Status

- **Current milestone:** publishing (needs the user's go-ahead: public repo, tap, DNS).
- **Done:** M0 to M7, MIT license, polish pass, mascot, update check, release pipeline,
  website, agent rules.
- **Was queued:** GitHub Pages
  site like stdusk (SEO, OG images, upkeep instructions); Homebrew
  publishing like stdusk with auto-install on release and an in-app
  update check.

## 2026-09-27

- **Distribution, built locally.** Modeled on stdusk.
  - In-app update check (stdusk has none): latest GitHub release at
    start + every 6 h; dot on Settings and a tab-strip pill; background
    `brew upgrade --cask hobo-ware/tap/kelp` when installed with brew,
    then "Restart to update". Settings has an Updates section.
  - `release.yml` (tag `v*`: tests, universal app, optional signing,
    GitHub Release, cask pushed to `Hobo-Ware/homebrew-tap`),
    `pages.yml`, `packaging/` docs and reference cask. Cask generation
    simulated locally, `ruby -c` passes; `postflight_steps` confirmed in
    Homebrew's source.
  - Icon is now the mascot; `Kelp.icns` built from compressed PNGs.
  - `site/` landing page with SEO, OG image and screenshots from public
    repos only (git/git, the kelp repo), all rebuilt by
    `scripts/make-site-assets.sh` with PSNR checks (47-59 dB).
  - Agent rules (`AGENTS.md`, `.agents/rules/project.md`) with the
    showcase check, like stdusk.

- **Polish pass.** One global egui style (spacing, 4px controls with
  clear states, accent focus, soft popup and dialog shadows, floating
  scrollbars). All right-click menus rebuilt as painted 30px rows with
  icons, hover fills, red-tinted danger items, inset separators and a
  mono header. Staging commit box made a self-sizing bottom panel
  (the button was clipped). Checkboxes squared off.
- **Mascot.** A kelp frond with a face whose air bladders are commit
  dots (`crates/kelp/assets/mascot.svg`), now also the app icon. Drawn
  natively in egui with sway, bobbing bladders, blinks and bubbles:
  loading screen, a welcome screen that waves for 4 s then holds still
  (and sways on hover), bubbles next to running jobs. Idle CPU still
  0.0% on the welcome screen and with trakt-web open.

- **M7 done: staging and committing.**
  - Unstaged / Staged / Conflicts sections with hover Stage/Unstage,
    Stage all / Unstage all, discard with confirm.
  - Staged and unstaged diffs with Stage hunk / Unstage hunk (exact
    one-hunk patches via `git apply --cached`).
  - Commit box with summary counter, description, amend, Cmd+Enter.
  - 49 tests pass (6 new staging round trips, status split, hunk
    patches). Checked visually on the demo repo with a partly staged
    file.

- Full retest after the user allowed Kelp through the firewall:
  - Live avatars work: with an empty cache Kelp fetched every visible
    trakt-web author itself (GitHub API, no-reply, Gravatar); an unknown
    email gets a real "not found" in ~0.5 s and is cached as a miss.
  - 41 tests pass, clippy clean, layout 1.4 ms / 14 ms (100k / 1M),
    git/git loads in 133 ms.
  - Scroll benchmarks now run ~2x slower than earlier because macOS
    throttles Kelp's window to ~10 fps while it sits behind other
    windows (slower CPU cores). Avatars on/off make no difference.
    Still under target: smooth scroll 0.20 ms (trakt-web), 0.60 ms
    (git/git).
  - Bugs found and fixed: offline test runs saved "avatars off" and a
    scratch repo into the user's real settings; avatar lookups could
    queue without limit on huge repos (now a newest-first queue of 48).
- User feedback "colors burn": the cause was the pastel fills of
  generated avatars, not the lane colors. Generated avatars are now
  dark tinted discs with colored initials. Lane palette unchanged.

- **M6 done.**
  - Settings window, saved to Application Support: descriptions in the
    graph, fade commits outside the selected history, avatar downloads.
  - Cmd+F search in the background over messages, authors, emails and
    hash prefixes, with next/previous; non-matches fade.
  - Bundled IBM Plex Sans (Regular, SemiBold) and JetBrains Mono, as in
    the design. Semibold headings and selected row.
  - Fallback font (23 MB) now loads on demand: memory with trakt-web
    open went from ~174 MB to ~130 MB (under the 150 MB target).
  - `scripts/bundle-macos.sh` builds `target/Kelp.app` (13 MB) with an
    icon rendered from `crates/kelp/assets/icon.svg`.
  - Session restore: reopens last tabs; Finder launch shows the empty
    state instead of failing on `/`.
  - README.

### Final numbers (release build, M-series Mac)

| What | Result | Target |
|---|---|---|
| Load trakt-web (5.9k commits) | 21-23 ms | - |
| Load git/git (86k), with commit-graph | 151 ms | under 200 ms for 100k |
| Layout, 1M generated commits | 14 ms | - |
| Graph frame, smooth scroll, trakt-web | 0.09 ms avg, 0.15 ms p95 | under 2 ms |
| Graph frame, smooth scroll, git/git | 0.24 ms avg, 0.40 ms p95 | under 2 ms |
| Graph frame, random jumps, git/git | 1.88 ms avg, 2.97 ms p95 | under 2 ms |
| Idle CPU | 0.0% | 0% |
| Memory, trakt-web open | ~130 MB | under 150 MB |
| Tests | 41 passing | - |

Known limits:
- History loads in one go; paging is still needed before a 1M-commit
  real repo (layout itself handles 1M in 14 ms).
- Random jumps on huge repos can spike to ~3 ms (text layout of all-new
  rows).
- egui has no right-to-left text reordering.
- This machine blocks network for new binaries, so live avatar HTTP was
  verified via curl with the same rules; expect a firewall prompt.

- **M5 done.**
  - Details: Path/Tree toggle, "All files" browser (lazy folders),
    comment badges per file.
  - Diff view: Diff/File and Unified/Split modes; unchanged files open
    as File.
  - Inline comments with reply, resolve/reopen, delete; resolved threads
    fold into a chip; off-screen threads listed at the top.
  - Stored in `.git/kelp/comments.json`, anchored by line text + 2 lines
    of context each side. Tests: follows its line after edits above,
    picks the copy with matching context, reports lost anchors, survives
    restart, Markdown export groups by file.
  - Verified visually on the demo repo in unified and split modes.

- **M3 and M4 done** (built together, they share dialogs and plumbing).
  - `Op` type in core describes every git write once; the same value
    drives the dialog preview, the job, and the tests.
  - Toolbar (Fetch, Pull, Push, Branch, Worktree, Stash, Pop), right-click
    menus everywhere, dialogs with command previews, toasts.
  - Worktrees page and sidebar section, new/remove/prune, open in
    terminal, open in a new tab. Repo tabs with a folder picker.
  - Round-trip tests against scratch repos: branch lifecycle, forced
    delete, stash push/pop, worktree add/list/remove/prune,
    ahead/behind vs a real upstream. All pass.
  - Visual check on a demo clone with 3 worktrees and a stash.
  - Skipped on purpose: Undo/Redo from the design (not in the asked
    scope, and risky to fake).

- **M1 done.**
  - Diff view for commits and uncommitted changes (hunks, line numbers,
    colored rows, Esc to go back). Uses the `similar` crate.
  - "Uncommitted changes" row above HEAD, dashed ring, change counts.
  - Graph column: drag to resize, double-click to reset, sideways scroll.
  - Background job runner: status, reload and commit-graph writes run
    off the UI thread. Reload on window focus.
  - Commit-graph file written automatically when missing.
  - Fallback system font for non-Latin scripts. Known limit: egui has no
    right-to-left reordering, so mixed Arabic/English text can show
    words in the wrong order.
- **M2 done.** Avatars: GitHub no-reply, GitHub API (one call per
  author, `gh auth token` when present), Gravatar `d=404`, then
  generated. Disk cache in `~/Library/Caches/kelp/avatars`, round mask
  applied once, textures only for visible rows, 4 worker threads.
  - Checked on trakt-web's last 25 authors: 16 via the GitHub API, 4 via
    no-reply emails, 2 fell back to generated.
  - Bug found and fixed while testing: a network error was cached as a
    miss for 7 days. Now only a real "not found" from every source is
    cached.
  - Note: this machine blocks network access for freshly built binaries
    (raw TCP times out, curl works), so the live HTTP path was checked
    via curl + `gh api` with the same rules, and Kelp read the result
    from its cache. Expect a firewall prompt on first run.
- Scroll benchmark after these changes: smooth scroll 0.11 ms (trakt-web)
  and 0.32 ms (git/git); random jumps 0.72 ms and 2.14 ms.

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
