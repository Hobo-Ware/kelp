use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, UserData, ViewportCommand};

const SETTLE_FRAMES: u32 = 3;

pub struct DevScreenshot {
    path: PathBuf,
    frames: u32,
    requested: bool,
    wait: Duration,
    ready_at: Option<Instant>,
}

impl DevScreenshot {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("KELP_SCREENSHOT")?;
        Some(Self {
            path: path.into(),
            frames: 0,
            requested: false,
            wait: std::env::var("KELP_SCREENSHOT_WAIT")
                .ok()
                .and_then(|s| s.parse::<f32>().ok())
                .map_or(Duration::ZERO, Duration::from_secs_f32),
            ready_at: None,
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
        let ready_at = *self.ready_at.get_or_insert_with(Instant::now);
        if self.frames > SETTLE_FRAMES && ready_at.elapsed() >= self.wait {
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
            self.requested = true;
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}
