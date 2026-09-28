use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, OnceLock};

use eframe::egui::{
    self, Align2, Color32, ColorImage, CornerRadius, FontId, Rect, RichText, Stroke, TextureHandle,
    TextureOptions, Ui, pos2, vec2,
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use kelp_core::preview::{Kind, Sides};

use crate::theme;

const SVG_MAX_SIDE: f32 = 2048.0;
const SVG_PREVIEW_ZOOM: f32 = 4.0;
const PANE_GAP: f32 = 16.0;
const LABEL_H: f32 = 28.0;
const CHECKER: f32 = 10.0;
const MARKDOWN_W: f32 = 820.0;

enum Picture {
    Decoding(Receiver<Result<ColorImage, String>>),
    Ready(TextureHandle),
    Failed(String),
}

struct Pane {
    title: &'static str,
    bytes: Arc<[u8]>,
    picture: Picture,
}

struct Markdown {
    text: String,
    cache: CommonMarkCache,
    unregistered: Vec<(String, Vec<u8>)>,
}

pub struct Preview {
    kind: Kind,
    sources: Vec<Arc<[u8]>>,
    panes: Vec<Pane>,
    markdown: Option<Markdown>,
}

fn sides_to_show(sides: &Sides) -> Vec<(&'static str, Arc<[u8]>)> {
    match (&sides.old, &sides.new) {
        (Some(old), Some(new)) if old != new => {
            vec![("Before", old.clone()), ("After", new.clone())]
        }
        (_, Some(new)) => vec![("", new.clone())],
        (Some(old), None) => vec![("Deleted", old.clone())],
        (None, None) => Vec::new(),
    }
}

impl Preview {
    pub fn new(sides: &Sides, path: &str, read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Self {
        let pairs = sides_to_show(sides);
        let sources = pairs.iter().map(|(_, bytes)| bytes.clone()).collect();
        let markdown = (sides.kind == Kind::Markdown)
            .then(|| pairs.last())
            .flatten()
            .map(|(_, bytes)| Markdown::new(&String::from_utf8_lossy(bytes), path, read));
        let panes = if sides.kind == Kind::Markdown {
            Vec::new()
        } else {
            pairs
                .into_iter()
                .map(|(title, bytes)| Pane {
                    title,
                    picture: decode(sides.kind, bytes.clone()),
                    bytes,
                })
                .collect()
        };
        Self {
            kind: sides.kind,
            sources,
            panes,
            markdown,
        }
    }

    pub fn same_content(&self, sides: &Sides) -> bool {
        let fresh: Vec<Arc<[u8]>> = sides_to_show(sides).into_iter().map(|(_, b)| b).collect();
        self.kind == sides.kind && self.sources == fresh
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        if let Some(markdown) = &mut self.markdown {
            markdown.ui(ui);
            return;
        }
        if self.panes.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Nothing to preview.").color(theme::text_muted()))
            });
            return;
        }
        for pane in &mut self.panes {
            pane.poll(ui.ctx());
        }
        let area = ui.available_rect_before_wrap().shrink(PANE_GAP);
        let count = self.panes.len() as f32;
        let pane_w = (area.width() - PANE_GAP * (count - 1.0)) / count;
        for (i, pane) in self.panes.iter().enumerate() {
            let left = area.left() + i as f32 * (pane_w + PANE_GAP);
            let rect = Rect::from_min_size(pos2(left, area.top()), vec2(pane_w, area.height()));
            pane.paint(ui, rect, self.kind);
        }
        ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
    }
}

impl Pane {
    fn poll(&mut self, ctx: &egui::Context) {
        let Picture::Decoding(rx) = &self.picture else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(image)) => {
                let options = if image.size[0].max(image.size[1]) <= 64 {
                    TextureOptions::NEAREST
                } else {
                    TextureOptions::LINEAR
                };
                self.picture = Picture::Ready(ctx.load_texture("preview", image, options));
            }
            Ok(Err(e)) => self.picture = Picture::Failed(e),
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.picture = Picture::Failed("decoder stopped".into())
            }
        }
    }

    fn paint(&self, ui: &Ui, rect: Rect, kind: Kind) {
        let painter = ui.painter_at(rect);
        let label_rect = Rect::from_min_size(rect.min, vec2(rect.width(), LABEL_H));
        let mut label = String::from(self.title);
        if let Picture::Ready(texture) = &self.picture
            && kind == Kind::Image
        {
            let [w, h] = texture.size();
            if !label.is_empty() {
                label.push_str("  ·  ");
            }
            label.push_str(&format!("{w} × {h}"));
        }
        if !label.is_empty() {
            label.push_str("  ·  ");
        }
        label.push_str(&human_size(self.bytes.len()));
        painter.text(
            label_rect.left_center(),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(12.0),
            theme::text_muted(),
        );
        let stage = Rect::from_min_max(pos2(rect.left(), label_rect.bottom()), rect.max);
        painter.rect(
            stage,
            CornerRadius::same(8),
            theme::panel(),
            Stroke::new(1.0, theme::border()),
            egui::StrokeKind::Inside,
        );
        checkerboard(&painter, stage.shrink(1.0));
        match &self.picture {
            Picture::Decoding(_) => {
                painter.text(
                    stage.center(),
                    Align2::CENTER_CENTER,
                    "Loading…",
                    FontId::proportional(13.0),
                    theme::text_faint(),
                );
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(50));
            }
            Picture::Failed(e) => {
                painter.text(
                    stage.center(),
                    Align2::CENTER_CENTER,
                    format!("Could not show this image: {e}"),
                    FontId::proportional(13.0),
                    theme::text_muted(),
                );
            }
            Picture::Ready(texture) => {
                let ppp = ui.ctx().pixels_per_point();
                let natural = texture.size_vec2() / ppp;
                let room = stage.shrink(24.0).size();
                let fit = (room.x / natural.x).min(room.y / natural.y);
                let max_zoom = if natural.max_elem() <= 64.0 { 4.0 } else { 1.0 };
                let size = natural * fit.min(max_zoom);
                let image_rect = Rect::from_center_size(stage.center(), size);
                painter.image(
                    texture.id(),
                    image_rect,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
    }
}

fn checkerboard(painter: &egui::Painter, rect: Rect) {
    let cols = (rect.width() / CHECKER).ceil() as i32;
    let rows = (rect.height() / CHECKER).ceil() as i32;
    for row in 0..rows {
        for col in (row % 2..cols).step_by(2) {
            let min = rect.min + vec2(col as f32 * CHECKER, row as f32 * CHECKER);
            let cell = Rect::from_min_size(min, vec2(CHECKER, CHECKER)).intersect(rect);
            painter.rect_filled(cell, 0.0, theme::checker_light());
        }
    }
}

impl Markdown {
    fn new(source: &str, path: &str, read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Self {
        let prepared = kelp_core::markdown::prepare(source, path);
        let mut text = prepared.text;
        let mut unregistered = Vec::new();
        for asset in prepared.assets {
            let Some(bytes) = read(&asset.repo_path) else {
                continue;
            };
            let bytes = if Kind::of(&asset.repo_path) == Some(Kind::Svg) {
                match svg_to_png(&bytes, asset.width) {
                    Some(png) => png,
                    None => continue,
                }
            } else {
                bytes
            };
            let uri = format!(
                "bytes://kelp-markdown/{:016x}/{}",
                fingerprint(&bytes),
                asset.repo_path
            );
            text = text.replace(&format!("]({})", asset.url), &format!("]({uri})"));
            unregistered.push((uri, bytes));
        }
        Self {
            text,
            cache: CommonMarkCache::default(),
            unregistered,
        }
    }

    fn ui(&mut self, ui: &mut Ui) {
        for (uri, bytes) in self.unregistered.drain(..) {
            ui.ctx().include_bytes(uri, bytes);
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let width = (ui.available_width() - 48.0).min(MARKDOWN_W);
                let side = (ui.available_width() - width) / 2.0;
                ui.horizontal_top(|ui| {
                    ui.add_space(side);
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        ui.add_space(20.0);
                        markdown_style(ui);
                        CommonMarkViewer::new()
                            .max_image_width(Some(width as usize))
                            .show(ui, &mut self.cache, &self.text);
                        ui.add_space(40.0);
                    });
                });
            });
    }
}

fn markdown_style(ui: &mut Ui) {
    let style = ui.style_mut();
    style.text_styles.insert(
        egui::TextStyle::Heading,
        FontId::new(28.0, theme::semibold()),
    );
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(14.5));
    style.spacing.item_spacing.y = 8.0;
    style.visuals.override_text_color = None;
    style.visuals.widgets.noninteractive.fg_stroke.color = theme::text();
}

fn fingerprint(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

fn svg_to_png(bytes: &[u8], width: Option<u32>) -> Option<Vec<u8>> {
    let image = rasterize_svg(bytes, RasterSize::Width(width)).ok()?;
    let [w, h] = image.size;
    let rgba: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|p| p.to_srgba_unmultiplied())
        .collect();
    let mut png = Vec::new();
    image::RgbaImage::from_raw(w as u32, h as u32, rgba)?
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(png)
}

fn decode(kind: Kind, bytes: Arc<[u8]>) -> Picture {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = match kind {
            Kind::Svg => rasterize_svg(&bytes, RasterSize::Preview),
            _ => decode_bitmap(&bytes),
        };
        let _ = tx.send(result);
    });
    Picture::Decoding(rx)
}

fn decode_bitmap(bytes: &[u8]) -> Result<ColorImage, String> {
    let image = image::load_from_memory(bytes)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, image.as_raw()))
}

enum RasterSize {
    Preview,
    Width(Option<u32>),
}

fn rasterize_svg(bytes: &[u8], size: RasterSize) -> Result<ColorImage, String> {
    use resvg::{tiny_skia, usvg};
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    let fontdb = FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone();
    let options = usvg::Options {
        fontdb,
        ..Default::default()
    };
    let tree = usvg::Tree::from_data(bytes, &options).map_err(|e| e.to_string())?;
    let natural = tree.size();
    let scale = match size {
        RasterSize::Preview => {
            (SVG_MAX_SIDE / natural.width().max(natural.height())).min(SVG_PREVIEW_ZOOM)
        }
        RasterSize::Width(Some(width)) => width as f32 / natural.width(),
        RasterSize::Width(None) => 1.0,
    };
    let w = (natural.width() * scale).ceil().max(1.0) as u32;
    let h = (natural.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("SVG is too large to draw")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(ColorImage::from_rgba_premultiplied(
        [w as usize, h as usize],
        pixmap.data(),
    ))
}

fn human_size(bytes: usize) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.1} KB", b as f32 / 1024.0),
        b => format!("{:.1} MB", b as f32 / (1024.0 * 1024.0)),
    }
}
