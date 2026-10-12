//! Skins in the window: the palette used for drawing, applying a skin to egui, the ⚙ → Skin menu,
//! creating a skin file from the current colours, and picking up edits of `.skin` files live.

use std::hash::{DefaultHasher, Hash, Hasher};

use super::*;
use crate::git::{open_file_in_editor, open_folder};
use crate::skin::{
    builtin_skins, create_skin_file, load_custom_skins, load_skin_choice, save_skin_choice,
    skins_dir, Skin, AUTO,
};
use crate::theme::visuals_for;

/// The skins known to the window and the chosen one.
#[derive(Default)]
pub(super) struct Skins {
    pub list: Vec<Skin>,
    pub chosen: String,
    /// Modification times of the skins folder, to notice edits.
    stamp: u64,
}

/// Changes when a `.skin` file is added, removed or saved.
fn folder_stamp(dir: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut files: Vec<_> = entries
            .flatten()
            .filter_map(|e| Some((e.path(), e.metadata().ok()?.modified().ok()?)))
            .collect();
        files.sort();
        files.hash(&mut hasher);
    }
    hasher.finish()
}

impl WorktreeBarApp {
    /// Loads built-in and custom skins and applies the stored choice.
    pub(super) fn init_skins(&mut self, ctx: &Context) {
        self.skins.chosen = self
            .theme_dir
            .as_deref()
            .map_or_else(|| AUTO.to_string(), load_skin_choice);
        self.reload_skins();
        self.apply_skin(ctx);
    }

    /// Built-in skins, then the files of the skins folder.
    fn reload_skins(&mut self) -> Vec<String> {
        let builtins = builtin_skins();
        let custom = self
            .theme_dir
            .as_deref()
            .map(|dir| load_custom_skins(dir, &builtins))
            .unwrap_or_default();
        if let Some(dir) = &self.theme_dir {
            self.skins.stamp = folder_stamp(&skins_dir(dir));
        }
        let chosen = self.skins.chosen.clone();
        let warnings = custom
            .iter()
            .find(|(skin, _)| skin.name == chosen)
            .map(|(_, w)| w.clone())
            .unwrap_or_default();
        self.skins.list = builtins
            .into_iter()
            .chain(custom.into_iter().map(|(s, _)| s))
            .collect();
        warnings
    }

    fn chosen_skin(&self) -> Option<&Skin> {
        self.skins.list.iter().find(|s| s.name == self.skins.chosen)
    }

    /// The palette everything is drawn with this frame.
    pub(super) fn current_palette(&self, ctx: &Context) -> Palette {
        match self.chosen_skin() {
            Some(skin) => skin.palette.clone(),
            None => theme::palette(ThemeChoice::from_egui(ctx.theme())).clone(),
        }
    }

    /// Auto: Merge Light / Dark by the system and the theme switch. A skin: its own mode only.
    fn apply_skin(&self, ctx: &Context) {
        ctx.set_visuals_of(egui::Theme::Dark, theme::visuals(ThemeChoice::Dark));
        ctx.set_visuals_of(egui::Theme::Light, theme::visuals(ThemeChoice::Light));
        let Some(skin) = self.chosen_skin() else {
            let stored = self.theme_dir.as_deref().and_then(theme::load_theme_choice);
            ctx.set_theme(stored.map_or(egui::ThemePreference::System, ThemeChoice::to_preference));
            return;
        };
        let choice = if skin.dark {
            ThemeChoice::Dark
        } else {
            ThemeChoice::Light
        };
        let mode = if skin.dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        };
        ctx.set_visuals_of(mode, visuals_for(&skin.palette, choice));
        ctx.set_theme(choice.to_preference());
    }

    /// Chooses a skin by name, keeps the choice, and reports problems of its file.
    pub(super) fn set_skin(&mut self, ctx: &Context, name: String) {
        self.skins.chosen = name.clone();
        let warnings = self.reload_skins();
        if self.chosen_skin().is_none() {
            self.skins.chosen = AUTO.to_string();
        }
        self.apply_skin(ctx);
        if let Some(dir) = &self.theme_dir {
            if let Err(e) = save_skin_choice(dir, &self.skins.chosen) {
                self.set_error(e);
            }
        }
        self.report_skin(&name, &warnings);
    }

    fn report_skin(&mut self, name: &str, warnings: &[String]) {
        if warnings.is_empty() {
            self.status = format!("Skin: {}", self.skins.chosen);
            return;
        }
        let problem = AppError::new(
            ErrorCode::Io,
            format!(
                "Skin '{name}': {} line(s) could not be used",
                warnings.len()
            ),
        );
        self.set_error(problem.with_detail(&warnings.join("\n")));
    }

    /// Writes the current colours to a new `.skin` file, switches to it and opens it to edit.
    pub(super) fn create_skin(&mut self, ctx: &Context) {
        let Some(dir) = self.theme_dir.clone() else {
            return self.set_error(AppError::new(
                ErrorCode::Io,
                "The home directory is not set: skins cannot be saved",
            ));
        };
        let dark = ThemeChoice::from_egui(ctx.theme()) == ThemeChoice::Dark;
        let current = self.chosen_skin().cloned().unwrap_or_else(|| Skin {
            name: if dark { "Merge Dark" } else { "Merge Light" }.to_string(),
            dark,
            palette: self.current_palette(ctx),
            file: None,
        });
        let created = create_skin_file(&dir, &current, &format!("{} copy", current.name));
        match created {
            Ok(file) => {
                let name = file
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.set_skin(ctx, name);
                if let Err(e) = open_file_in_editor(&self.config, &self.editors.current, &file) {
                    self.set_error(e);
                }
            }
            Err(e) => self.set_error(e),
        }
    }

    pub(super) fn open_skins_folder(&mut self) {
        let Some(dir) = self.theme_dir.as_deref().map(skins_dir) else {
            return;
        };
        let opened = std::fs::create_dir_all(&dir)
            .map_err(|e| AppError::io(&format!("cannot create {}", dir.display()), &e))
            .and_then(|()| open_folder(&dir));
        if let Err(e) = opened {
            self.set_error(e);
        }
    }

    /// A saved `.skin` file shows at once; checked with the other background work.
    pub(super) fn watch_skins(&mut self, ctx: &Context) {
        let Some(dir) = self.theme_dir.as_deref().map(skins_dir) else {
            return;
        };
        if folder_stamp(&dir) == self.skins.stamp {
            return;
        }
        let warnings = self.reload_skins();
        if self.chosen_skin().is_some_and(|s| s.file.is_some()) {
            self.apply_skin(ctx);
            let name = self.skins.chosen.clone();
            self.report_skin(&name, &warnings);
        }
    }

    /// ⚙ → Skin: Auto, the built-in skins, your skins, and the two skin tools.
    pub(super) fn skin_menu(&self, ui: &mut Ui) -> Option<Action> {
        let mut picked = None;
        ui.menu_button("Skin", |ui| {
            let mut item = |ui: &mut Ui, name: &str, label: String| {
                let mark = if self.skins.chosen == name {
                    "✓ "
                } else {
                    "    "
                };
                if ui.button(format!("{mark}{label}")).clicked() {
                    picked = Some(Action::SetSkin(name.to_string()));
                    ui.close();
                }
            };
            item(ui, AUTO, "Auto (Merge, follows the system)".to_string());
            ui.separator();
            for skin in self.skins.list.iter().filter(|s| s.file.is_none()) {
                item(ui, &skin.name, skin.name.clone());
            }
            let custom: Vec<&Skin> = self
                .skins
                .list
                .iter()
                .filter(|s| s.file.is_some())
                .collect();
            if !custom.is_empty() {
                ui.separator();
                for skin in custom {
                    item(ui, &skin.name, skin.name.clone());
                }
            }
            ui.separator();
            for (label, action) in [
                ("Create Skin from Current…", Action::CreateSkin),
                ("Open Skins Folder", Action::OpenSkinsFolder),
            ] {
                if ui.button(label).clicked() {
                    picked = Some(action);
                    ui.close();
                }
            }
        });
        picked
    }
}
