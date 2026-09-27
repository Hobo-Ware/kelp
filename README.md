# Kelp

A small, fast, low-power git client for macOS, written in Rust with
[egui](https://github.com/emilk/egui) and
[gitoxide](https://github.com/GitoxideLabs/gitoxide). It draws a
GitKraken-style commit graph and covers the everyday work: branches,
worktrees, stashes, inspecting commits and reviewing files.

## Run it

```sh
cargo run --release -- ~/path/to/repo      # open a repo
./scripts/bundle-macos.sh                  # build target/Kelp.app
```

With no path, Kelp reopens the tabs from last time, or the current
folder if it is a repo.

## What it does

- **Graph:** lanes with rounded corners, rows tinted in the branch
  color, avatars (GitHub, then Gravatar, then generated initials),
  branch and tag labels. Drag the graph column edge to resize; scroll
  sideways for wide histories.
- **Commits:** message, author, parents, changed files as a list or a
  tree, or every file in the commit.
- **Diffs:** unified or split, or the full file. Uncommitted changes get
  their own row above HEAD.
- **Review:** hover a line in a diff, click **+**, and leave a comment.
  Threads support replies and resolving, and follow their line when code
  above them changes. Comments live in `.git/kelp/comments.json`, are
  never pushed, and can be copied as Markdown.
- **Branches:** check out, create, rename, delete (local and remote),
  merge, rebase, fetch, pull, push. Every action shows the git command
  it runs.
- **Worktrees:** list, create (new or existing branch), remove, prune,
  open in a new tab or a terminal.
- **Stashes:** stash, pop, apply, drop.
- **Search:** Cmd+F across messages, authors, emails and hash prefixes.

## Keys

| Key | Action |
|---|---|
| Up / Down, J / K | Move through commits |
| Cmd+F | Search; Enter / Shift+Enter for next / previous |
| Esc | Back to the graph, close search or dialogs |
| Cmd+Enter | Save a review comment or reply |
| Right-click | Actions for commits, branches, tags, stashes, worktrees |
| Double-click a branch | Check it out |

## Where Kelp keeps things

| What | Where |
|---|---|
| Settings and open tabs | `~/Library/Application Support/kelp/settings.json` |
| Avatar cache | `~/Library/Caches/kelp/avatars` |
| Review comments | `<repo>/.git/kelp/comments.json` |

If a repo has no commit-graph file, Kelp writes one in the background
(`git commit-graph write --reachable --changed-paths`), which makes
every later open much faster.

## Speed

Measured on an M-series Mac in release builds. Full numbers are in
[LEDGER.md](LEDGER.md).

| | trakt-web (6k commits) | git/git (86k commits) |
|---|---|---|
| Open to first graph | 21 ms | 151 ms |
| Frame while scrolling | 0.09 ms | 0.24 ms |
| Idle CPU | 0% | 0% |

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo bench -p kelp-core --bench layout
cargo run --release -p kelp-core --example load -- ~/path/to/repo
```

Developer switches (environment variables):

| Variable | Effect |
|---|---|
| `KELP_SCREENSHOT=out.png` | Save the window to a PNG and quit (`KELP_SCREENSHOT_WAIT=2` waits first) |
| `KELP_BENCH_SCROLL=1` | Print graph frame times for random jumps and smooth scrolling |
| `KELP_OFFLINE=1` | No avatar downloads |
| `KELP_OPEN_DIFF=1` / `split` | Open the first changed file of HEAD |
| `KELP_OPEN_WORKTREES=1`, `KELP_OPEN_DIALOG=worktree`, `KELP_OPEN_SETTINGS=1`, `KELP_SEARCH=text` | Open a screen on start |

The plan, design rules and decisions are in [PLAN.md](PLAN.md); progress
is logged in [LEDGER.md](LEDGER.md). Fonts: IBM Plex Sans and JetBrains
Mono, both under the SIL Open Font License (see `crates/kelp/assets/fonts`).
