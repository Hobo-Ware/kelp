mod actions;
mod app;
mod avatars;
mod blame_view;
mod checks_ui;
mod cli;
mod clone;
mod columns;
mod commands;
mod commit_helpers;
mod compare_view;
mod conflict_view;
mod console_view;
mod details;
mod dev_bench;
mod dev_screenshot;
mod dialogs;
mod diff_view;
mod file_history_view;
mod filter_bar;
mod focus_areas;
mod fonts;
mod graph_hover;
mod graph_rows;
mod graph_view;
mod help;
mod icons;
mod instance;
mod jobs;
mod macos;
mod mascot;
mod menus;
mod message_editor;
mod open_with;
mod palette;
mod panels;
mod preview_view;
mod pulls_ui;
mod pulls_view;
mod rebase_view;
mod recents;
mod ref_labels;
mod reflog_view;
mod repo_view;
mod settings;
mod sidebar;
mod signatures;
mod signing_panel;
mod staging;
mod stash_view;
mod theme;
mod updater;
mod welcome;
mod whats_new;
mod widgets;
mod window;
mod worktrees_view;

use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result {
    let args = match cli::start() {
        Ok(cli::Start::Here(paths)) => paths,
        Ok(cli::Start::Done) => return Ok(()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let saved = settings::Settings::load();
    let saved_theme = saved.theme_preference();
    let paths = startup_paths(args, &saved);
    let placement = window::restore(saved.window.filter(|_| !settings::is_dev_run()));
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Kelp")
        .with_icon(dock_icon())
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false)
        .with_inner_size(placement.size)
        .with_min_inner_size(window::MIN_SIZE);
    if let Some(position) = placement.position {
        viewport = viewport.with_position(position);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Kelp",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx, saved_theme);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            fonts::install(&cc.egui_ctx);
            Ok(Box::new(app::KelpApp::open(cc.egui_ctx.clone(), paths)))
        }),
    )
}

fn dock_icon() -> egui::IconData {
    let in_bundle = std::env::current_exe()
        .and_then(|exe| exe.canonicalize())
        .is_ok_and(|exe| cli::app_bundle(&exe).is_some());
    if in_bundle {
        egui::IconData::default()
    } else {
        app_icon()
    }
}

fn app_icon() -> egui::IconData {
    let image = image::load_from_memory(include_bytes!("../assets/icon-512.png"))
        .expect("bundled icon is a valid PNG")
        .into_rgba8();
    egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}

fn startup_paths(args: Vec<PathBuf>, saved: &settings::Settings) -> Vec<PathBuf> {
    if !args.is_empty() {
        return args;
    }
    if !saved.open_tabs.is_empty() {
        return saved
            .open_tabs
            .iter()
            .filter(|p| p.exists())
            .cloned()
            .collect();
    }
    std::env::current_dir()
        .ok()
        .filter(|dir| dir.parent().is_some() && gix::discover(dir).is_ok())
        .into_iter()
        .collect()
}
