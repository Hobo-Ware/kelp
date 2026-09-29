use eframe::egui::{self, Ui};
use kelp_core::review::Review;

use crate::avatars::AvatarStore;
use crate::diff_view::{DiffView, Event, Layout};

const GAP: f32 = 14.0;

pub struct MultiDiff {
    pub views: Vec<DiffView>,
    layout: Layout,
}

impl MultiDiff {
    pub fn new(views: Vec<DiffView>) -> Self {
        let layout = views.first().map(DiffView::layout).unwrap_or_default();
        Self { views, layout }
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        review: &mut Review,
        author: &str,
        avatars: &mut AvatarStore,
    ) -> (Event, Option<String>) {
        let mut event = Event::None;
        let mut from = None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, view) in self.views.iter_mut().enumerate() {
                    ui.push_id(("stacked-diff", i), |ui| {
                        let e = view.stacked_ui(ui, review, author, avatars);
                        if !matches!(e, Event::None) {
                            event = e;
                            from = Some(view.path().to_string());
                        }
                    });
                    ui.add_space(GAP);
                }
            });
        if let Some(changed) = self
            .views
            .iter()
            .map(DiffView::layout)
            .find(|l| *l != self.layout)
        {
            self.layout = changed;
            for view in &mut self.views {
                view.set_layout(changed);
            }
        }
        (event, from)
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::{self, Event, Modifiers, Pos2, RawInput, Rect, epaint::Shape, vec2};
    use kelp_core::git_cli::run;
    use kelp_core::review::Review;

    use super::MultiDiff;
    use crate::diff_view::{DiffSource, DiffView};

    fn text_on_screen(output: &egui::FullOutput) -> String {
        output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Text(t) => Some(t.galley.text().to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn one_scroll_reaches_the_second_file() {
        let dir = std::env::temp_dir().join(format!("kelp-multi-diff-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.email", "t@example.com"],
            &["config", "user.name", "T"],
        ] {
            run(&dir, args).unwrap();
        }
        let body = |tag: &str| (0..300).map(|i| format!("{tag} {i}\n")).collect::<String>();
        for name in ["first.txt", "second.txt"] {
            std::fs::write(dir.join(name), "base\n").unwrap();
        }
        run(&dir, &["add", "."]).unwrap();
        run(&dir, &["commit", "-q", "-m", "base"]).unwrap();
        for name in ["first.txt", "second.txt"] {
            std::fs::write(dir.join(name), body(name)).unwrap();
        }
        let git = gix::open(&dir).unwrap();
        let views = ["first.txt", "second.txt"]
            .map(|p| {
                DiffView::load(&git, Some(&dir), DiffSource::Unstaged, p)
                    .unwrap()
                    .embedded()
            })
            .into_iter()
            .collect();
        let mut multi = MultiDiff::new(views);

        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut review = Review::default();
        let mut avatars = crate::avatars::AvatarStore::new(ctx.clone(), None);
        avatars.enabled = false;
        let mut frame = |events: Vec<Event>| {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 600.0))),
                events,
                modifiers: Modifiers::NONE,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                let _ = multi.ui(ui, &mut review, "T", &mut avatars);
            })
        };
        let first = text_on_screen(&frame(vec![Event::PointerMoved(Pos2::new(600.0, 300.0))]));
        assert!(first.contains("first.txt"));
        assert!(
            !first.contains("second.txt"),
            "second file is below the fold"
        );
        let wheel = |dy: f32| Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -dy),
            modifiers: Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        };
        let mut seen = String::new();
        for _ in 0..40 {
            seen = text_on_screen(&frame(vec![wheel(400.0)]));
            frame(vec![]);
            if seen.contains("second.txt") {
                break;
            }
        }
        assert!(
            seen.contains("second.txt"),
            "scrolling never reached the second file"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
