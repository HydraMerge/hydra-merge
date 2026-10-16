//! "Create Sparse Worktree": pick the top-level folders the new worktree should hold.

use super::*;

impl WorktreeBarApp {
    /// Cancel, Escape and a click outside close the window; Create needs at least one folder.
    pub(super) fn sparse_dialog(&mut self, ctx: &Context, p: &Palette) -> Option<Action> {
        let dialog = self.sparse_dialog.as_mut()?;
        let mut picked = None;
        let modal = egui::Modal::new(Id::new("sparse-worktree")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(RichText::new("Create Sparse Worktree").heading());
            let dest = tilde_path(&dialog.root.join(&dialog.branch));
            ui.label(RichText::new(format!("{} at {dest}", dialog.branch)).color(p.text_muted));
            ui.label(
                RichText::new(
                    "Only the checked folders are written to disk. Files at the root always are.",
                )
                .small()
                .color(p.text_muted),
            );
            ui.add_space(8.0);
            ScrollArea::vertical()
                .id_salt("sparse-folders")
                .max_height(280.0)
                .show(ui, |ui| {
                    for (name, checked) in dialog.folders.iter_mut() {
                        ui.checkbox(checked, format!("{name}/"));
                    }
                });
            ui.add_space(10.0);
            let chosen: Vec<String> = dialog
                .folders
                .iter()
                .filter(|(_, checked)| *checked)
                .map(|(name, _)| name.clone())
                .collect();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} of {} folders",
                        chosen.len(),
                        dialog.folders.len()
                    ))
                    .color(p.text_muted),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let create = Button::new(RichText::new("Create").color(p.button_primary_fg))
                        .fill(p.button_primary_bg);
                    if ui.add_enabled(!chosen.is_empty(), create).clicked() {
                        picked = Some(Action::CreateSparse(
                            dialog.root.clone(),
                            dialog.branch.clone(),
                            chosen.clone(),
                        ));
                    }
                    if ui.button("Cancel").clicked() {
                        picked = Some(Action::CloseSparse);
                    }
                });
            });
        });
        if modal.should_close() && picked.is_none() {
            picked = Some(Action::CloseSparse);
        }
        picked
    }
}
