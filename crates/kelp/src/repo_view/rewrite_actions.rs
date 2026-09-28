use std::path::Path;

use eframe::egui;
use kelp_core::rebase::{self, Outcome as Stop};
use kelp_core::undo::{self, Outcome};

use super::{JobOutput, Repo};
use crate::message_editor::{Event, MessageEditor};

const REWRITING: &str = "Rewriting history";

impl Repo {
    pub fn open_message_editor(&mut self, commit: &str) {
        let Ok(id) = self.repo.rev_parse_single(commit).map(|id| id.detach()) else {
            self.notify(format!("Kelp could not find commit {commit}"), true);
            return;
        };
        let details = match kelp_core::commit::details(&self.repo, id) {
            Ok(details) => details,
            Err(e) => {
                self.notify(format!("{e:#}"), true);
                return;
            }
        };
        let message = if details.body.is_empty() {
            details.title.clone()
        } else {
            format!("{}\n\n{}", details.title, details.body)
        };
        let is_head = self.repo.head_id().is_ok_and(|head| head.detach() == id);
        self.message_editor = Some(MessageEditor::new(id.to_string(), &message, is_head));
    }

    pub(super) fn show_message_editor(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.message_editor else {
            return;
        };
        match editor.show(ctx) {
            Event::None => {}
            Event::Cancel => self.message_editor = None,
            Event::Save(message) => {
                let commit = editor.commit.clone();
                self.message_editor = None;
                let short = commit[..7.min(commit.len())].to_string();
                self.run_rewrite(format!("Edit message of {short}"), move |dir| {
                    rebase::edit_message(dir, &commit, &message)
                });
            }
        }
    }

    pub fn run_rewrite(
        &mut self,
        label: String,
        rewrite: impl FnOnce(&Path) -> anyhow::Result<Stop> + Send + 'static,
    ) {
        if self.jobs.is_running(REWRITING) {
            self.notify("Another history rewrite is still running", true);
            return;
        }
        let dir = self.dir.clone();
        self.jobs.spawn(REWRITING, move || {
            let before = undo::capture(&dir).ok();
            let result = rewrite(&dir);
            let undo = match (&result, before) {
                (Ok(Stop::Done), Some(before)) => undo::record_rewrite(&dir, label.clone(), before),
                (Ok(Stop::Done), None) => Outcome::NotUndoable {
                    label: label.clone(),
                    reason: "Kelp could not read the repository state around it",
                },
                (Ok(_), _) => Outcome::NotUndoable {
                    label: label.clone(),
                    reason: "it stopped before finishing; Abort in the banner goes back",
                },
                (Err(_), _) => Outcome::Unchanged,
            };
            JobOutput::Rewrite {
                label,
                result,
                undo,
            }
        });
    }

    pub(super) fn finish_rewrite(
        &mut self,
        label: String,
        result: anyhow::Result<Stop>,
        undo: Outcome,
    ) {
        self.keep_undo(undo);
        match result {
            Ok(Stop::Done) => self.notify(format!("{label} done  ·  Cmd+Z undoes it"), false),
            Ok(Stop::Editing(sha)) => self.notify(
                format!("Rebase paused at {sha} for editing: amend, then Continue"),
                false,
            ),
            Ok(Stop::Conflicts) => self.notify(
                "Rebase stopped on conflicts. Resolve them, then continue or abort the rebase.",
                true,
            ),
            Ok(Stop::Paused(reason)) => self.notify(format!("Rebase paused: {reason}"), true),
            Err(e) => self.notify(format!("{e:#}"), true),
        }
        self.reload();
        self.refresh_status();
    }
}
