use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Fetch,
    Pull,
    Push,
    Branch,
    Worktree,
    Stash,
    Pop,
    Check,
    Copy,
    Pencil,
    Trash,
    Merge,
    Rebase,
    Plus,
    Terminal,
    Folder,
    Minus,
    CherryPick,
    Revert,
    Reset,
    Undo,
    Eye,
    EyeOff,
    Pin,
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
        Icon::Check => line(&[(5.0, 12.5), (10.0, 17.5), (19.0, 7.0)]),
        Icon::Copy => {
            line(&[
                (9.0, 9.0),
                (20.0, 9.0),
                (20.0, 20.0),
                (9.0, 20.0),
                (9.0, 9.0),
            ]);
            line(&[
                (15.0, 9.0),
                (15.0, 4.0),
                (4.0, 4.0),
                (4.0, 15.0),
                (9.0, 15.0),
            ]);
        }
        Icon::Pencil => {
            line(&[
                (4.0, 20.0),
                (5.0, 15.0),
                (16.0, 4.0),
                (20.0, 8.0),
                (9.0, 19.0),
                (4.0, 20.0),
            ]);
            line(&[(13.5, 6.5), (17.5, 10.5)]);
        }
        Icon::Trash => {
            line(&[(4.0, 7.0), (20.0, 7.0)]);
            line(&[(9.0, 7.0), (9.0, 4.0), (15.0, 4.0), (15.0, 7.0)]);
            line(&[(6.0, 7.0), (7.0, 20.0), (17.0, 20.0), (18.0, 7.0)]);
            line(&[(10.0, 11.0), (10.0, 16.0)]);
            line(&[(14.0, 11.0), (14.0, 16.0)]);
        }
        Icon::Merge => {
            circle(6.0, 5.0, 2.0);
            circle(6.0, 19.0, 2.0);
            circle(18.0, 12.0, 2.0);
            line(&[(6.0, 7.0), (6.0, 17.0)]);
            line(&[(6.0, 7.0), (6.0, 9.0), (12.0, 12.0), (16.0, 12.0)]);
        }
        Icon::Rebase => {
            circle(6.0, 19.0, 2.0);
            circle(18.0, 5.0, 2.0);
            line(&[(6.0, 17.0), (6.0, 10.0), (15.0, 5.0), (16.0, 5.0)]);
            line(&[(12.0, 3.0), (15.5, 5.0), (12.0, 7.5)]);
        }
        Icon::Plus => {
            line(&[(12.0, 5.0), (12.0, 19.0)]);
            line(&[(5.0, 12.0), (19.0, 12.0)]);
        }
        Icon::Terminal => {
            line(&[(4.0, 6.0), (10.0, 12.0), (4.0, 18.0)]);
            line(&[(12.0, 19.0), (20.0, 19.0)]);
        }
        Icon::Folder => {
            line(&[
                (3.0, 6.0),
                (9.0, 6.0),
                (11.0, 8.0),
                (21.0, 8.0),
                (21.0, 19.0),
                (3.0, 19.0),
                (3.0, 6.0),
            ]);
        }
        Icon::Minus => line(&[(5.0, 12.0), (19.0, 12.0)]),
        Icon::CherryPick => {
            circle(7.0, 17.0, 3.0);
            circle(17.0, 18.0, 3.0);
            line(&[(8.0, 14.0), (12.5, 5.0), (16.5, 15.0)]);
            line(&[(12.5, 5.0), (18.0, 3.5)]);
        }
        Icon::Revert => {
            line(&[(9.0, 5.0), (5.0, 9.0), (9.0, 13.0)]);
            line(&[
                (5.0, 9.0),
                (14.0, 9.0),
                (17.5, 10.5),
                (19.0, 14.0),
                (17.5, 17.5),
                (14.0, 19.0),
                (8.0, 19.0),
            ]);
        }
        Icon::Reset => {
            circle(5.5, 12.0, 2.5);
            line(&[(20.0, 12.0), (10.0, 12.0)]);
            line(&[(14.0, 8.0), (10.0, 12.0), (14.0, 16.0)]);
        }
        Icon::Undo => {
            let turn: Vec<(f32, f32)> = (0..=12)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 12.0;
                    (14.0 + 5.0 * a.cos(), 13.5 + 5.0 * a.sin())
                })
                .collect();
            let path: Vec<(f32, f32)> = std::iter::once((5.0, 8.5))
                .chain(turn)
                .chain(std::iter::once((8.0, 18.5)))
                .collect();
            line(&path);
            line(&[(8.5, 5.0), (5.0, 8.5), (8.5, 12.0)]);
        }
        Icon::Eye | Icon::EyeOff => {
            let lid = |sign: f32| -> Vec<(f32, f32)> {
                (0..=12)
                    .map(|i| {
                        let t = i as f32 / 12.0;
                        (
                            3.0 + 18.0 * t,
                            12.0 + sign * 6.5 * (std::f32::consts::PI * t).sin(),
                        )
                    })
                    .collect()
            };
            line(&lid(-1.0));
            line(&lid(1.0));
            circle(12.0, 12.0, 2.8);
            if icon == Icon::EyeOff {
                line(&[(4.0, 20.0), (20.0, 4.0)]);
            }
        }
        Icon::Pin => {
            line(&[(9.0, 4.0), (15.0, 4.0)]);
            line(&[
                (10.0, 4.0),
                (10.0, 10.0),
                (7.0, 14.0),
                (17.0, 14.0),
                (14.0, 10.0),
                (14.0, 4.0),
            ]);
            line(&[(12.0, 14.0), (12.0, 21.0)]);
        }
    }
}

pub fn center_square(rect: Rect, size: f32) -> Rect {
    Rect::from_center_size(
        pos2(rect.center().x, rect.center().y),
        eframe::egui::vec2(size, size),
    )
}
