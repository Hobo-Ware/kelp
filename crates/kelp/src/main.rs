mod app;
mod avatars;
mod commands;
mod details;
mod dev_bench;
mod dev_screenshot;
mod dialogs;
mod diff_view;
mod fonts;
mod graph_view;
mod icons;
mod jobs;
mod macos;
mod mascot;
mod menus;
mod open_with;
mod preview_view;
mod repo_view;
mod settings;
mod sidebar;
mod staging;
mod theme;
mod updater;
mod widgets;
mod window;
mod worktrees_view;

use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result {
    let saved = settings::Settings::load();
    let paths = startup_paths(&saved);
    let placement = window::restore(saved.window.filter(|_| !settings::is_dev_run()));
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Kelp")
        .with_icon(app_icon())
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
            theme::apply(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            fonts::install(&cc.egui_ctx);
            Ok(Box::new(app::KelpApp::open(cc.egui_ctx.clone(), paths)))
        }),
    )
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

fn startup_paths(saved: &settings::Settings) -> Vec<PathBuf> {
    let canonical = |p: PathBuf| std::fs::canonicalize(&p).unwrap_or(p);
    let args: Vec<PathBuf> = std::env::args()
        .skip(1)
        .map(PathBuf::from)
        .map(canonical)
        .collect();
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
