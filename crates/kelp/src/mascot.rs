use std::f32::consts::PI;

use eframe::egui::{Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};

const BLADE: Color32 = Color32::from_rgb(0x8f, 0xd1, 0x6a);
const BLADE_LIGHT: Color32 = Color32::from_rgb(0xa4, 0xdf, 0x83);
const MIDRIB: Color32 = Color32::from_rgb(0x6f, 0xb3, 0x4f);
const STEM: Color32 = Color32::from_rgb(0x4d, 0x8a, 0x3a);
const ROOT: Color32 = Color32::from_rgb(0x3c, 0x6e, 0x2d);
const EYE: Color32 = Color32::from_rgb(0xf8, 0xfb, 0xf5);
const INK: Color32 = Color32::from_rgb(0x15, 0x18, 0x1e);
const CHEEK: Color32 = Color32::from_rgba_premultiplied(0x8c, 0x4f, 0x62, 0x8c);
const BUBBLE: Color32 = Color32::from_rgb(0x9f, 0xd4, 0xe8);
const BULB_LEFT: Color32 = Color32::from_rgb(0x2d, 0xd4, 0xbf);
const BULB_RIGHT: Color32 = Color32::from_rgb(0xfb, 0x92, 0x3c);

const RIBBON_STEPS: usize = 48;
const BLINK_EVERY: f32 = 3.6;
const BLINK_FOR: f32 = 0.14;

pub struct Mascot {
    origin: Pos2,
    scale: f32,
    time: f32,
    sway: f32,
}

impl Mascot {
    pub fn new(rect: Rect, time: f32, animated: bool) -> Self {
        let scale = rect.width().min(rect.height()) / 512.0;
        let origin = rect.center() - vec2(256.0, 256.0) * scale;
        Self {
            origin,
            scale,
            time: if animated { time } else { 0.9 },
            sway: if animated { 1.0 } else { 0.35 },
        }
    }

    fn p(&self, x: f32, y: f32) -> Pos2 {
        self.origin + vec2(x, y) * self.scale
    }

    fn stem_offset(&self, y: f32) -> f32 {
        let lift = ((470.0 - y) / 170.0).clamp(0.0, 2.0);
        self.sway * 7.0 * (self.time * 1.3).sin() * lift * lift
    }

    fn cubic(&self, a: Vec2, b: Vec2, c: Vec2, d: Vec2, steps: usize) -> Vec<Pos2> {
        (0..=steps)
            .map(|i| {
                let t = i as f32 / steps as f32;
                let mt = 1.0 - t;
                let v =
                    a * mt.powi(3) + b * 3.0 * mt * mt * t + c * 3.0 * mt * t * t + d * t.powi(3);
                self.p(v.x + self.stem_offset(v.y), v.y)
            })
            .collect()
    }

    fn stroke(&self, width: f32, color: Color32) -> Stroke {
        Stroke::new(width * self.scale, color)
    }

    fn blade_center(&self, t: f32) -> Vec2 {
        let y = 302.0 + (52.0 - 302.0) * t;
        let x = 256.0 + 36.0 * t * t + 10.0 * (t * PI).sin();
        let wave = self.sway * 9.0 * (self.time * 1.3 + t * 1.4).sin() * (0.35 + t);
        vec2(x + wave + self.stem_offset(302.0), y)
    }

    fn blade_half_width(t: f32) -> f32 {
        let width = (PI * (t * 1.05).min(1.0)).sin().max(0.0).powf(0.8) * 78.0 * (1.0 - 0.35 * t);
        let ruffle = 4.5 * (t * PI * 11.0).sin() * (PI * t).sin().powf(1.5);
        (width + ruffle).max(0.0)
    }

    pub fn paint(&self, painter: &Painter) {
        let v = |x: f32, y: f32| vec2(x, y);
        let stem = self.cubic(
            v(256.0, 470.0),
            v(250.0, 430.0),
            v(262.0, 400.0),
            v(256.0, 360.0),
            16,
        );
        let stem_top = self.cubic(
            v(256.0, 360.0),
            v(252.0, 332.0),
            v(258.0, 316.0),
            v(256.0, 302.0),
            10,
        );
        let bob = |phase: f32| self.sway * 5.0 * (self.time * 2.1 + phase).sin();
        let left_bulb = v(172.0, 346.0 + bob(0.0));
        let right_bulb = v(338.0, 292.0 + bob(1.7));
        let left_branch = self.cubic(
            v(254.0, 402.0),
            v(230.0, 394.0),
            v(200.0, 382.0),
            left_bulb + v(6.0, 8.0),
            12,
        );
        let right_branch = self.cubic(
            v(258.0, 350.0),
            v(284.0, 342.0),
            v(312.0, 326.0),
            right_bulb + v(-6.0, 8.0),
            12,
        );
        let root: Vec<Pos2> = [
            (230.0, 478.0),
            (240.0, 468.0),
            (250.0, 466.0),
            (256.0, 471.0),
            (262.0, 466.0),
            (274.0, 468.0),
            (284.0, 478.0),
        ]
        .iter()
        .map(|&(x, y)| self.p(x, y))
        .collect();

        painter.add(Shape::line(root, self.stroke(12.0, ROOT)));
        painter.add(Shape::line(left_branch, self.stroke(12.0, STEM)));
        painter.add(Shape::line(right_branch, self.stroke(12.0, STEM)));
        painter.add(Shape::line(stem, self.stroke(16.0, STEM)));
        painter.add(Shape::line(stem_top, self.stroke(16.0, STEM)));

        self.paint_blade(painter);
        self.paint_face(painter);
        for (center, color) in [(left_bulb, BULB_LEFT), (right_bulb, BULB_RIGHT)] {
            let c = self.p(center.x + self.stem_offset(center.y), center.y);
            painter.circle(c, 24.0 * self.scale, color, self.stroke(8.0, INK));
            painter.circle_filled(
                c + vec2(-7.0, -8.0) * self.scale,
                6.0 * self.scale,
                Color32::from_white_alpha(140),
            );
        }
        self.paint_bubbles(painter);
    }

    fn paint_blade(&self, painter: &Painter) {
        let mut left = Vec::with_capacity(RIBBON_STEPS + 1);
        let mut right = Vec::with_capacity(RIBBON_STEPS + 1);
        let mut middle = Vec::with_capacity(RIBBON_STEPS + 1);
        for i in 0..=RIBBON_STEPS {
            let t = i as f32 / RIBBON_STEPS as f32;
            let c = self.blade_center(t);
            let ahead = if t < 0.99 {
                self.blade_center(t + 0.01) - c
            } else {
                c - self.blade_center(t - 0.01)
            };
            let normal = vec2(-ahead.y, ahead.x).normalized();
            let w = Self::blade_half_width(t);
            left.push(self.p(c.x + normal.x * w, c.y + normal.y * w));
            right.push(self.p(c.x - normal.x * w, c.y - normal.y * w));
            middle.push(self.p(c.x, c.y));
        }
        painter.add(ribbon(&left, &right, BLADE));
        painter.add(ribbon(&middle, &right, BLADE_LIGHT));
        painter.add(Shape::line(left.clone(), Stroke::new(1.0, BLADE)));
        painter.add(Shape::line(right.clone(), Stroke::new(1.0, BLADE_LIGHT)));
        let rib: Vec<Pos2> = middle[..RIBBON_STEPS * 3 / 10].to_vec();
        painter.add(Shape::line(rib, self.stroke(5.0, MIDRIB)));
    }

    fn paint_face(&self, painter: &Painter) {
        let anchor = self.blade_center(0.5);
        let at = |dx: f32, dy: f32| self.p(anchor.x + dx, anchor.y + dy);
        let blink = (self.time % BLINK_EVERY) < BLINK_FOR && self.sway > 0.5;
        let eye_h = if blink { 2.5 } else { 19.0 };
        let look = self.sway * 1.5 * (self.time * 0.7).sin();
        for dx in [-27.0, 27.0] {
            painter.add(Shape::ellipse_filled(
                at(dx, -8.0),
                vec2(16.0, eye_h) * self.scale,
                EYE,
            ));
            if !blink {
                painter.circle_filled(at(dx + 3.0 + look, -4.0), 9.5 * self.scale, INK);
                painter.circle_filled(at(dx + 7.0 + look, -9.0), 3.5 * self.scale, Color32::WHITE);
            }
        }
        for dx in [-44.0, 44.0] {
            painter.add(Shape::ellipse_filled(
                at(dx, 22.0),
                vec2(10.0, 5.5) * self.scale,
                CHEEK,
            ));
        }
        let smile: Vec<Pos2> = (0..=10)
            .map(|i| {
                let t = i as f32 / 10.0;
                at(-12.0 + 24.0 * t, 26.0 + 7.0 * (t * PI).sin())
            })
            .collect();
        painter.add(Shape::line(smile, self.stroke(5.0, INK)));
    }

    fn paint_bubbles(&self, painter: &Painter) {
        for (i, (x, r)) in [(376.0, 9.0), (396.0, 6.0), (384.0, 4.0)]
            .into_iter()
            .enumerate()
        {
            let travel = if self.sway > 0.5 {
                (self.time * 28.0 + i as f32 * 46.0) % 140.0
            } else {
                i as f32 * 44.0
            };
            let y = 230.0 - travel;
            let fade = 1.0 - (travel / 140.0).powi(2);
            let wobble = self.sway * 3.0 * (self.time * 3.0 + i as f32).sin();
            let color = Color32::from_rgba_unmultiplied(
                BUBBLE.r(),
                BUBBLE.g(),
                BUBBLE.b(),
                (200.0 * fade) as u8,
            );
            painter.circle_stroke(
                self.p(x + wobble, y),
                r * self.scale,
                self.stroke(4.0, color),
            );
        }
    }
}

fn ribbon(a: &[Pos2], b: &[Pos2], color: Color32) -> Shape {
    let mut mesh = Mesh::default();
    for (pa, pb) in a.iter().zip(b) {
        mesh.colored_vertex(*pa, color);
        mesh.colored_vertex(*pb, color);
    }
    for i in 0..a.len().saturating_sub(1) as u32 {
        let k = i * 2;
        mesh.add_triangle(k, k + 1, k + 2);
        mesh.add_triangle(k + 1, k + 3, k + 2);
    }
    Shape::mesh(mesh)
}

pub fn paint(painter: &Painter, rect: Rect, time: f32, animated: bool) {
    Mascot::new(rect, time, animated).paint(painter);
}

pub fn bubbles(painter: &Painter, center: Pos2, time: f32, color: Color32) {
    for i in 0..3 {
        let phase = (time * 1.6 + i as f32 / 3.0) % 1.0;
        let y = center.y + 6.0 - phase * 12.0;
        let alpha = ((1.0 - phase) * 220.0) as u8;
        let c = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
        painter.circle_stroke(
            pos2(center.x - 4.0 + i as f32 * 4.0, y),
            1.5 + i as f32 * 0.5,
            Stroke::new(1.2, c),
        );
    }
}
