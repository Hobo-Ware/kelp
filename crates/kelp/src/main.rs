mod app;
mod details;
mod dev_bench;
mod dev_screenshot;
mod graph_view;
mod sidebar;
mod theme;

use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Kelp")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Kelp",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(app::KelpApp::open(cc.egui_ctx.clone(), path)))
        }),
    )
}
