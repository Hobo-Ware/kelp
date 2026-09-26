use std::path::PathBuf;

use eframe::egui::{self, UserData, ViewportCommand};

const SETTLE_FRAMES: u32 = 3;

pub struct DevScreenshot {
    path: PathBuf,
    frames: u32,
    requested: bool,
}

impl DevScreenshot {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("KELP_SCREENSHOT")?;
        Some(Self {
            path: path.into(),
            frames: 0,
            requested: false,
        })
    }

    pub fn tick(&mut self, ctx: &egui::Context, ready: bool) {
        let images: Vec<_> = ctx.input(|i| {
            i.raw
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
                .collect()
        });
        if let Some(image) = images.first() {
            let [w, h] = image.size;
            let rgba: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            if let Err(e) = image::save_buffer(
                &self.path,
                &rgba,
                w as u32,
                h as u32,
                image::ColorType::Rgba8,
            ) {
                eprintln!("screenshot failed: {e}");
            }
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }
        if !ready || self.requested {
            return;
        }
        self.frames += 1;
        if self.frames > SETTLE_FRAMES {
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
            self.requested = true;
        }
        ctx.request_repaint();
    }
}
