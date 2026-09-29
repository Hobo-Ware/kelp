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

enum Request {
    Author { email: String, commit: String },
    Owner(String),
}

#[derive(Default)]
struct Queue {
    items: Mutex<VecDeque<(String, Request)>>,
    ready: Condvar,
}

impl Queue {
    fn push_newest(&self, item: (String, Request)) -> Option<String> {
        let mut items = self.items.lock().ok()?;
        items.push_front(item);
        let dropped = (items.len() > MAX_PENDING)
            .then(|| items.pop_back())
            .flatten();
        self.ready.notify_one();
        dropped.map(|(key, _)| key)
    }

    fn pop_newest(&self) -> Option<(String, Request)> {
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
                    while let Some((key, request)) = queue.pop_newest() {
                        let image = match request {
                            Request::Author { email, commit } => resolver.resolve(&email, &commit),
                            Request::Owner(owner) => resolver.resolve_owner(&owner),
                        };
                        if result_tx.send((key, image)).is_err() {
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
        let email = email.trim().to_lowercase();
        let request = Request::Author {
            email: email.clone(),
            commit: commit.to_string(),
        };
        self.lookup(email, request)
    }

    pub fn owner_texture(&mut self, owner: &str) -> Option<TextureId> {
        if owner.is_empty() || !self.enabled {
            return None;
        }
        let owner = owner.to_lowercase();
        self.lookup(format!("owner:{owner}"), Request::Owner(owner))
    }

    fn lookup(&mut self, key: String, request: Request) -> Option<TextureId> {
        match self.slots.get(&key) {
            Some(Slot::Ready(texture)) => Some(texture.id()),
            Some(_) => None,
            None => {
                let queue = self.queue.as_ref()?;
                self.slots.insert(key.clone(), Slot::Pending);
                if let Some(dropped) = queue.push_newest((key, request)) {
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
