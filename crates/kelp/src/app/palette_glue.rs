use eframe::egui;

use super::{KelpApp, State, Tab};
use crate::actions::{AppAction, Run};
use crate::columns::Column;
use crate::palette::{self, Pick, Sources};
use crate::panels::{self, Side};
use crate::{settings, welcome};

impl KelpApp {
    pub(super) fn palette_frame(&mut self, ctx: &egui::Context) {
        palette::shortcut_sheet(ctx, &mut self.shortcuts_open);
        if !self.palette.open {
            return;
        }
        let tabs: Vec<String> = self.tabs.iter().map(Tab::title).collect();
        let repo = match self.tabs.get(self.active).map(|t| &t.state) {
            Some(State::Ready(repo)) => Some(repo.as_ref()),
            _ => None,
        };
        let mut state = repo.map(|r| r.action_state()).unwrap_or_default();
        state.tabs = self.tabs.len();
        let sources = Sources {
            state,
            branches: repo.map(|r| r.palette_branches()).unwrap_or_default(),
            files: repo.map(|r| r.palette_files()).unwrap_or_default(),
            tabs,
            active_tab: self.active,
            history: repo.map(|r| r.palette_history()),
        };
        if let Some(pick) = self.palette.ui(ctx, &sources) {
            self.run_pick(ctx, pick);
        }
    }

    fn run_pick(&mut self, ctx: &egui::Context, pick: Pick) {
        match pick {
            Pick::Run(Run::App(action)) => {
                self.remember_palette();
                self.run_app_action(action);
                return;
            }
            Pick::Tab(i) => {
                self.active = i.min(self.tabs.len().saturating_sub(1));
                return;
            }
            _ => {}
        }
        let Some(Tab {
            state: State::Ready(repo),
            ..
        }) = self.tabs.get_mut(self.active)
        else {
            return;
        };
        match pick {
            Pick::Run(Run::Repo(action)) => repo.run_action(ctx, action),
            Pick::Checkout(branch) => repo.check_out(&branch),
            Pick::BranchMenu(name) => {
                if let Some(entries) = repo.branch_entries(&name) {
                    self.palette.show_branch_menu(name, entries);
                }
            }
            Pick::Command(command) => repo.execute(ctx, vec![command]),
            Pick::Commit(id) => repo.reveal_commit(id),
            Pick::File(path) => repo.open_diff(&path),
            Pick::Run(Run::App(_)) | Pick::Tab(_) => {}
        }
        self.remember_palette();
    }

    pub(super) fn run_app_action(&mut self, action: AppAction) {
        let columns = &mut self.settings.graph_columns;
        match action {
            AppAction::NewTab => self.show_home(),
            AppAction::OpenRepo => self.pick_folder(),
            AppAction::Clone => {
                let ctx = self.ctx.clone();
                self.run_welcome(&ctx, welcome::Action::Clone);
            }
            AppAction::NewRepository => {
                let ctx = self.ctx.clone();
                self.init_repository(&ctx);
            }
            AppAction::ToggleSidebar => self.toggle_panel(Side::Sidebar),
            AppAction::ToggleDetails => self.toggle_panel(Side::Details),
            AppAction::ZoomIn | AppAction::ZoomOut | AppAction::ZoomReset => {
                let direction = match action {
                    AppAction::ZoomIn => 1,
                    AppAction::ZoomOut => -1,
                    _ => 0,
                };
                let ctx = self.ctx.clone();
                self.set_zoom(&ctx, panels::zoom_step(self.settings.zoom, direction));
            }
            AppAction::CloseTab => self.close_tab(self.active),
            AppAction::NextTab => {
                self.active = super::cycled(self.active, self.tabs.len(), 1);
                self.home = false;
            }
            AppAction::PreviousTab => {
                self.active = super::cycled(self.active, self.tabs.len(), -1);
                self.home = false;
            }
            AppAction::Settings => self.show_settings = true,
            AppAction::Shortcuts => self.shortcuts_open = true,
            AppAction::CheckUpdates => self.updater.check_now(),
            AppAction::WhatsNew => self.whats_new = crate::whats_new::WhatsNew::since(None),
            AppAction::Help(page) => {
                if let Err(e) = page.open() {
                    self.notify(format!("Could not open {}: {e}", page.url()));
                }
            }
            AppAction::ToggleDescriptions => {
                self.settings.show_descriptions = !self.settings.show_descriptions
            }
            AppAction::ToggleFade => {
                self.settings.dim_outside_history = !self.settings.dim_outside_history
            }
            AppAction::ToggleAuthorColumn => columns.toggle(Column::Author),
            AppAction::ToggleDateColumn => columns.toggle(Column::Date),
            AppAction::ToggleHashColumn => columns.toggle(Column::Hash),
        }
        self.save_settings();
    }

    fn remember_palette(&mut self) {
        if self.palette.recent != self.settings.recent_actions {
            self.settings.recent_actions = self.palette.recent.clone();
            self.save_settings();
        }
    }

    fn save_settings(&self) {
        if !settings::is_dev_run() {
            self.settings.save();
        }
    }
}
