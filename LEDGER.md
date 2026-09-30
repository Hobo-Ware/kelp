# Kelp - ledger

A running log of what got done, newest first. The plan lives in
[PLAN.md](PLAN.md).

## Status

- **Current milestone:** v0.8.13 released. Left for v1.0: the 8 h soak rerun and the two-week bug bash. Signing is parked until there is an Apple Developer account (unsigned builds plus the cask quarantine strip, like stdusk).
- **Done:** M0 to M7, MIT license, polish pass, mascot, update check, release pipeline,
  website, agent rules.
- **Was queued:** GitHub Pages
  site like stdusk (SEO, OG images, upkeep instructions); Homebrew
  publishing like stdusk with auto-install on release and an in-app
  update check.

## 2026-09-30

- **Fix: the click that focuses the window no longer maximizes it.**
  `window_drag_area` ignores a title bar double-click unless the window
  has been focused for 0.4 s (`FocusGain`); dragging is unchanged. No
  new repaints.

- **v0.8.13: worktrees page matches the sidebar.** Agent marks in the
  name column (fixed slot, truncated names), rows ordered by
  `worktree_rank`. 456 tests pass.

- **v0.8.12: worktrees in the same tab.** `Command::OpenWorktree` fills
  `Repo::switch_to`; the app swaps the active tab (`plan_switch`: focus
  an existing tab, else replace the active one, else append). Sidebar
  order via `worktree_rank`: current, agent running, changes, idle.
  456 tests pass.

- **v0.8.11: agents in worktrees, click-to-reveal, hover fixes.**
  `kelp_core::agents` finds agent sessions (`ps`, then `lsof` cwd for
  new pids, cached) and assigns each to the deepest worktree; a scanner
  thread (`agent_watch`) runs every 10 s only while focused with two or
  more worktrees and repaints only on change (idle CPU 0.0% on
  trakt-workers). Brand marks are the Simple Icons SVGs stdusk uses.
  Hover path follows the commit's own lane at forks and WIP rows share
  the on/off-path strokes. 453 tests pass.

## 2026-09-29

- **v0.8.10: help, don't block.** `dialogs::recovery` maps a failed op to
  a follow-up confirm (worktree with changes, unmerged branch, rejected
  push); merge, rebase, interactive rebase and pull use `--autostash`.
  440 tests pass.

- **Moved or renamed repositories can be re-located** (PR #3 by
  tysonkerridge). A tab whose folder is gone says so and offers "Use
  <folder>…" (confirms with the full path) and "Locate folder…". The
  guess is the direct parent when it is now the repository (and holds
  the last HEAD, when known), or a sibling holding the commit HEAD was on
  when the repo was last opened (`Recent::tip`); recents and other dead
  tabs under the old path follow.

- **v0.8.8: update checker.** Latest version from the
  `github.com/<repo>/releases/latest` redirect instead of the rate-limited
  API; hourly checks, quiet 15 min retry on background failures, and an
  on-disk bundle version check (on every check and on focus) that goes
  straight to Restart. 433 tests pass.

- **v0.8.7: branch switching fixes.** `checkout_remote` only reuses the
  same-name local branch (a branch created from origin/main tracks it and
  used to win). `switch_or_open_worktree` opens the worktree that holds a
  branch instead of running a switch git refuses. 428 tests pass.

- **v0.8.6: lock retries, side-branch worktree rows.** `git_cli` retries
  lock failures (index.lock, "could not write index") with 50-800 ms
  waits and explains a lock that never frees. Other worktrees' WIP rows
  sit in the first free lane beside their head with a dashed curve into
  it (`side_lane`). 425 tests pass.

- **v0.8.5: label stacks, icon counts, square owner avatars.** Hovered or
  selected commits with several refs draw `ref_labels::paint_stack` over
  the rows below (sticky while the pointer is inside it).
  `change_counts` draws pencil / plus / minus counts for WIP rows and
  commit details. `avatar::Shape::RoundedSquare` for owner avatars.
  423 tests pass.

- **v0.8.4: text button hover.** One `widgets::text_button` for every
  frameless text button (hover and pressed backgrounds, pointer cursor,
  focus ring, AccessKit label). New dev switch `KELP_POINTER=x,y`.

- **v0.8.3: autostash, pinned WIP, owner badges.** Branch-switching ops
  stash local changes (untracked included), switch and pop with
  `--index`; a conflict keeps a named stash, and a switch that stashed
  nothing never pops an older stash. The current worktree's WIP row is
  pinned to display row 0 with a dashed gutter link into HEAD
  (`RowMap::wip_link`). Remote labels on GitHub show the owner avatar
  (`github.com/<owner>.png`, cached like author avatars). 421 tests pass.

- **v0.8.2: keyboard all around.** Shared `list_keys` helper (Up, Down,
  Home, End; a focused row keeps the arrows, otherwise the list whose
  diff is open takes them) used by commit files, Changes (plus S / U),
  stash, reflog, file history, console, pull requests, worktrees,
  compare and the welcome list. Conflicts: Alt+Up/Down, O / T / B.
  Enter only confirms safe dialogs. Remote checkout reuses the local
  branch and fast-forwards it (`Op::SwitchFastForward`). What's new
  bullets drawn directly so they line up. 415 tests pass.

- **v0.8.1: faded history fades as a unit.** Edges carry the row they
  come down from and fade with it; faded avatars get an opaque backing
  so lines don't show through. Signing parked until there is an Apple
  Developer account.

## 2026-09-28

- **v0.8.0: v1.0 docs and What's new.**
  - `CHANGELOG.md` for every release; `changelog.html` is built from it
    by `scripts/make-changelog-page.py` and CI fails if it is stale.
  - Site pages: getting started, shortcuts (generated from the shortcut
    sheet, test fails if stale), undo, FAQ. `scripts/check-site.py`
    checks links, anchors, meta tags, sitemap and dashes in CI.
  - What's new modal after an upgrade (up to 3 sections, fresh install
    shows nothing); Settings > Help and 7 palette actions.
  - Current worktree's uncommitted row: solid ring and strip in the
    branch color; other worktrees stay grey and dashed.
  - Site: dropped the fake title bar that doubled the window buttons.
  - 395 tests pass.

- **v0.7 "Trust and polish" (soak still running).**
  - Git console (Cmd+Opt+L): every git and gh command with args
    (secrets redacted), time, exit code and output; error toasts link to
    their entry.
  - Commit box: Conventional Commit type picker (auto when the history
    uses it), co-author picker, Commit and push (Cmd+Shift+Enter).
    History hints load in the background.
  - Keyboard: every action has a shortcut or palette entry (checklist
    test), no colliding or macOS-reserved shortcuts, F6 area focus,
    Shift+F10 menus, keyboard-only focus rings, AccessKit labels.
  - Light theme from one palette (System / Light / Dark, live), WCAG
    contrast tests, 21 scenes checked in both themes.
  - Scale on the Linux kernel logged above. Soak script `scripts/soak.sh`
    (first run stopped at 1.2 h with no crash report, 428 churn steps:
    RSS 119 to 138 MB with no growth, CPU 0.11% average; threads grow
    about one per opened tab because the script never closes tabs).
  - 382 tests pass. Idle CPU 0.0% in both themes.

- **Scale: the Linux kernel (1,484,088 commits, 702 lanes).** Lanes are
  stored as spans with a bucket index instead of one pass edge per lane
  per row (equivalence test against the old algorithm on random
  histories); the walk uses flat parent lists and reuses its id index.

| Linux kernel | Before | After |
|---|---|---|
| Load (walk + sort + layout) | 2.2 s | 1.4 s |
| Peak memory while loading (core only) | 2.37 GB | 435 MB |
| History held after load (core only) | - | 272 MB |
| App resident / physical footprint | - | ~600 MB / ~600 MB |
| Smooth scroll avg / p95 (background window) | - | 0.41 / 0.75 ms |
| Random jumps avg | - | 3.2 ms |

  For comparison the tidepool demo has a 346 MB footprint, 201 MB of it
  the window's graphics buffers, so the kernel history costs about
  250 MB on top. git/git is unchanged (0.33 ms smooth, 47 MB core).

- **v0.6 "Remotes and GitHub", remaining items in (two branches).**
  - CI dots on commits (green, red, amber) from `gh api graphql` for
    visible commits only, 50 per call, cached (1 h done, 1 min running,
    10 min none), backoff on 403/429. trakt-web: 100 commits in 3.4 s.
  - Pull requests page (Open / Mine / Review requested, filter): check
    out (switch or fetch `refs/pull/N/head`, fork-safe names), open,
    show in graph. trakt-web: 16 open PRs in 2.7 s.
  - Signature badges (Verified / Unverified / unknown or expired key),
    Commit signing settings (Off, GPG, SSH; repo or global), clearer
    errors when signing fails. trakt-web commits read `N`, no badge.
  - Submodules section (state, init/update, open, reveal) and readable
    submodule diffs (old -> new commit with titles).
  - Git LFS pointers shown as files, with the local object swapped in
    for diffs and previews when present.
  - 345 tests pass. Idle CPU 0.0% on trakt-web with network on, 146 MB.

- **Liquid Glass app icon (macOS 26).** A layered `AppIcon.icon`
  (stem, leaf, face, bladders over a dark fill, each glass with
  translucency) compiled by Xcode's actool into `Assets.car`, shipped
  next to `Kelp.icns` with `CFBundleIconName`. Inside the bundle Kelp
  no longer sets a runtime Dock icon (it pinned the flat image and
  blocked tinting). NSWorkspace renders Kelp tinted like Slack.
  `scripts/make-icon.sh` rebuilds both; `Assets.car` is committed since
  CI may not have Xcode 26.

- **v0.5 "History tools", all items in (three parallel branches, merged
  on main).**
  - File history (follows renames, streamed: trakt-web `deno.lock` 250
    commits, first rows in 42 ms, all in 161 ms) with the diff beside it.
  - Blame mode (avatar, hash, date per block, hover tooltip, click to
    reveal, Blame before this commit): 7,605 lines in 190 ms.
  - Compare any two commits (Cmd-click, Compare with..., with working
    tree), A/B marks, range diffs that follow renames.
  - Graph filters (author, path, time span, only mine) in the
    background; others fade and keep their lanes. Filter pass on git/git
    85,787 commits: author 0.43 s, path 1.42 s. Scroll unchanged.
  - Reflog page with lost-commit detection (0.2 s on git/git), Restore
    branch here (reset --keep, undoable), branch/checkout/cherry-pick.
  - Stash view lists untracked files (third parent).
  - Fixes: every menu caps its width; file history's embedded diff has
    no second X; the reflog refreshes after actions.
  - 316 tests pass.

| git/git scroll (background window) | Smooth avg / p95 | Jumps avg |
|---|---|---|
| No filter | 0.30 / 0.50 ms | 2.12 ms |
| Author + path filter | 0.31 / 0.52 ms | 1.55 ms |

- **v0.4 "Edit anything", all items in (three parallel branches, merged
  on main).**
  - Rename in place: F2 or Rename turns a sidebar branch into a text
    field with `check-ref-format` errors inline; remote branch rename
    (push new, delete old, move upstream) behind a confirm.
  - Edit message on any commit: HEAD amends with `--only` (index kept),
    older commits reword through the rebase engine, no editor.
  - Undo and redo cover interactive rebases and message edits (exact old
    tip restored, refused if the repo moved on); tags are tracked too.
  - Rebase view: Edit stops at a commit with a banner to amend and
    Continue; merge commits in range use `--rebase-merges` (merge rows
    fixed, no reorder or squash there).
  - Discard hunks or selected lines (reverse patch on the work tree,
    CRLF and missing EOF newline kept, undoable); staging multi-select
    (Cmd/Shift-click) to stage, stash or discard several files; stash
    one file, or staged changes only.
  - Stashes: rename, and a stash view with its diff. Worktrees: move.
    Tags: create (light or annotated), push, delete local and remote.
    Remotes: add, fetch, prune, rename, change URL, remove.
  - Diff header lays out its controls first; in a narrow center the
    folder, then the change arrows, then Split/Unified step aside.
  - 280 tests pass.

- **`kelp <dir>` from the terminal (issue #1).** From a terminal the
  command hands the folders to a running Kelp over a per-user Unix socket
  (`~/Library/Caches/kelp/instance.sock`, mode 0600) and exits; the app
  opens or focuses the tab and comes to the front. With no Kelp running
  it launches `Kelp.app` with `open -n -a` (or itself in a new process
  group from source) and returns. `-w/--wait`, non-terminal and dev runs
  stay in the foreground; `--help`, `--version`, unknown flags and
  missing folders are handled. Checked end to end through the real
  bundle via a Homebrew-style symlink: launch returns in 0.7 s, handoff
  in 10 ms, one process, no duplicate tab.

- **Pull requests inline, plus user-reported polish (after v0.3.0).**
  - PR pills (#N, colored by open/draft/merged/closed, CI dot) on graph
    labels, the `+N` list, sidebar rows, the details panel and the status
    bar; click opens the PR. Menus: Open pull request / Create pull
    request (gh, or the compare URL). Loaded via gh (all PRs with light
    fields, open PRs with checks) or the REST API, cached 5 min.
  - Real-repo test on trakt-web found two bugs the fixtures could not:
    GitHub timed out (502/504) on "all PRs with checks", and the app
    deadlocked reading gh output over 64 KB. Fixed; 202 PRs in ~4 s.
  - Labels shrink their name next to a pill instead of collapsing to `+N`.
  - Sidebar: Worktrees first; clicking a section title toggles it;
    Manage has a real hit area.
  - Selected avatar halo drawn after all rows (was clipped by its row).
  - Commit descriptions capped at ~8 lines with a Show full message
    viewer (Copy, Esc).
  - 227 tests pass.

- **v0.3 "Find anything", all items in (three parallel branches, merged
  on main).**
  - Command palette (Cmd+K / Cmd+Shift+P): fuzzy over 45+ registered
    actions, branches (Enter checks out, Cmd+Enter for branch actions),
    commits by hash or title (background search, cancelled on typing),
    files and tabs; `>` `@` `#` `/` modes; last 5 actions on top.
    Shortcut sheet (Cmd+/) generated from the same registry, and a test
    keeps the README Keys table identical to it.
  - Sidebar: branches in nested `/` folders (single children folded),
    remotes by remote then prefix, tags too; filter box (Cmd+Opt+F) with
    highlighted matches; pinned group; hide merged (background
    `git branch --merged`); sort by name or last commit; state in
    `.git/kelp/sidebar.json`.
  - New tab page: recent repos (20, deduped, branch read from HEAD),
    Open, Clone (progress, cancel, auth hint) and New repository; Cmd+T
    opens it. Collapsible sidebar and details (Cmd+Opt+S / Cmd+Opt+D),
    widths remembered. Zoom 80 to 160% (Cmd+Plus / Minus / 0) with the
    tab strip kept at native size so the window buttons stay centered.
  - Window geometry is saved in screen points so zoom doesn't shrink it.
    `Settings::save` is a no-op in dev runs.
  - 216 tests pass; idle CPU 0.0%, 125 MB.

- **v0.2 "A graph you can touch", all items in (three parallel branches
  plus the toolbar fix, merged on main).**
  - `+N` chip lists every ref on a commit, each with its full menu;
    labels: click selects, right-click menu, double-click checks out;
    drag a label onto a commit for Merge / Check out and merge / Rebase
    onto / Reset to here.
  - Hover: the first-parent path to the nearest ref is drawn brighter,
    other lanes dim; tooltip with title, body, author, date and hash.
  - Uncommitted rows for every worktree ("this worktree", then
    `<name> · N changed` with Open); the `+` and avatar initials are
    optically centered.
  - Hide or solo branches (sidebar eye, menus on labels and sidebar,
    status chip to undo), saved per repo in `.git/kelp/view.json`.
  - Optional Author, Date and Hash columns, resizable, saved in Settings.
  - Toolbar centered over the center column.
  - 171 tests pass; idle CPU 0.0%, 123 MB.

| git/git scroll (background window) | Smooth avg | Smooth p95 | Jumps avg |
|---|---|---|---|
| Default columns | 0.30 ms | 0.69 ms | 1.87 ms |
| All columns on | 0.34 ms | 0.74 ms | 1.97 ms |

## 2026-09-27

- **v0.1.2 feature batch (six parallel branches, merged and rebased on main).**
  - Tabs and window: Cmd+T/O/W, Cmd+1..9, Ctrl+Tab, Cmd+R; drag to
    reorder (tabs keyed by path); middle-click close; tab menu (close,
    close others, reveal, copy path); drop a folder or a file inside a
    repo to open it; window size and position remembered.
  - Git actions: cherry-pick, revert, reset soft/mixed/hard (hard asks,
    counting lost commits and files); push sets the upstream or asks for
    a remote, and offers force-with-lease when rejected; ahead/behind
    badges on Pull and Push.
  - Diffs: previous/next change (arrows, Alt+Up/Down), word-level
    highlights on paired lines, line-level staging (click or shift-click
    line numbers); Open in editor / Reveal in Finder on every file row and
    in the diff header, editor picked in Settings.
  - Undo/redo (Cmd+Z / Cmd+Shift+Z) for every local op, recorded
    generically around HEAD, refs, index, work tree and stashes with
    snapshots under refs/kelp/undo; refuses when the repo moved on; push,
    fetch and worktree ops are reported as not undoable. Interactive
    rebase is not tracked yet.
  - Interactive rebase view: reorder by drag, pick/reword/squash/fixup/
    drop, inline messages, runs without an editor; refuses dirty trees
    and merge commits in range.
  - Conflicts: banner with Continue/Skip/Abort for merge, rebase,
    cherry-pick and revert; side-by-side conflict view with ours/theirs/
    both per conflict, result preview, whole-file choices, delete vs
    modify.
  - 140 tests pass. Not exercised by hand (screenshot runs ignore input):
    real key presses, Finder drops, window restore, drags in the rebase
    view, the enabled Undo button.

- **Previews, tab strip alignment, split/unified fix (user feedback).**
  - Preview mode in the diff view: images (PNG, JPEG, GIF, WebP, BMP,
    ICO, TIFF) and SVG (resvg) side by side before/after on a
    checkerboard with dimensions and sizes, decoded off the UI thread;
    Markdown rendered with egui_commonmark. README-style HTML (centered
    logo `img` tags, wrapper `p`/`div`) is rewritten to Markdown, and
    relative images load from the same commit, index or work tree (SVGs
    rasterized at their `width`). Binary images open in Preview; SVG and
    Markdown open in Diff with Preview one click away, or in Preview when
    browsing a file.
  - Split/Unified: both header toggles shared interaction ids, so clicks
    could land on the other control (headless click test added, fails
    on the old ids). The chosen layout now carries over between files,
    and watcher reloads keep comment drafts.
  - Tab strip: wordmark removed, small leaf mark, and the mark, tabs and
    + all centered on the window-button line.
  - 67 tests pass.

- **Window, menus and controls pass (user feedback).**
  - Title bar folded into the tab strip: fullsize content view, hidden
    title, and an empty compact unified toolbar so AppKit centers the
    window buttons in a 40pt strip (measured: close button centered at
    20pt). Tab strip is 40pt, leaves 78pt for the buttons (none in
    fullscreen), tabs sit on its bottom edge, empty space drags the
    window and double-click zooms.
  - Dock icon: eframe replaces it with the egui logo unless the app
    passes one; Kelp now passes its own 512px icon.
  - Commit menu was as wide as the graph (it inherited the row area's
    width); menus now cap their width. The white menu the user saw came
    from an old bundle built before the dark-theme fix.
  - New painted segmented control (24px, inset active pill, hover state)
    for File/Diff, Split/Unified and Path/Tree, replacing tall framed
    buttons.
  - Diff header: an X on the right closes the preview (Esc still works);
    the "Graph" back button on the left is gone.
  - `KELP_OPEN_MENU=commit` dev switch for screenshots of the commit menu.

- **v0.1.0 published.** Public repo `Hobo-Ware/kelp`, tap deploy key and
  secret set, Pages on (workflow, `kelp.hoboware.dev`). Release run built
  the universal app and pushed `Casks/kelp.rb` to the tap. Installed on
  the dev Mac with `brew install --cask hobo-ware/tap/kelp`: app in
  `/Applications`, `kelp` on PATH, no quarantine flag, launches fine.
  DNS CNAME `kelp` -> `hobo-ware.github.io` still to add.

- **Stays current, calmer colors, better marketing shots.**
  - File watcher (`kelp-core/src/watch.rs`, FSEvents via `notify`):
    ref/HEAD changes reload the graph, index and work-tree edits refresh
    status. Bursts settle for 400 ms; git-ignored paths are skipped with a
    cached `git check-ignore`, so builds in `target/` wake nothing. Status
    runs with `--no-optional-locks` so our own `git status` does not
    trigger itself. Reloads that arrive mid-reload run once more after.
  - Auto-fetch: quiet `git fetch --all --prune` every 5 min (1/5/15/30 or
    off in Settings); a failure toasts once until the next success.
  - Verified: a CLI commit made while the app was open appeared on its
    own; 3 watcher round-trip tests (commit, edit, ignored files quiet).
    Idle CPU still 0.0%.
  - White checkboxes and toggles came from egui's light theme on a
    light-mode Mac. The app now forces dark and styles only that theme.
  - Softer palette: lifted background, lower text contrast, muted lane
    colors, tinted HEAD pill instead of a solid one, fainter row bands.
  - Marketing shots now use a generated demo repo
    (`scripts/make-demo-repo.py`, "tidepool", 36 commits, 6 authors)
    instead of git/git's wall of merges. PSNR 51-60 dB.
  - Screenshot runs drop keyboard input: typing elsewhere while one ran
    moved the selection with J/K.

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
