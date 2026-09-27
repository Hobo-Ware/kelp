use kelp_core::ops::Op;
use kelp_core::undo::{self, Outcome, Record, lowercase_first};

use super::{JobOutput, Repo};

const UNDOING: &str = "Undoing";
const REDOING: &str = "Redoing";

impl Repo {
    pub fn undo_last(&mut self) {
        if self.jobs.is_running(UNDOING) || self.jobs.is_running(REDOING) {
            return;
        }
        let Some(record) = self.undo.take_undo() else {
            let text = match &self.undo.last_skipped {
                Some((label, reason)) => format!("{label} can't be undone: {reason}"),
                None => "Nothing to undo".into(),
            };
            self.notify(text, false);
            return;
        };
        let dir = self.dir.clone();
        self.jobs.spawn(UNDOING, move || {
            let result = undo::undo(&dir, &record);
            JobOutput::Undone {
                record: Box::new(record),
                result,
            }
        });
    }

    pub fn redo_last(&mut self) {
        if self.jobs.is_running(UNDOING) || self.jobs.is_running(REDOING) {
            return;
        }
        let Some(original) = self.undo.take_redo() else {
            self.notify("Nothing to redo", false);
            return;
        };
        let dir = self.dir.clone();
        self.jobs.spawn(REDOING, move || {
            let (result, outcome) = undo::run_recorded(&original.op, &dir);
            JobOutput::Redone {
                original: Box::new(original),
                result,
                outcome,
            }
        });
    }

    pub fn undo_hint(&self) -> String {
        let mut hint = match self.undo.next_undo() {
            Some(record) => format!("Undo {} (⌘ Z)", lowercase_first(&record.label)),
            None => "Nothing to undo".into(),
        };
        if let Some((label, reason)) = &self.undo.last_skipped {
            hint.push_str(&format!("\n{label} can't be undone: {reason}"));
        }
        if let Some(record) = self.undo.next_redo() {
            hint.push_str(&format!(
                "\nRedo {} (⌘ Shift Z)",
                lowercase_first(&record.label)
            ));
        }
        hint
    }

    pub fn can_undo(&self) -> bool {
        self.undo.next_undo().is_some() && !self.jobs.is_running(UNDOING)
    }

    pub(super) fn keep_undo(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Recorded(record) => {
                let evicted = self.undo.record(*record);
                self.forget(evicted);
            }
            Outcome::NotUndoable { label, reason } => self.undo.skip(label, reason),
            Outcome::Unchanged => {}
        }
    }

    pub(super) fn finish_undo(&mut self, record: Record, result: anyhow::Result<Vec<String>>) {
        match result {
            Ok(commands) => {
                if let Op::Commit { message, amend } = &record.op {
                    let (summary, body) = message.split_once("\n\n").unwrap_or((message, ""));
                    self.commit_summary = summary.to_string();
                    self.commit_body = body.to_string();
                    self.amend = *amend;
                }
                self.notify(
                    format!(
                        "Undid {}  ·  {}",
                        lowercase_first(&record.label),
                        short_shas(&commands.join("; "))
                    ),
                    false,
                );
                self.undo.undone(record);
            }
            Err(e) => {
                self.notify(format!("{e:#}"), true);
                self.undo.restore_undo(record);
            }
        }
        self.reload();
        self.refresh_status();
    }

    pub(super) fn finish_redo(
        &mut self,
        original: Record,
        result: anyhow::Result<String>,
        outcome: Outcome,
    ) {
        match result {
            Ok(_) => {
                if matches!(original.op, Op::Commit { .. }) {
                    self.commit_summary.clear();
                    self.commit_body.clear();
                    self.amend = false;
                }
                self.notify(format!("Redid {}", lowercase_first(&original.label)), false);
                if let Outcome::Recorded(record) = outcome {
                    self.undo.record_redone(*record);
                }
                self.forget(vec![original]);
            }
            Err(e) => {
                self.notify(format!("{e:#}"), true);
                self.undo.restore_redo(original);
            }
        }
        self.reload();
        self.refresh_status();
    }

    fn forget(&self, records: Vec<Record>) {
        if records.is_empty() {
            return;
        }
        let dir = self.dir.clone();
        std::thread::spawn(move || undo::forget(&dir, &records));
    }
}

fn short_shas(text: &str) -> String {
    text.split(' ')
        .map(|word| {
            if word.len() == 40 && word.bytes().all(|b| b.is_ascii_hexdigit()) {
                &word[..7]
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::short_shas;

    #[test]
    fn toast_shortens_object_ids() {
        let sha = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(
            short_shas(&format!("git update-ref refs/heads/main {sha}")),
            "git update-ref refs/heads/main 0123456"
        );
    }
}
