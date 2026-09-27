use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};

use eframe::egui::{self, ColorImage, TextureHandle, TextureId, TextureOptions};
use gix::ObjectId;
use kelp_core::avatar::{self, GitHubRepo, Image, Resolver};

const WORKERS: usize = 4;
const MAX_PENDING: usize = 48;

enum Slot {
    Pending,
    Ready(TextureHandle),
    Missing,
}

#[derive(Default)]
struct Queue {
    items: Mutex<VecDeque<(String, String)>>,
    ready: Condvar,
}

impl Queue {
    fn push_newest(&self, item: (String, String)) -> Option<String> {
        let mut items = self.items.lock().ok()?;
        items.push_front(item);
        let dropped = (items.len() > MAX_PENDING)
            .then(|| items.pop_back())
            .flatten();
        self.ready.notify_one();
        dropped.map(|(email, _)| email)
    }

    fn pop_newest(&self) -> Option<(String, String)> {
        let mut items = self.items.lock().ok()?;
        loop {
            if let Some(item) = items.pop_front() {
                return Some(item);
            }
            items = self.ready.wait(items).ok()?;
        }
    }
}

pub struct AvatarStore {
    queue: Option<Arc<Queue>>,
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
                queue: None,
                results,
                slots: HashMap::new(),
                ctx,
                enabled: false,
            };
        }
        let queue = Arc::new(Queue::default());
        let worker_queue = queue.clone();
        let worker_ctx = ctx.clone();
        std::thread::spawn(move || {
            let resolver = Arc::new(Resolver::new(github, avatar::gh_token()));
            for _ in 0..WORKERS {
                let resolver = resolver.clone();
                let queue = worker_queue.clone();
                let result_tx: Sender<(String, Option<Image>)> = result_tx.clone();
                let ctx = worker_ctx.clone();
                std::thread::spawn(move || {
                    while let Some((email, commit)) = queue.pop_newest() {
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
            queue: Some(queue),
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
                let queue = self.queue.as_ref()?;
                self.slots.insert(key.clone(), Slot::Pending);
                if let Some(dropped) = queue.push_newest((key, commit.to_string())) {
                    self.slots.remove(&dropped);
                }
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
