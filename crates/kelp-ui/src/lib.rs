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
//! - [`fonts`]: `install` registers the bundled fonts and the `semibold` family the widgets draw
//!   with. Call it once at startup.
//! - [`avatars::AvatarStore`]: background avatar loading, handed to the graph.
//! - [`jobs::Jobs`]: a small background job runner that wakes the UI when a job finishes.
//! - [`graph`]: [`graph::GraphView`] draws the commit graph from a `kelp_core::history::History`
//!   and a [`graph::GraphInput`], and returns a [`graph::GraphAction`].
//! - [`diff`]: [`diff::DiffBody`] draws a `kelp_core::diff::FileDiff` (unified or split, word
//!   highlights, jumping between changes, review thread cards and the comment composer) and
//!   returns [`diff::DiffAction`]s. Threads are passed in as `kelp_core::review::Thread` values,
//!   so the host decides where they are stored.
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
