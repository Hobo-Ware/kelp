use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, ColorImage, TextureHandle, TextureId, TextureOptions};
use gix::ObjectId;
use kelp_core::avatar::{self, GitHubRepo, Image, Resolver};

const WORKERS: usize = 4;

enum Slot {
    Pending,
    Ready(TextureHandle),
    Missing,
}

pub struct AvatarStore {
    requests: Option<Sender<(String, String)>>,
    results: Receiver<(String, Option<Image>)>,
    slots: HashMap<String, Slot>,
    ctx: egui::Context,
    pub enabled: bool,
}

impl AvatarStore {
    pub fn new(ctx: egui::Context, github: Option<GitHubRepo>) -> Self {
        let (result_tx, results) = mpsc::channel();
        if std::env::var_os("KELP_OFFLINE").is_some() {
            return Self {
                requests: None,
                results,
                slots: HashMap::new(),
                ctx,
                enabled: false,
            };
        }
        let (requests, request_rx) = mpsc::channel::<(String, String)>();
        let request_rx = Arc::new(Mutex::new(request_rx));
        let worker_ctx = ctx.clone();
        std::thread::spawn(move || {
            let resolver = Arc::new(Resolver::new(github, avatar::gh_token()));
            for _ in 0..WORKERS {
                let resolver = resolver.clone();
                let request_rx = request_rx.clone();
                let result_tx: Sender<(String, Option<Image>)> = result_tx.clone();
                let ctx = worker_ctx.clone();
                std::thread::spawn(move || {
                    loop {
                        let next = request_rx.lock().ok().and_then(|rx| rx.recv().ok());
                        let Some((email, commit)) = next else { break };
                        let image = resolver.resolve(&email, &commit);
                        if result_tx.send((email, image)).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                    }
                });
            }
        });
        Self {
            requests: Some(requests),
            results,
            slots: HashMap::new(),
            ctx,
            enabled: true,
        }
    }

    pub fn texture(&mut self, email: &str, commit: ObjectId) -> Option<TextureId> {
        if email.is_empty() || !self.enabled {
            return None;
        }
        let key = email.trim().to_lowercase();
        match self.slots.get(&key) {
            Some(Slot::Ready(texture)) => Some(texture.id()),
            Some(_) => None,
            None => {
                let Some(requests) = &self.requests else {
                    return None;
                };
                self.slots.insert(key.clone(), Slot::Pending);
                let _ = requests.send((key, commit.to_string()));
                None
            }
        }
    }

    pub fn poll(&mut self) {
        while let Ok((email, image)) = self.results.try_recv() {
            let slot = match image {
                Some(image) => {
                    let size = image.size as usize;
                    let color = ColorImage::from_rgba_unmultiplied([size, size], &image.rgba);
                    Slot::Ready(self.ctx.load_texture(
                        format!("avatar:{email}"),
                        color,
                        TextureOptions::LINEAR,
                    ))
                }
                None => Slot::Missing,
            };
            self.slots.insert(email, slot);
        }
    }
}
