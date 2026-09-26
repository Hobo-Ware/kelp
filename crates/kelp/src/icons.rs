use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2};

#[derive(Clone, Copy)]
pub enum Icon {
    Fetch,
    Pull,
    Push,
    Branch,
    Worktree,
    Stash,
    Pop,
}

pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let scale = rect.width().min(rect.height()) / 24.0;
    let origin = rect.center() - eframe::egui::vec2(12.0, 12.0) * scale;
    let p = |x: f32, y: f32| origin + eframe::egui::vec2(x, y) * scale;
    let stroke = Stroke::new(1.8 * scale.max(0.75), color);
    let line = |points: &[(f32, f32)]| {
        let pts: Vec<Pos2> = points.iter().map(|&(x, y)| p(x, y)).collect();
        painter.add(Shape::line(pts, stroke));
    };
    let circle = |x: f32, y: f32, r: f32| {
        painter.circle_stroke(p(x, y), r * scale, stroke);
    };
    let tray = || {
        line(&[
            (4.0, 9.0),
            (20.0, 9.0),
            (20.0, 20.0),
            (4.0, 20.0),
            (4.0, 9.0),
        ])
    };
    let lid = || line(&[(6.0, 9.0), (8.0, 4.0), (16.0, 4.0), (18.0, 9.0)]);
    match icon {
        Icon::Fetch => {
            line(&[(12.0, 3.0), (12.0, 15.0)]);
            line(&[(7.0, 10.0), (12.0, 15.0), (17.0, 10.0)]);
            line(&[(5.0, 21.0), (19.0, 21.0)]);
        }
        Icon::Pull => {
            line(&[(12.0, 3.0), (12.0, 13.0)]);
            line(&[(8.0, 9.0), (12.0, 13.0), (16.0, 9.0)]);
            line(&[(4.0, 15.0), (4.0, 20.0), (20.0, 20.0), (20.0, 15.0)]);
        }
        Icon::Push => {
            line(&[(12.0, 21.0), (12.0, 8.0)]);
            line(&[(7.0, 13.0), (12.0, 8.0), (17.0, 13.0)]);
            line(&[(5.0, 3.0), (19.0, 3.0)]);
        }
        Icon::Branch => {
            circle(6.0, 5.0, 2.0);
            circle(6.0, 19.0, 2.0);
            circle(18.0, 7.0, 2.0);
            line(&[(6.0, 7.0), (6.0, 17.0)]);
            let curve: Vec<(f32, f32)> = (0..=12)
                .map(|i| {
                    let t = i as f32 / 12.0;
                    let mt = 1.0 - t;
                    let x = mt.powi(3) * 18.0
                        + 3.0 * mt * mt * t * 18.0
                        + 3.0 * mt * t * t * 6.0
                        + t.powi(3) * 6.0;
                    let y = mt.powi(3) * 9.0
                        + 3.0 * mt * mt * t * 14.0
                        + 3.0 * mt * t * t * 12.0
                        + t.powi(3) * 17.0;
                    (x, y)
                })
                .collect();
            line(&curve);
        }
        Icon::Worktree => {
            line(&[
                (3.0, 6.0),
                (9.0, 6.0),
                (11.0, 8.0),
                (21.0, 8.0),
                (21.0, 19.0),
                (3.0, 19.0),
                (3.0, 6.0),
            ]);
            line(&[(12.0, 11.0), (12.0, 16.0)]);
            line(&[(9.5, 13.5), (14.5, 13.5)]);
        }
        Icon::Stash => {
            tray();
            lid();
            line(&[(12.0, 12.0), (12.0, 17.0)]);
            line(&[(9.5, 14.5), (12.0, 17.0), (14.5, 14.5)]);
        }
        Icon::Pop => {
            tray();
            lid();
            line(&[(12.0, 17.0), (12.0, 12.0)]);
            line(&[(9.5, 14.5), (12.0, 12.0), (14.5, 14.5)]);
        }
    }
}

pub fn center_square(rect: Rect, size: f32) -> Rect {
    Rect::from_center_size(
        pos2(rect.center().x, rect.center().y),
        eframe::egui::vec2(size, size),
    )
}
