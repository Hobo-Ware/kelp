# Kelp - ledger

A running log of what got done, newest first. The plan lives in
[PLAN.md](PLAN.md).

## Status

- **Current milestone:** M0 (not started)
- **Next up:** set up the Cargo workspace and an empty egui window.

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
