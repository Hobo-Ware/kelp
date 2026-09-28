<p align="center"><img src="site/mascot.svg" width="140" alt="Kelp mascot"></p>

# Kelp

A small, fast, low-power git client for macOS ([kelp.hoboware.dev](https://kelp.hoboware.dev)), written in Rust with
[egui](https://github.com/emilk/egui) and
[gitoxide](https://github.com/GitoxideLabs/gitoxide). It draws a
GitKraken-style commit graph and covers the everyday work: branches,
worktrees, stashes, inspecting commits and reviewing files.

## Install

```sh
brew install hobo-ware/tap/kelp
```

Kelp updates itself in the background when installed with Homebrew.

## Run from source

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
  sideways for wide histories. Hover a commit to light up its path to
  the branch tip and see its full message, author, date and hash.
  Uncommitted changes show up for every worktree, each above its own
  HEAD, with an Open button for the other worktrees.
- **Labels you can use:** click a branch or tag label to select its
  commit, right-click for its menu, double-click to check it out. The
  `+N` chip lists every ref on that commit, each with its own menu.
  Drag a label onto another commit or label to merge, rebase or reset.
- **Less noise:** hover a branch in the sidebar and click the eye to hide
  it from the graph, or right-click for Show only this branch. Hidden
  branches drop their own commits and lanes; the status bar chip shows
  them all again. Saved per repo in `.git/kelp/view.json`.
- **Columns:** right-click the column header to add Author, Date and
  Hash columns; drag their edges to resize.
- **Commits:** message, author, parents, changed files as a list or a
  tree, or every file in the commit.
- **Diffs:** unified or split, or the full file, with the changed words
  highlighted inside each line. Jump between changes with the arrows or
  Alt+Up / Alt+Down. Uncommitted changes get their own row above HEAD.
- **Previews:** images (PNG, JPEG, GIF, WebP, BMP, ICO, TIFF) and SVGs
  side by side before and after, and Markdown rendered with its images
  loaded from the repo.
- **Review:** hover a line in a diff, click **+**, and leave a comment.
  Threads support replies and resolving, and follow their line when code
  above them changes. Comments live in `.git/kelp/comments.json`, are
  never pushed, and can be copied as Markdown.
- **Branches:** check out, create, rename, delete (local and remote),
  merge, rebase, fetch, pull, push. Every action shows the git command
  it runs.
- **Worktrees:** list, create (new or existing branch), remove, prune,
  open in a new tab or a terminal.
- **Staging:** unstaged and staged lists, stage or unstage files, single hunks or
  single lines (click line numbers, Shift-click for a range), discard, commit and
  amend (hooks run as usual).
- **Stashes:** stash, pop, apply, drop.
- **Undo:** Cmd+Z undoes the last thing Kelp did (commit, amend, checkout,
  branch create/rename/delete, merge, rebase, pull, stash, discard,
  stage), and Cmd+Shift+Z redoes it. Undo refuses rather than lose work
  when the repo changed since; pushes and fetches can't be undone.
- **Conflicts:** a stopped merge, rebase, cherry-pick or revert shows a
  banner with Continue, Skip and Abort. Each conflicted file opens side
  by side, ours and theirs, with Use ours / theirs / both per conflict,
  a preview of the result, and whole-file choices (including delete vs
  modify).
- **Interactive rebase:** right-click a commit, then reorder the commits after it by
  dragging, and pick, reword, squash, fixup or drop each one. Messages are edited
  inline; no editor opens.
- **Search:** Cmd+F across messages, authors, emails and hash prefixes.
- **Command palette:** Cmd+K (or Cmd+Shift+P) finds any action, branch,
  commit, changed file or tab. Prefix with `>` for actions, `@` for
  branches, `#` for commits and `/` for files; Cmd+Enter on a branch lists
  its actions. Cmd+/ shows every shortcut.
- **Files:** right-click any file for Open in editor, Reveal in Finder and
  Copy path; the diff header has an editor button too. Pick the editor in
  Settings (empty uses the first installed of Cursor, VS Code, Zed and
  Sublime Text).
- **Stays current:** changes made from the command line or another app
  show up on their own, and remotes are fetched in the background every
  5 minutes (change or turn off in Settings).

## Keys

| Key | Action |
|---|---|
| Cmd+K / Cmd+Shift+P | Command palette: actions, branches, commits, files and tabs |
| Cmd+/ | Keyboard shortcuts |
| Up / Down, J / K | Move through commits |
| Cmd+F | Search; Enter / Shift+Enter for next / previous |
| Esc | Back to the graph, close search or dialogs |
| Cmd+T / Cmd+O | Open a repository in a new tab |
| Cmd+W | Close the tab |
| Cmd+1 ... Cmd+9 | Go to a tab (Cmd+9 is the last one) |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| Middle-click a tab | Close it |
| Drag a tab | Reorder tabs |
| Drop a folder on the window | Open it (a file opens the repository it is in) |
| Alt+Up / Alt+Down | Previous / next change in a diff |
| Cmd+Enter | Save a review comment or reply |
| Cmd+R | Refresh the graph, changes and worktrees |
| Cmd+Z / Cmd+Shift+Z | Undo / redo the last action |
| Cmd+, | Settings |
| Right-click | Actions for commits, branches, tags, stashes, worktrees and tabs |
| Double-click a branch | Check it out |
| Double-click a graph label | Check out that branch |
| Drag a graph label onto a commit | Merge, rebase or reset |
| P / R / S / F / D | In interactive rebase: pick, reword, squash, fixup or drop the hovered commit |

## Where Kelp keeps things

| What | Where |
|---|---|
| Settings, open tabs, window size and position | `~/Library/Application Support/kelp/settings.json` |
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
| `KELP_HIDE_REFS=a,b`, `KELP_COLUMNS=author,date,hash` | Hide branches or show columns for this run |
| `KELP_OPEN_DIFF=1` / `split` | Open the first changed file of the selected commit |
| `KELP_OPEN_WORKTREES=1`, `KELP_OPEN_DIALOG=worktree` (or `reset-hard`, `push-to`, `force-push`), `KELP_OPEN_SETTINGS=1`, `KELP_OPEN_PALETTE=<query>`, `KELP_OPEN_SHORTCUTS=1`, `KELP_SEARCH=text`, `KELP_OPEN_MENU=branch` (or `commit`, `tab`), `KELP_OPEN_CONFLICT=<file>` (add `#ours,theirs,both` to pre-pick, `:result` for the result), `KELP_OPEN_REFS=<commit>` (the `+N` ref list), `KELP_OPEN_DROP=<ref>@<commit>` (the drop menu) | Open a screen on start |
| `KELP_SELECT_COMMIT=<rev>`, `KELP_SELECT_WIP=1`, `KELP_OPEN_DIFF=path:<file>` / `preview:<file>` / `unstaged:<file>` | Select a commit or file on start |
| `KELP_FAKE_UPDATE=<version>` | Pretend a newer release exists |
| `KELP_HOVER_ROW=<row>` | Draw the graph as if that commit row were hovered |
| `KELP_OPEN_REBASE=<rev>` (or `<rev>:<letters>`, e.g. `HEAD~4:prsd`) | Open interactive rebase from a commit, optionally with actions preset |

The plan, design rules and decisions are in [PLAN.md](PLAN.md); progress
is logged in [LEDGER.md](LEDGER.md). Fonts: IBM Plex Sans and JetBrains
Mono, both under the SIL Open Font License (see `crates/kelp/assets/fonts`).

## License

MIT, see [LICENSE](LICENSE).
