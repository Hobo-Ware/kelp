# Kelp - plan

A small, fast, low-power desktop git client written in Rust, with a
history graph that looks better than GitKraken's.

Progress is logged in [LEDGER.md](LEDGER.md). This file holds what we
are building and the decisions behind it. Update it when a decision
changes, not for day-to-day progress.

## Goals

- A great-looking commit graph: colored lanes, smooth curves, avatars.
- Manage branches: create, check out, rename, delete, merge, rebase,
  fetch, pull, push.
- Manage worktrees: list, add, remove, prune, open in a new tab.
- Inspect commits: message, author, parents, changed files, diffs.
- Browse the file tree of any commit and review files, with inline
  comments.
- Near 0% CPU when idle. Low memory. Fast start.

## Not doing (for now)

- Pull requests, issues, cloud features, AI features.
- Interactive rebase editor, conflict resolution UI.
- Windows support is nice to have, not a target. macOS first, Linux second.

## Designs

Canvas: https://claude.ai/artifact/D1nTRbzP3FPE2chnRhhS8k

Boards: graph view, file review with inline comments, branches and
worktrees, new worktree dialog, avatars and graph dots.

Visual rules the app must follow:

- Dark theme, flat fills, no gradients or blur. Everything is cheap for
  egui to draw.
- Lane palette: teal `#2dd4bf`, orange `#fb923c`, violet `#c084fc`,
  blue `#60a5fa`, pink `#f472b6`. Accent (kelp green) `#8fd16a`.
- Rows are 30px. Lanes are 24px apart. Avatars are 20px with a 2px
  lane color ring (about 75% of the row, so neighbors never crowd).
- Lane lines: 2.25px solid strokes. No glow by default (it read as an
  outline, not a glow). It may come back as an optional setting.
- Lines only run straight down a lane or flat along a row, joined by a
  true circular arc with a radius of half a row (15px). Not a bezier: a
  quadratic corner looks pinched. No diagonal or S-shaped curves:
  - A branch that forked off runs down its lane to the parent's row,
    turns, and runs flat into the parent dot.
  - A merge runs flat out of the merge dot along its own row, turns, and
    runs down the lane to the merged parent.
- Every commit row gets a band tinted in its lane color (about 9%
  alpha), starting at the dot's center (hidden under it) and running to
  the message column, plus a 3px lane color strip at the start of the
  message. The selected row's band is stronger (about 30%) and its
  message area turns blue.
- Every commit, merges included, gets the same avatar dot, so the column
  of circles stays even. The selected commit gets an extra soft halo.
  Uncommitted changes are a dashed ring.
- Optional (off by default): selecting a commit dims lanes outside its
  history. Fading made colors look muddy when always on.
- Branch labels are rounded pills joined to their dot by a thin line.
  The checked-out branch is a solid pill. Remote-only is dashed. Tags
  use a mono font. Long names are cut off with "…".

## Stack

| Area | Choice | Why |
|---|---|---|
| UI | `egui` via `eframe` | Native, GPU drawn, only repaints on input, so it idles near 0% CPU. Easy custom drawing for the graph. |
| Git reads | `gix` (gitoxide) | Pure Rust and fast for walking history, refs, trees and diffs. |
| Git writes | `git` CLI | Branch, worktree, fetch, pull and push behave exactly like the terminal: same auth, hooks and config. |
| HTTP (avatars) | `ureq` on a background thread | Small, blocking, no async runtime needed. |
| Images | `image` | Decode avatar PNG/JPEG into egui textures. |

## Layout

A Cargo workspace so the git and graph logic can be tested without a UI:

```
crates/
  kelp-core/   git reading, graph lane layout, avatars, comments (no UI)
  kelp/        the egui app (binary)
```

## How key parts work

### Graph layout

- Walk commits in topological order with `gix`, newest first, in pages
  so huge repos open instantly.
- One pass assigns each commit a lane (column). A lane frees up when its
  branch merges or forks off, and gets reused lower down.
- Each branch keeps its color even when its lane position is reused.
- Only rows on screen are laid out and drawn.

### Avatars (in priority order)

1. **GitHub.** No-reply emails (`12345+name@users.noreply.github.com`)
   map straight to `avatars.githubusercontent.com/u/12345`. Older
   `name@users.noreply.github.com` map to `github.com/name.png`. For
   other emails, when the repo's remote is on GitHub, ask the GitHub API
   for one commit per unique author. Use `gh auth token` when available
   to avoid the low anonymous rate limit.
2. **Gravatar.** `gravatar.com/avatar/<sha256 of email>?d=404`. A 404
   means no picture, so fall through.
3. **Generated.** Initials on a soft color picked from a hash of the
   email. Drawn locally.

Results (and misses) are cached on disk in the OS cache folder, keyed by
email hash, with misses retried after 7 days. Fetches run in the
background and only for rows on screen.

### Inline review comments

- Stored per repo in `.git/kelp/comments.json`, so they are never
  committed or pushed. Worktrees share the same store.
- Anchored to file path + blob id + line, with a few lines of context so
  a comment can follow its line when the file changes.
- Export a review as Markdown. "Send to Claude" is a later idea.

### Speed with large repos

Kelp has to stay smooth on repos with hundreds of thousands of commits.

Targets (checked with benchmarks, not by feel):

| What | Target |
|---|---|
| Open a 100k-commit repo to first painted graph | under 200 ms |
| Open a 1M-commit repo to first painted graph | under 1 s |
| Scrolling | matches the display refresh rate (120 Hz on ProMotion) |
| Graph + rows drawing per frame | under 2 ms |
| Memory for 100k commits | under 150 MB |
| Idle CPU | 0% |

How:

- **Read less.** Use git's commit-graph file (through `gix`) to get
  parents and order without opening each commit. Load message, author
  and date only for rows near the screen, on a background thread.
- **Stream.** Walk history in pages. The first screen paints as soon as
  its page is ready; the rest keeps loading in the background.
- **Compact data.** Commits are stored as plain arrays of numbers
  (indexes, not strings or pointers). Commit ids are interned once.
- **Lay out once.** The lane layout runs a single pass as pages arrive
  and stores, per row, only the few line pieces crossing it. Scrolling
  never recomputes layout.
- **Draw only what's visible.** Each frame looks up the visible row
  range and draws just those rows' line pieces, dots and text. Cost
  stays the same at row 10 or row 900,000.
- **Cache the expensive bits.** Text layout (egui galleys) cached per
  row; avatars decoded once into GPU textures and shared by author.
- **Never block the UI thread.** Git reads, diffs, avatar fetches and
  file watching all run on worker threads and send results back.
- **Benchmarks in the repo.** `criterion` benches for walk and layout on
  generated 100k and 1M commit repos, plus a real large repo (e.g. a
  clone of the Linux kernel) run by hand. Numbers go in the ledger.

### Power use

- egui runs in reactive mode: no repaint unless there is input or a
  background task finished.
- No animations that run in a loop.
- File watching (to spot outside changes) is debounced, and paused
  while the window is hidden.

## Milestones

Each milestone ends with a check we can actually run.

| # | Milestone | Done when |
|---|---|---|
| M0 | Workspace, window opens, CI (fmt, clippy, test) | `cargo run` opens an empty Kelp window; Activity Monitor shows ~0% CPU when idle. |
| M1 | Graph and commit view: open a repo, lanes, rounded-corner lines, row bands, glow, branch labels, select a commit, details and file list, diff view | Lane layout unit tests pass on fixture repos; benchmarks meet the speed targets above on generated 100k and 1M commit repos. |
| M2 | Avatars: GitHub, then Gravatar, then generated, with disk cache | Tests for the email rules; a repo with mixed authors shows all three kinds. |
| M3 | Branches: sidebar, check out, create, rename, delete, merge, rebase, fetch, pull, push | Each action works on a scratch repo and the graph refreshes. |
| M4 | Worktrees: list, add (dialog shows the git command), remove, prune, open in tab | Round trip on a scratch repo. |
| M5 | File tree and review: tree mode, file view, inline comments, Markdown export | Comments survive restart and follow their line after an edit above them. |
| M6 | Polish: repo tabs, focus-path dimming, search, settings, `.app` bundle | Usable as a daily driver. |
| M7 | Staging and committing: staged/unstaged lists, stage/unstage/discard files, stage/unstage hunks, commit and amend | Round-trip tests on scratch repos: stage one hunk of two, unstage it, commit, amend, discard. |

## Decisions

- **Commit-graph:** when a repo has no commit-graph file, Kelp writes one
  in the background (`git commit-graph write --reachable
  --changed-paths`), like `git gc` does. First open is normal speed,
  every open after is fast. (2026-09-27)
- **Wide histories:** the graph column is resizable from its header
  (double-click resets) and scrolls sideways when there are more lanes
  than fit. (2026-09-27)
- **License:** MIT, confirmed by the user. (2026-09-27)
- **External changes:** instead of always-on file watching, Kelp reloads
  refs and status when the window regains focus. Cheaper on power.
  (2026-09-27)

Status: M0 to M6 done on 2026-09-27. Details and numbers in
[LEDGER.md](LEDGER.md).

- **Distribution:** Homebrew cask in `Hobo-Ware/homebrew-tap`, released by a
  `v*` tag like stdusk. Installed copies update themselves in the
  background via `brew upgrade` and ask for a restart. Site at
  `kelp.hoboware.dev` from `site/`. (2026-09-27)

## Open questions

- Should the "Send to Claude" review button be in scope, and what should
  it send? (The review card has Copy as Markdown for now.)
- Next features worth considering, not in the original scope: undo/redo,
  interactive rebase, line-level staging.
- Signing and notarizing the `.app` if it will be shared with others.
- Linux: code paths exist (fonts, cache, terminal), but nothing has been
  tested there.
