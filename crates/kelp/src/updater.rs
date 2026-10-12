use eframe::egui;
use kelp_core::update::{Release, Source};
use kelp_ui::update::{Status, UpdateConfig, Updater};

pub fn new(ctx: egui::Context) -> Updater {
    let config = UpdateConfig {
        source: Source::new("Hobo-Ware/kelp", "hobo-ware/tap/kelp")
            .with_user_agent("kelp-git-client"),
        app_name: "Kelp".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    };
    let mut updater = Updater::new(ctx, config);
    if let Ok(version) = std::env::var("KELP_FAKE_UPDATE") {
        updater.status = Status::Available(Release {
            version,
            url: String::new(),
        });
    }
    updater
}
