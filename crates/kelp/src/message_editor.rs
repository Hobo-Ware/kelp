use eframe::egui::{self, Key, Margin, Modifiers, RichText, Stroke, vec2};

use crate::theme;

const SUMMARY_LIMIT: usize = 72;
const WIDTH: f32 = 560.0;

pub struct MessageEditor {
    pub commit: String,
    pub summary: String,
    pub body: String,
    original: String,
    is_head: bool,
    focused: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    None,
    Save(String),
    Cancel,
}

impl MessageEditor {
    pub fn new(commit: String, message: &str, is_head: bool) -> Self {
        let message = message.trim_end();
        let (summary, body) = message.split_once("\n\n").unwrap_or((message, ""));
        Self {
            commit,
            summary: summary.trim().to_string(),
            body: body.to_string(),
            original: message.to_string(),
            is_head,
            focused: false,
        }
    }

    pub fn message(&self) -> String {
        let summary = self.summary.trim();
        let body = self.body.trim_end();
        if body.trim().is_empty() {
            summary.to_string()
        } else {
            format!("{summary}\n\n{body}")
        }
    }

    fn can_save(&self) -> bool {
        !self.summary.trim().is_empty() && self.message() != self.original
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Event {
        let mut event = Event::None;
        let modal = egui::Modal::new(egui::Id::new("kelp-edit-message"))
            .backdrop_color(theme::backdrop())
            .frame(
                egui::Frame::new()
                    .fill(theme::popup())
                    .stroke(Stroke::new(1.0, theme::popup_border()))
                    .corner_radius(10)
                    .inner_margin(Margin::same(22)),
            )
            .show(ctx, |ui| {
                ui.set_width(WIDTH);
                ui.spacing_mut().item_spacing.y = 10.0;
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Edit commit message")
                            .size(16.0)
                            .family(theme::semibold())
                            .color(theme::text_strong()),
                    );
                    ui.label(
                        RichText::new(&self.commit[..7.min(self.commit.len())])
                            .monospace()
                            .color(theme::text_faint()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let len = self.summary.chars().count();
                        let color = if len > SUMMARY_LIMIT {
                            theme::modified()
                        } else {
                            theme::text_faint()
                        };
                        ui.label(
                            RichText::new(format!("{len}/{SUMMARY_LIMIT}"))
                                .size(11.0)
                                .color(color),
                        );
                    });
                });
                let summary = ui.add(
                    egui::TextEdit::singleline(&mut self.summary)
                        .id(egui::Id::new("kelp-edit-message-summary"))
                        .hint_text("Summary (required)")
                        .desired_width(f32::INFINITY)
                        .margin(Margin::symmetric(10, 8)),
                );
                if !self.focused {
                    summary.request_focus();
                    self.focused = true;
                }
                ui.add(
                    egui::TextEdit::multiline(&mut self.body)
                        .id(egui::Id::new("kelp-edit-message-body"))
                        .hint_text("Description")
                        .desired_rows(6)
                        .desired_width(f32::INFINITY)
                        .margin(Margin::symmetric(10, 8)),
                );
                let note = if self.is_head {
                    "Amends the latest commit's message. Staged changes stay staged."
                } else {
                    "Rewrites this commit and the ones after it. Cmd+Z undoes it."
                };
                ui.label(RichText::new(note).size(12.0).color(theme::text_faint()));
                let enter_in_summary =
                    summary.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                let command_enter = ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let save = ui.add_enabled(
                            self.can_save(),
                            egui::Button::new(
                                RichText::new("Save message")
                                    .size(12.0)
                                    .family(theme::semibold())
                                    .color(theme::on_accent()),
                            )
                            .fill(theme::accent())
                            .corner_radius(5)
                            .min_size(vec2(0.0, 30.0)),
                        );
                        if (save.clicked() || enter_in_summary || command_enter) && self.can_save()
                        {
                            event = Event::Save(self.message());
                        }
                        let cancel = egui::Button::new(RichText::new("Cancel").size(12.0))
                            .corner_radius(5)
                            .min_size(vec2(0.0, 30.0));
                        if ui.add(cancel).clicked() {
                            event = Event::Cancel;
                        }
                    });
                });
            });
        if modal.should_close() && event == Event::None {
            event = Event::Cancel;
        }
        event
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::{Event as Input, Key, Modifiers, RawInput};

    use super::*;

    fn frame(ctx: &egui::Context, editor: &mut MessageEditor, events: Vec<Input>) -> Event {
        let input = RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(900.0, 700.0),
            )),
            events,
            ..Default::default()
        };
        let mut out = Event::None;
        let _ = ctx.run_ui(input, |ui| out = editor.show(ui.ctx()));
        out
    }

    fn key(key: Key, modifiers: Modifiers) -> Vec<Input> {
        vec![
            Input::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            },
            Input::Key {
                key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers,
            },
        ]
    }

    fn editor() -> (egui::Context, MessageEditor) {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut editor = MessageEditor::new("0123456789".into(), "old title\n\nold body", false);
        frame(&ctx, &mut editor, vec![]);
        frame(&ctx, &mut editor, vec![]);
        (ctx, editor)
    }

    #[test]
    fn splits_and_joins_the_message() {
        let editor = MessageEditor::new("abc".into(), "title\n\nbody line\n", true);
        assert_eq!(
            (editor.summary.as_str(), editor.body.as_str()),
            ("title", "body line")
        );
        assert_eq!(editor.message(), "title\n\nbody line");
    }

    #[test]
    fn typing_then_enter_saves_the_new_message() {
        let (ctx, mut editor) = editor();
        frame(&ctx, &mut editor, vec![Input::Text(" v2".into())]);
        let saved = frame(&ctx, &mut editor, key(Key::Enter, Modifiers::NONE));
        assert_eq!(saved, Event::Save("old title v2\n\nold body".into()));
    }

    #[test]
    fn command_enter_saves_and_escape_cancels() {
        let (ctx, mut editor) = editor();
        frame(&ctx, &mut editor, vec![Input::Text("!".into())]);
        let saved = frame(&ctx, &mut editor, key(Key::Enter, Modifiers::COMMAND));
        assert_eq!(saved, Event::Save("old title!\n\nold body".into()));
        let (ctx, mut editor) = self::editor();
        let mut cancelled = frame(&ctx, &mut editor, key(Key::Escape, Modifiers::NONE));
        if cancelled == Event::None {
            cancelled = frame(&ctx, &mut editor, vec![]);
        }
        assert_eq!(cancelled, Event::Cancel);
    }

    #[test]
    fn unchanged_or_empty_messages_do_not_save() {
        let (ctx, mut editor) = editor();
        assert_eq!(
            frame(&ctx, &mut editor, key(Key::Enter, Modifiers::COMMAND)),
            Event::None
        );
        editor.summary.clear();
        assert!(!editor.can_save());
    }
}
