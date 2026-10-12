//! The confirmation dialog for destructive actions.

use super::*;

impl WorktreeBarApp {
    /// Cancel, Escape and a click outside close the dialog; only the danger button acts.
    pub(super) fn confirm_dialog(&mut self, ctx: &Context, p: &Palette) -> Option<Action> {
        let confirm = self.confirm.as_ref()?;
        let mut decision: Option<bool> = None;
        let modal = egui::Modal::new(Id::new("confirm")).show(ctx, |ui| {
            ui.label(RichText::new(confirm.title).heading());
            ui.label(&confirm.subject);
            if let Some(note) = confirm.note {
                ui.label(RichText::new(note).color(p.text_muted));
            }
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    decision = Some(false);
                }
                let label = RichText::new(confirm.button).color(p.button_danger_fg);
                if ui
                    .add(Button::new(label).fill(p.button_danger_bg))
                    .clicked()
                {
                    decision = Some(true);
                }
            });
        });
        if modal.should_close() {
            decision = decision.or(Some(false));
        }
        let confirmed = decision?;
        let confirm = self.confirm.take()?;
        confirmed.then_some(*confirm.then)
    }
}

impl WorktreeBarApp {
    /// "Open Repository": a path typed, or picked with "Choose…" in the macOS folder picker.
    /// Enter or Open adds it; Esc, Cancel or a click outside closes the window.
    pub(super) fn open_repo_dialog(&mut self, ctx: &Context, p: &Palette) -> Option<Action> {
        self.open_dialog.as_ref()?;
        let busy = self.busy();
        let roots: Vec<String> = self.config.roots.iter().map(|r| tilde_path(r)).collect();
        let mut picked = None;
        let modal = egui::Modal::new(Id::new("open-repository")).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.label(RichText::new("Open Repository").heading());
            let hint = format!("A .bare layout or a clone under {}", roots.join(", "));
            ui.label(RichText::new(hint).color(p.text_muted));
            ui.add_space(8.0);
            picked = self.open_dialog_body(ui, p, busy);
        });
        if modal.should_close() && picked.is_none() {
            self.open_dialog = None;
        }
        picked
    }

    fn open_dialog_body(&mut self, ui: &mut Ui, p: &Palette, busy: bool) -> Option<Action> {
        let dialog = self.open_dialog.as_mut()?;
        let mut picked = None;
        ui.horizontal(|ui| {
            let edit = ui.add(
                TextEdit::singleline(&mut dialog.path)
                    .id_salt("open-repository-path")
                    .hint_text("/Users/…/repos/group/repo")
                    .desired_width(440.0),
            );
            if dialog.focus {
                edit.request_focus();
                dialog.focus = false;
            }
            let entered = edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if entered && !dialog.path.trim().is_empty() {
                picked = Some(Action::AddPath);
            }
            if ui.add_enabled(!busy, Button::new("Choose…")).clicked() {
                picked = Some(Action::ChooseFolder);
            }
        });
        if let Some(error) = &dialog.error {
            ui.add_space(4.0);
            ui.label(RichText::new(error).color(p.error_accent));
        }
        ui.add_space(10.0);
        let ready = !busy && !dialog.path.trim().is_empty();
        ui.horizontal(|ui| {
            if busy {
                ui.add(Spinner::new());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let open = Button::new(RichText::new("Open").color(p.button_primary_fg))
                    .fill(p.button_primary_bg);
                if ui.add_enabled(ready, open).clicked() {
                    picked = Some(Action::AddPath);
                }
                if ui.button("Cancel").clicked() {
                    picked = Some(Action::CloseRepoDialog);
                }
            });
        });
        picked
    }
}
