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
mod repo_view;
mod settings;
mod sidebar;
mod staging;
mod theme;
mod updater;
mod widgets;
mod worktrees_view;

use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result {
    let paths = startup_paths();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Kelp")
            .with_icon(app_icon())
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Kelp",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
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

fn startup_paths() -> Vec<PathBuf> {
    let canonical = |p: PathBuf| std::fs::canonicalize(&p).unwrap_or(p);
    let args: Vec<PathBuf> = std::env::args()
        .skip(1)
        .map(PathBuf::from)
        .map(canonical)
        .collect();
    if !args.is_empty() {
        return args;
    }
    let saved = settings::Settings::load().open_tabs;
    if !saved.is_empty() {
        return saved.into_iter().filter(|p| p.exists()).collect();
    }
    std::env::current_dir()
        .ok()
        .filter(|dir| dir.parent().is_some() && gix::discover(dir).is_ok())
        .into_iter()
        .collect()
}
