use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui;

pub struct Jobs<T> {
    tx: Sender<(u64, T)>,
    rx: Receiver<(u64, T)>,
    running: Vec<(u64, String)>,
    next_id: u64,
    ctx: egui::Context,
}

impl<T: Send + 'static> Jobs<T> {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            running: Vec::new(),
            next_id: 0,
            ctx,
        }
    }

    pub fn spawn(&mut self, label: impl Into<String>, work: impl FnOnce() -> T + Send + 'static) {
        let id = self.next_id;
        self.next_id += 1;
        self.running.push((id, label.into()));
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send((id, work()));
            ctx.request_repaint();
        });
    }

    pub fn finished(&mut self) -> Vec<T> {
        let mut done = Vec::new();
        while let Ok((id, output)) = self.rx.try_recv() {
            self.running.retain(|(r, _)| *r != id);
            done.push(output);
        }
        done
    }

    pub fn running(&self) -> impl Iterator<Item = &str> {
        self.running.iter().map(|(_, label)| label.as_str())
    }

    pub fn is_running(&self, label: &str) -> bool {
        self.running.iter().any(|(_, l)| l == label)
    }
}
