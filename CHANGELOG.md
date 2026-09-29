# Changelog

Everything that changed in Kelp, newest first.

## v0.8.7 - 2026-09-29

- Double-clicking a remote branch like origin/main checks out your local main, not some other branch that happens to track origin/main.
- Switching to a branch that is checked out in another worktree opens that worktree instead of showing an error.

## v0.8.6 - 2026-09-29

- Switching branches no longer fails with "could not write index" when another git process is busy in the same repository: Kelp waits a moment and tries again.
- Another worktree's uncommitted changes branch off the commit they're based on, instead of sitting on the line like a commit.

## v0.8.5 - 2026-09-29

- A commit with several branches or tags stacks its labels when you hover or select it, so you can reach each one.
- Change counts show as icons with numbers (pencil, plus, minus) instead of words.
- GitHub org and owner avatars keep their square shape with rounded corners instead of being cropped to a circle.

## v0.8.4 - 2026-09-29

- Text buttons like Stash, Stage all and Unstage all light up when you hover them, so it's clear they can be clicked.

## v0.8.3 - 2026-09-29

- Switching branches with local changes no longer fails: Kelp stashes them, switches and puts them back. If they clash with the new branch, a copy stays in a stash named after the switch.
- Your uncommitted changes always sit at the top of the graph, with a dashed line down to the commit you have checked out.
- Remote branches on GitHub show the owner's avatar instead of "origin/".

## v0.8.2 - 2026-09-29

- Arrow keys work in every list: the files of a commit, Changes, stashes, the reflog, file history, the git console, pull requests, worktrees, comparisons and the recent repositories on a new tab. Home and End jump to the first and last row, and where a list opens a diff, the diff follows.
- In Changes, S stages and U unstages the open or focused file.
- In a conflict, Alt+Up and Alt+Down jump between conflicts, and O, T or B picks ours, theirs or both.
- Enter no longer confirms dangerous dialogs like deleting a branch or a hard reset; those need a click on their button.
- Double-clicking a remote branch such as origin/main now checks out your local main and fast-forwards it, instead of failing because main already exists.
- The bullets in What's new line up with their text.

## v0.8.1 - 2026-09-29

- With fading outside the selected history turned on, a faded branch now fades as a whole: its lines fade with its commits, and lines no longer show through faded avatars.

## v0.8.0 - 2026-09-28

- After an update, Kelp shows what's new since the version you last ran.
- New help pages on the site: getting started, shortcuts, how undo works, and a FAQ. Settings and the palette link to them, and to a changelog page.
- The uncommitted changes of the worktree you're in now stand out from other worktrees: a solid circle in the branch color instead of a grey dashed one.

## v0.7.0 - 2026-09-28

- A light theme. Kelp follows your Mac's appearance by default, and Settings lets you pick Light or Dark instead.
- A git console (Cmd+Opt+L) lists every git command Kelp ran, with its output, how long it took and whether it worked. Error messages link straight to the command that failed.
- The commit box can add a Conventional Commit type (feat, fix, docs and more), add co-authors from people in your history, and commit and push in one go (Cmd+Shift+Enter).
- Everything works from the keyboard: every action has a shortcut or a palette entry, F6 moves between the sidebar, graph and details, and Shift+F10 opens the menu for the focused row.
- VoiceOver can read branches, commits and buttons.
- Huge repositories open much faster and use far less memory. The Linux kernel, with almost 1.5 million commits, opens in about a second and a half.
- The rebase warning no longer runs under the buttons.

## v0.6.0 - 2026-09-28

- Commits show a small dot for their CI status: green passed, red failed, amber still running. Hover to see which checks failed.
- A Pull requests page lists open pull requests, yours, and the ones waiting for your review. Check one out, open it, or find its branch in the graph.
- Signed commits show a Verified badge, and Settings can set up commit signing with GPG or SSH.
- Submodules get their own sidebar section, and a changed submodule shows which commits it moved between.
- Git LFS files show as LFS files instead of pointer text, and images preview when the file is on your Mac.

## v0.5.1 - 2026-09-28

- The app icon turns to glass and tints with your Dock on macOS 26.

## v0.5.0 - 2026-09-28

- File history: every commit that touched a file, following renames, with its changes beside the list.
- Blame shows who last changed each line. Click a line to jump to that commit, or blame the version before it.
- Compare any two commits: Cmd-click a second one, or compare with your current branch or your working files.
- Filter the graph by author, file path, time span or only your own commits. Everything else fades.
- The reflog page is a safety net for commits that look lost, with a one-click restore you can undo.
- Stashes show the untracked files they hold.
- Menus keep a sensible width instead of stretching.

## v0.4.0 - 2026-09-28

- Rename a branch right in the sidebar with F2. Renaming a branch on the remote is one confirm away.
- Edit the message of any commit, not just the last one.
- Undo covers interactive rebases and message edits too.
- The interactive rebase can stop at a commit so you can change it, and it handles merge commits.
- Discard a single hunk or just the lines you picked, and bring them back with Cmd+Z.
- Pick several files in the staging panel to stage, stash or discard them together, or stash just one file.
- Rename stashes and see their changes, move worktrees, create and push tags, and manage remotes.
- Typing `kelp <folder>` in the terminal opens it in the running Kelp and gives your prompt back right away.

## v0.3.1 - 2026-09-28

- Branches show their pull request number and state, with a dot for CI. Click it to open the pull request.
- Open or create a pull request from any branch menu.
- Worktrees come first in the sidebar, and clicking a section title opens or closes it.
- Long commit descriptions fold after a few lines, with a Show full message viewer.
- The halo around the selected avatar is a full circle again.

## v0.3.0 - 2026-09-28

- A command palette (Cmd+K) finds any action, branch, commit, file or tab.
- A shortcut sheet (Cmd+/) lists every shortcut.
- Branches fold into folders like feat/ and fix/, with a filter box, pinned branches, a way to hide merged ones, and sorting by name or date.
- A new tab page with your recent repositories, plus Clone and New repository.
- Collapse the sidebar and details panel, and zoom the whole window.

## v0.2.0 - 2026-09-28

- Click the +N chip on a commit to see every branch and tag on it.
- Click, right-click or double-click branch labels in the graph, and drag one onto a commit to merge, rebase or reset.
- Hover a commit to trace its branch and see its details.
- Uncommitted changes from your other worktrees show in the graph, each with an Open button.
- Hide a branch from the graph, or show only one.
- Add author, date and hash columns next to the commit message.

## v0.1.2 - 2026-09-27

- Undo (Cmd+Z) for commits, checkouts, resets, merges, discards and more.
- An interactive rebase view: reorder, reword, squash, fixup or drop commits without an editor.
- A conflict view with ours and theirs side by side, and Continue or Abort in one place.
- Cherry-pick, revert and reset from the commit menu. Pushes set their upstream and offer a safe force push.
- Word-level highlights, jump between changes, and stage single lines.
- Open files in your editor or reveal them in Finder.
- Tab shortcuts, drag to reorder tabs, drop a folder onto the window, and Kelp remembers its window size.

## v0.1.1 - 2026-09-27

- The window buttons sit in the tab strip, and the Dock shows the Kelp icon.
- Previews for images, SVGs and Markdown.
- Split and Unified diffs switch reliably and remember your choice.

## v0.1.0 - 2026-09-27

- The first release: a fast commit graph with avatars, branches, worktrees, stashes, diffs, inline review comments, staging and committing.
- Kelp updates itself through Homebrew, refreshes when files change, and fetches in the background.
