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

## From the terminal

```sh
kelp ~/path/to/repo        # opens in the background, the prompt comes right back
kelp .                     # already running? the folder opens as a tab there
kelp -w ~/path/to/repo     # stay in the foreground until Kelp quits
```

If the folder is already open, Kelp switches to its tab and comes to the
front. `kelp --help` lists the options.

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
  branch and tag labels (remote branches on GitHub show the owner's
  avatar instead of `origin/`). Drag the graph column edge to resize; scroll
  sideways for wide histories. Hover a commit to light up its path to
  the branch tip and see its full message, author, date and hash.
  Your uncommitted changes sit at the top of the graph, with a dashed
  line down to the commit you have checked out; other worktrees' changes
  sit above their own HEAD, with an Open button.
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
  Alt+Up / Alt+Down.
- **File history and blame:** every commit that touched a file, following
  renames, with its diff beside the list; blame shows who last changed each
  line (hover for the commit, click to find it in the graph, right-click to
  blame the version before). From any file menu, the diff header's clock and
  Blame mode, or the palette.
- **Previews:** images (PNG, JPEG, GIF, WebP, BMP, ICO, TIFF) and SVGs
  side by side before and after, and Markdown rendered with its images
  loaded from the repo.
- **Review:** hover a line in a diff, click **+**, and leave a comment.
  Threads support replies and resolving, and follow their line when code
  above them changes. Comments live in `.git/kelp/comments.json`, are
  never pushed, and can be copied as Markdown.
- **Branches:** check out, create, rename in place (F2, or Rename in any
  branch menu), delete (local and remote), rename on the remote (pushes
  the new name, deletes the old one, moves the upstream), merge, rebase,
  fetch, pull, push. Switching with local changes stashes them and puts
  them back on the new branch. Every action shows the git command it runs.
- **Tags and remotes:** create lightweight or annotated tags from any
  commit (the dialog can push the new tag to a remote), push one or all ("Push all tags" goes to the remote you last used for a tag; "Push all tags to…" asks, with several remotes),
  delete locally (the dialog can also delete it on a remote) or only on the remote. A push the
  remote refuses because it already has that tag on another commit asks before replacing it. Add, rename, repoint, fetch, prune and remove remotes from the Remote section.
- **Sidebar:** branches, remotes and tags grouped into folders by their
  `/` prefix (a folder with one branch folds into it), a filter box that
  also matches worktrees and stashes, pinned branches on top, "Hide
  merged branches" and sort by name or last commit from each section's
  `...` menu. Kept per repo in `.git/kelp/sidebar.json`.
- **Worktrees:** list, create (new or existing branch), move, remove,
  prune. Opening one (double-click, Enter or Open) switches the current tab
  to it, since it is the same repository; the right-click menu can open it
  in a new tab or a terminal instead. Clicking one in the sidebar scrolls
  the graph to it. A worktree with an AI agent running in it (Claude, Codex,
  Gemini, Copilot, Aider or Cursor) shows that tool's mark and a session
  count, the sidebar lists yours first, then the ones with an agent, then
  the ones with changes, and idle ones last, and the remove confirm mentions
  a running agent.
- **Staging:** unstaged and staged lists, stage or unstage files, single hunks or
  single lines (click line numbers, Shift-click for a range), discard files,
  hunks or lines (Cmd+Z brings them back), commit and amend (hooks run as usual).
  Cmd-click or Shift-click files to stage, stash or discard several at once.
- **Commit box helpers:** a Conventional Commit type picker (type, scope and a
  breaking-change toggle) that shows up when the recent history uses that style
  and can be turned on or off per repository; a co-author picker that adds
  `Co-authored-by` trailers as removable chips; and Commit and push
  (Cmd+Shift+Enter), which pushes the new commit the same way the Push button does.
- **Git console:** Cmd+Opt+L, the palette or the status bar button lists every git
  and gh command Kelp ran, newest first, with its exit status, duration and output
  (tokens and passwords are masked). Error toasts link to the failing command.
- **Stashes:** stash everything, chosen files or only the staged changes (with an
  optional message), pop, apply, drop, rename, and see a stash's changes file
  by file.
- **Signed commits:** a Verified, Unverified or unknown-key badge on each
  signed commit, with the signer and key on hover. Settings turns signing
  on with GPG or SSH for this repository or all of them; Kelp signs through
  git, so terminal commits sign the same way.
- **Submodules:** a Submodules section with each one's state (not
  initialized, clean, modified, new commits) and recorded commit, to
  initialize, update or open in a tab. Submodule changes in diffs show the
  old and new commit with their titles.
- **Git LFS:** LFS files show their size and oid, and the real content when
  the object is on disk, so images still preview.
- **Reflog:** every place HEAD or a branch pointed to, newest first,
  with commits that are on no branch any more marked lost. Restore the
  current branch to any entry (reset --keep, undoable), branch from it,
  check it out or cherry-pick it. Open it from the Local section menu, a
  branch menu or the palette. Stashes show their untracked files too.
- **Undo:** Cmd+Z undoes the last thing Kelp did (commit, amend, checkout,
  branch create/rename/delete, merge, rebase, pull, stash, discard,
  stage, interactive rebase, message edits), and Cmd+Shift+Z redoes it. Undo refuses rather than lose work
  when the repo changed since; pushes and fetches can't be undone.
- **Conflicts:** a stopped merge, rebase, cherry-pick or revert shows a
  banner with Continue, Skip and Abort. Each conflicted file opens side
  by side, ours and theirs, with Use ours / theirs / both per conflict,
  a preview of the result, and whole-file choices (including delete vs
  modify).
- **Interactive rebase:** right-click a commit, then reorder the commits after it by
  dragging, and pick, reword, edit, squash, fixup or drop each one. Messages are edited
  inline; no editor opens. Edit stops at that commit so you can amend it, then
  Continue from the banner. Ranges with merge commits keep their shape
  (`--rebase-merges`): merges stay put, the other commits can be picked, reworded,
  edited or dropped.
- **Edit any commit message:** the pencil in the details panel, the commit menu or
  the palette. The latest commit is amended (staged changes stay staged); older
  ones are reworded through a rebase. Cmd+Z undoes it.
- **Search:** Cmd+F across messages, authors, emails and hash prefixes.
- **Compare:** Cmd-click a second commit, or pick Compare with… in a
  commit or branch menu, to see every file that changed between them
  (A is the older one). Compare with the current branch or the working
  tree in one click; each file opens a diff across the range.
- **Filter:** Cmd+Shift+F or the funnel in the graph header narrows the
  graph by author (with suggestions), path, a time span or only your own
  commits. Other commits fade but keep their lanes; the status bar shows
  the match count and a Clear.
- **Command palette:** Cmd+K (or Cmd+Shift+P) finds any action, branch,
  commit, changed file or tab. Prefix with `>` for actions, `@` for
  branches, `#` for commits and `/` for files; Cmd+Enter on a branch lists
  its actions. Cmd+/ shows every shortcut.
- **Files:** right-click any file for Open in editor, Reveal in Finder and
  Copy path; the diff header has an editor button too. Pick the editor in
  Settings (empty uses the first installed of Cursor, VS Code, Zed and
  Sublime Text).
- **Pull requests:** on GitHub repos, branches with a pull request show a
  `#123` pill (green open, grey draft, purple merged, red closed, with a
  dot for checks) on graph labels, in the sidebar, the details panel and
  the status bar; click it to open the PR. Branch menus offer Open pull
  request or Create pull request. Uses `gh` when installed, otherwise the
  GitHub API; cached for five minutes and refreshed after fetch, pull and
  push. A Pull requests page (Remote header menu or the palette) lists
  open PRs with Open / Mine / Review requested tabs and a filter; each
  row can check out the PR (`gh`-free: fetches `refs/pull/N/head`), open
  it, or show its branch in the graph.
- **CI status:** on GitHub repos, commits on screen get a dot next to their
  time (green passed, red failed, amber running), fetched in batches of 50
  through `gh api graphql`, cached on disk and paused with backoff when
  GitHub rate-limits. Hover lists the failing and running checks; click
  opens them on GitHub.
- **Stays current:** changes made from the command line or another app
  show up on their own, and remotes are fetched in the background every
  5 minutes (change or turn off in Settings).
- **Start anywhere:** the new tab page lists recent repositories (type to
  filter) with Open folder, Clone (progress, cancel, opens when done) and
  New repository. The sidebar and details panels collapse, and the whole
  app zooms from 80% to 160%; sizes and zoom are remembered.
- **Moved or renamed folders:** a tab whose folder is gone offers the
  likely new place (a parent repository, or a sibling folder holding the
  same last commit) or Locate folder; recents and other tabs follow.
- **Light and dark:** a soft light theme and the dark one, built from the
  same colors. Settings > Appearance picks System (follows macOS as it
  changes), Light or Dark.
- **What's new and help:** after an update, Kelp shows the release notes
  once (up to three releases). Settings > Help and the palette open them
  again, along with the [docs](https://kelp.hoboware.dev/docs/getting-started.html)
  and a Report a bug link.

## Keys

| Key | Action |
|---|---|
| Cmd+K / Cmd+Shift+P | Command palette: actions, branches, commits, files and tabs |
| Cmd+/ | Keyboard shortcuts |
| F6 / Shift+F6 | Move focus to the next / previous area: sidebar, graph, details |
| Tab / Shift+Tab | Move focus to the next / previous control |
| Enter / Space | Activate the focused control; Enter on a branch checks it out |
| Shift+F10 | Open the menu of the focused row or commit |
| Up / Down, J / K | Move through commits |
| Up / Down, Home / End | Move through the files of a commit or of Changes; the diff follows |
| Up / Down, Home / End | Move through stashed files, the reflog, file history and the console |
| Enter | Show a file history commit in the graph, or expand a console entry |
| S / U | Stage / unstage the open or focused file in Changes |
| Cmd+F | Search; Enter / Shift+Enter for next / previous |
| Cmd+Shift+F | Filter commits by author, path or date; Esc closes |
| Cmd+Opt+F | Filter the sidebar; Esc clears |
| F2 | Rename the selected branch; Enter saves, Esc cancels |
| Esc | Back to the graph, close search or dialogs |
| Up / Down, Enter | Pick a pull request, worktree or recent repository; Enter opens it |
| O / T / B | Use ours, theirs or both for the current conflict |
| Enter | Confirm a dialog from its text field; destructive ones need their button |
| Cmd+T | New tab: recent repositories, open, clone or create one |
| Cmd+O | Open a folder in a new tab |
| Cmd+W | Close the tab |
| Cmd+Opt+S / Cmd+Opt+D | Show or hide the sidebar / details panel |
| Cmd+Plus / Cmd+Minus / Cmd+0 | Zoom in / out / reset |
| Cmd+1 ... Cmd+9 | Go to a tab (Cmd+9 is the last one) |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| Middle-click a tab | Close it |
| Drag a tab | Reorder tabs |
| Drop a folder on the window | Open it (a file opens the repository it is in) |
| Alt+Up / Alt+Down | Previous / next change in a diff, or conflict in the conflict view |
| Cmd+Enter | Save a review comment or reply |
| Cmd+Shift+Enter | Commit and push, from the commit box |
| Cmd+R | Refresh the graph, changes and worktrees |
| Cmd+Shift+R | Fetch |
| Cmd+Shift+L / Cmd+Shift+U | Pull / push |
| Cmd+Shift+B | New branch |
| Cmd+Shift+S | Stash all changes |
| Cmd+Shift+C | Show uncommitted changes |
| Cmd+Z / Cmd+Shift+Z | Undo / redo the last action |
| Cmd+, | Settings |
| Cmd+Opt+L | Git console: every command Kelp ran, with its output |
| Right-click | Actions for commits, branches, tags, stashes, worktrees and tabs |
| Double-click a branch | Check it out |
| Double-click a graph label | Check out that branch |
| Cmd-click a commit | Compare it with the selected commit; Esc stops comparing |
| Drag a graph label onto a commit | Merge, rebase or reset |
| Cmd-click / Shift-click a changed file | Pick several to stage, stash or discard |
| Cmd-click / Shift-click a file in a commit | Show several files' diffs stacked |
| P / R / E / S / F / D | In interactive rebase: pick, reword, edit, squash, fixup or drop the hovered commit |

## Where Kelp keeps things

| What | Where |
|---|---|
| Settings, open tabs, recent repositories, window size and position, panel sizes, zoom | `~/Library/Application Support/kelp/settings.json` |
| Avatar cache | `~/Library/Caches/kelp/avatars` |
| Review comments | `<repo>/.git/kelp/comments.json` |
| Whether the commit type picker is on | `<repo>/.git/kelp/commit.json` |

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
| `KELP_THEME=light` (or `dark`) | Use that theme for this run, whatever Settings says |
| `KELP_HIDE_REFS=a,b`, `KELP_COLUMNS=author,date,hash` | Hide branches or show columns for this run |
| `KELP_OPEN_DIFF=1` / `split` | Open the first changed file of the selected commit |
| `KELP_OPEN_WORKTREES=1`, `KELP_OPEN_DIALOG=worktree` (or `reset-hard`, `push-to`, `force-push`, `tag`, `delete-tag`, `replace-tag`, `add-remote`, `rename-remote-branch`, `clone`; `KELP_CLONE_URL=<url>` prefills it), `KELP_OPEN_WELCOME=1` (the new tab page; `KELP_RECENTS=<path>,<path>` fakes its list), `KELP_ZOOM=1.25`, `KELP_COLLAPSE=sidebar,details`, `KELP_OPEN_SETTINGS=1` (`help` scrolls to the Help links), `KELP_WHATS_NEW=<version>` (What's new since that version; empty shows the latest), `KELP_OPEN_PALETTE=<query>`, `KELP_OPEN_SHORTCUTS=1`, `KELP_OPEN_MESSAGE=1` (full commit message), `KELP_PICK=<file>,<file>` (picks unstaged files; add `KELP_OPEN_STASH=1` for the stash prompt), `KELP_SEARCH=text`, `KELP_OPEN_MENU=branch` (or `commit`, `tab`, `sort`), `KELP_SIDEBAR_FILTER=text`, `KELP_OPEN_CONFLICT=<file>` (add `#ours,theirs,both` to pre-pick, `:result` for the result), `KELP_OPEN_REFS=<commit>` (the `+N` ref list), `KELP_OPEN_DROP=<ref>@<commit>` (the drop menu), `KELP_RENAME=<branch>` (inline rename), `KELP_SHOW_STASH=stash@{0}` (a stash's changes), `KELP_OPEN_REFLOG=1` (or a branch name), `KELP_FILE_HISTORY=<path>`, `KELP_BLAME=<path>`, `KELP_OPEN_PULLS=1` (the pull requests page), `KELP_OPEN_CONSOLE=1` (the git console; `bg` also shows background checks) | Open a screen on start |
| `KELP_SELECT_COMMIT=<rev>`, `KELP_SELECT_WIP=1`, `KELP_OPEN_DIFF=path:<file>` / `paths:<a>,<b>` / `preview:<file>` / `unstaged:<file>` | Select a commit or file on start |
| `KELP_FOCUS=sidebar` (or `graph`, `details`) | Put keyboard focus in an area, with the focus ring showing |
| `KELP_COMPARE=<a>..<b>` (`<b>` can be `worktree`), `KELP_FILTER=author:<name>,path:<prefix>,period:day\|week\|month,mine` | Start comparing or filtering |
| `KELP_FAKE_UPDATE=<version>` | Pretend a newer release exists |
| `KELP_FAKE_PULLS=<file>` | Load pull requests from a `gh pr list --json` file instead of GitHub |
| `KELP_FAKE_AGENTS=<path>:<tool>,...` | Pretend those AI agent sessions are running (`claude`, `codex`, `gemini`, `copilot`, `aider`, `cursor`) |
| `KELP_FAKE_CHECKS=<file>` | CI dots from a JSON map of commit prefix to `{state, failing, pending}` instead of GitHub |
| `KELP_HOVER_ROW=<row>` | Draw the graph as if that commit row were hovered |
| `KELP_POINTER=<x>,<y>` | Keep the mouse pointer at that spot, in points, to capture hover states |
| `KELP_OPEN_REBASE=<rev>` (or `<rev>:<letters>`, e.g. `HEAD~4:prsd`) | Open interactive rebase from a commit, optionally with actions preset |
| `KELP_EDIT_MESSAGE=<rev>` | Open the message editor for a commit |

[CHANGELOG.md](CHANGELOG.md) is the one source for release notes: the app
embeds it for What's new, and `scripts/make-changelog-page.py` turns it
into `site/changelog.html`. `site/docs/shortcuts.html` is built from the
shortcut sheet with `KELP_WRITE_DOCS=1 cargo test -p kelp shortcuts_page`.
CI fails when either page is out of date, and `scripts/check-site.py`
checks the site's links, meta tags and sitemap.

The plan, design rules and decisions are in [PLAN.md](PLAN.md); progress
is logged in [LEDGER.md](LEDGER.md). Fonts: IBM Plex Sans and JetBrains
Mono, both under the SIL Open Font License (see `crates/kelp/assets/fonts`).

## License

MIT, see [LICENSE](LICENSE).
