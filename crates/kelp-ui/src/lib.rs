//! egui widgets from the Kelp git client, split out so other egui apps can embed the commit
//! graph and the diff view with the same look.
//!
//! Everything here is plain data in, actions out. The widgets never open a repository during
//! paint: the host loads history and diffs with `kelp-core`, passes them in, and applies the
//! actions the widgets return.
//!
//! # Supported API
//!
//! - [`theme`]: the palette (`p()`, `apply`, `sync`, `set_dark`) and color helpers (`lane`,
//!   `with_alpha`, `tone`, `semibold`). Call `theme::apply` once, then `theme::sync` each frame.
//! - [`fonts`]: `install` replaces the context's fonts with the bundled ones and the `semibold`
//!   family the widgets draw with; call it once at startup. A host with its own fonts calls
//!   `extend(&mut FontDefinitions)` instead, which adds the bundled fonts and `semibold` and keeps
//!   the host's families and their order. `extend_fallback(&mut FontDefinitions, text)` adds the
//!   system fallback font for scripts the bundled fonts lack, to the host's definitions, and the
//!   widgets' own fallback (`ensure_fallback`) adds to the context's current fonts rather than
//!   replacing them.
//! - [`avatars::AvatarStore`]: background avatar loading, handed to the graph.
//! - [`jobs::Jobs`]: a small background job runner that wakes the UI when a job finishes.
//! - [`graph`]: [`graph::GraphView`] draws the commit graph from a `kelp_core::history::History`
//!   and a [`graph::GraphInput`], and returns a [`graph::GraphAction`].
//! - [`diff`]: [`diff::DiffBody`] draws a `kelp_core::diff::FileDiff` (unified or split, word
//!   highlights, jumping between changes, review thread cards and the comment composer) and
//!   returns [`diff::DiffAction`]s. Threads are passed in as `kelp_core::review::Thread` values,
//!   so the host decides where they are stored. Thread cards and the composer are at most 640
//!   points wide and narrow with the pane. [`diff::DiffTexts`] (`DiffInput::texts`) holds the
//!   host-specific wording: where comments are saved and the shortcut hints; `Default` is Kelp's.
//! - [`paint`]: small drawing helpers (`draw_avatar`, `paint_avatar`, `selection_halo`,
//!   `commit_tooltip`, `truncated`).
//! - [`update`]: [`update::Updater`] checks GitHub releases and upgrades a Homebrew cask. The
//!   repository, cask and app name come from an [`update::UpdateConfig`].
//!
//! The other modules (`icons`, `widgets`, `menu`, `pulls_ui`) are shared with the Kelp app and
//! are not part of the supported API.

pub mod avatars;
pub mod diff;
pub mod fonts;
pub mod graph;
#[doc(hidden)]
pub mod icons;
pub mod jobs;
#[doc(hidden)]
pub mod menu;
pub mod paint;
#[doc(hidden)]
pub mod pulls_ui;
pub mod theme;
pub mod update;
#[doc(hidden)]
pub mod widgets;
