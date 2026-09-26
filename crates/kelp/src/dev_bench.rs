use std::time::Duration;

use eframe::egui::{self, ViewportCommand};

const FRAMES: usize = 600;
const ROWS_PER_FRAME: usize = 37;
const SMOOTH_ROWS_PER_FRAME: usize = 2;

pub struct ScrollBench {
    samples: Vec<Duration>,
}

impl ScrollBench {
    pub fn from_env() -> Option<Self> {
        std::env::var_os("KELP_BENCH_SCROLL").map(|_| Self {
            samples: Vec::with_capacity(FRAMES * 2),
        })
    }

    pub fn next_row(&self, rows: usize) -> usize {
        let i = self.samples.len();
        if i < FRAMES {
            (i * ROWS_PER_FRAME * 7919) % rows.max(1)
        } else {
            (rows / 3 + (i - FRAMES) * SMOOTH_ROWS_PER_FRAME) % rows.max(1)
        }
    }

    pub fn record(&mut self, ctx: &egui::Context, frame_time: Duration) {
        if self.samples.len() >= FRAMES * 2 {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }
        self.samples.push(frame_time);
        if self.samples.len() < FRAMES * 2 {
            ctx.request_repaint();
            return;
        }
        report("random jumps ", &self.samples[..FRAMES]);
        report("smooth scroll", &self.samples[FRAMES..]);
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
}

fn report(label: &str, samples: &[Duration]) {
    let mut sorted = samples.to_vec();
    sorted.sort();
    let total: Duration = sorted.iter().sum();
    let pct = |p: f64| sorted[((sorted.len() - 1) as f64 * p) as usize];
    println!(
        "{label}: avg {:?}  p50 {:?}  p95 {:?}  max {:?}",
        total / sorted.len() as u32,
        pct(0.5),
        pct(0.95),
        sorted[sorted.len() - 1]
    );
}
