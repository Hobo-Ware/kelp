use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Color32, Sense, Stroke, Ui, vec2};
use gix::ObjectId;
use kelp_core::signing::{self, Signature};

use crate::theme;

pub struct Signatures {
    known: HashMap<ObjectId, Signature>,
    pending: HashSet<ObjectId>,
    tx: Sender<(ObjectId, Signature)>,
    rx: Receiver<(ObjectId, Signature)>,
}

impl Default for Signatures {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            known: HashMap::new(),
            pending: HashSet::new(),
            tx,
            rx,
        }
    }
}

impl Signatures {
    pub fn get(&mut self, ctx: &egui::Context, dir: &Path, id: ObjectId) -> Option<&Signature> {
        for (done, signature) in self.rx.try_iter() {
            self.pending.remove(&done);
            self.known.insert(done, signature);
        }
        if !self.known.contains_key(&id) && self.pending.insert(id) {
            let tx = self.tx.clone();
            let ctx = ctx.clone();
            let dir = dir.to_path_buf();
            std::thread::spawn(move || {
                let signature = signing::read(&dir, &id.to_string()).unwrap_or(Signature::Unsigned);
                if tx.send((id, signature)).is_ok() {
                    ctx.request_repaint();
                }
            });
        }
        self.known.get(&id)
    }
}

pub fn badge(ui: &mut Ui, signature: &Signature) {
    let Some(label) = signature.label() else {
        return;
    };
    let color = match signature {
        Signature::Verified { .. } => theme::added(),
        Signature::Expired { .. } => theme::modified(),
        Signature::Unverified { .. } => theme::deleted(),
        Signature::UnknownKey { .. } | Signature::Unsigned => theme::text_muted(),
    };
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_string(), egui::FontId::proportional(11.0), color);
    let (rect, response) =
        ui.allocate_exact_size(vec2(galley.size().x + 14.0, 18.0), Sense::hover());
    ui.painter().rect(
        rect,
        9.0,
        theme::with_alpha(color, 0x1e),
        Stroke::new(1.0, theme::with_alpha(color, 0x70)),
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(
        rect.center() - galley.size() / 2.0,
        galley,
        Color32::PLACEHOLDER,
    );
    if let Some(detail) = signature.detail() {
        response.on_hover_text(detail);
    }
}
