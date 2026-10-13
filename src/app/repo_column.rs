//! The Repositories column: every scanned repository, its filter and its marks.

use super::*;

impl WorktreeBarApp {
    /// Every scanned repository, top to bottom. A click opens it as a top tab. The filter
    /// (magnifier or ⌘L) narrows the list; a dot marks changes, another an open tab.
    pub(super) fn repo_sidebar(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        repos: &[Repo],
    ) -> Option<Action> {
        let shown = Panel::left("repositories")
            .resizable(true)
            .default_size(REPOS_DEFAULT_WIDTH)
            .min_size(REPOS_MIN_WIDTH)
            .frame(panel_frame(p.sidebar_bg).inner_margin(Margin {
                left: 0,
                right: COLUMN_GAP,
                top: 0,
                bottom: 0,
            }))
            .show(ui, |ui| {
                ui.visuals_mut().widgets.noninteractive.fg_stroke.color = p.sidebar_fg;
                self.repos_header(ui, p, repos.len());
                Frame::new()
                    .inner_margin(Margin::symmetric(8, 4))
                    .show(ui, |ui| self.repo_list(ui, p, repos))
                    .inner
            });
        paint_column_edge(ui, p, shown.response.rect);
        shown.inner
    }

    /// Title, count and the magnifier that opens the filter.
    fn repos_header(&mut self, ui: &mut Ui, p: &Palette, count: usize) {
        let rect = column_header(ui, p, "Repositories");
        let icon =
            Rect::from_center_size(pos2(rect.right() - 8.0, rect.center().y), vec2(16.0, 16.0));
        ui.painter().text(
            pos2(icon.left() - 6.0, rect.center().y),
            Align2::RIGHT_CENTER,
            count.to_string(),
            FontId::proportional(12.0),
            p.text_muted,
        );
        let response = ui
            .interact(icon, Id::new("repo-search-toggle"), Sense::click())
            .on_hover_text(format!(
                "Filter repositories ({})",
                crate::os::shortcut_find_repo()
            ));
        let color = if response.hovered() {
            p.text_strong
        } else {
            p.text_muted
        };
        super::layout::paint_magnifier(ui.painter(), icon.center(), color);
        if response.clicked() {
            let panes = &mut self.panes;
            panes.repo_search_open = !panes.repo_search_open;
            panes.repo_focus = panes.repo_search_open;
            if !panes.repo_search_open {
                panes.repo_query.clear();
            }
        }
    }

    fn repo_list(&mut self, ui: &mut Ui, p: &Palette, repos: &[Repo]) -> Option<Action> {
        if repos.is_empty() {
            ui.label(RichText::new("Nothing scanned yet").color(p.text_muted));
            return None;
        }
        if self.panes.repo_search_open || !self.panes.repo_query.is_empty() {
            ui.scope(|ui| {
                ui.visuals_mut().extreme_bg_color = p.input_bg;
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.panes.repo_query)
                        .hint_text("Filter repositories")
                        .desired_width(f32::INFINITY)
                        .id_salt("repo-search"),
                );
                if std::mem::take(&mut self.panes.repo_focus) {
                    edit.request_focus();
                }
                if edit.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                    self.panes.repo_query.clear();
                    self.panes.repo_search_open = false;
                }
            });
            ui.add_space(2.0);
        }
        let query = self.panes.repo_query.clone();
        let shown: Vec<&Repo> = repos
            .iter()
            .filter(|repo| name_matches(&tab_label(repos, repo), &query))
            .collect();
        if shown.is_empty() {
            ui.label(RichText::new(format!("No repository matches '{query}'")).color(p.text_muted));
            return None;
        }
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut picked = None;
                for repo in shown {
                    let active = self.active_root.as_ref() == Some(&repo.root);
                    let open = !self.closed_tabs.contains(&repo.root);
                    let marks = repo_marks(p, repo_has_changes(repo), open && !active);
                    let label = tab_label(repos, repo);
                    let tip = repo_tip(repo, open);
                    let response = location_row(ui, p, &label, active, &marks).on_hover_text(tip);
                    if response.clicked() {
                        picked = Some(Action::Activate(repo.root.clone()));
                    }
                }
                picked
            })
            .inner
    }
}

/// Any worktree with uncommitted changes (as far as its status is read).
fn repo_has_changes(repo: &Repo) -> bool {
    repo.worktrees.iter().any(|wt| wt.dirty == Some(true))
}

/// Changes (orange) before an open tab (accent), right-aligned in that order.
fn repo_marks(p: &Palette, changes: bool, open_tab: bool) -> Vec<Color32> {
    let mut marks = Vec::new();
    if changes {
        marks.push(p.dirty);
    }
    if open_tab {
        marks.push(p.accent);
    }
    marks
}

fn repo_tip(repo: &Repo, open: bool) -> String {
    let mut tip = repo.root.display().to_string();
    let dirty = repo
        .worktrees
        .iter()
        .filter(|wt| wt.dirty == Some(true))
        .count();
    if dirty > 0 {
        tip.push_str(&format!("\n{dirty} worktree(s) with changes"));
    }
    if open {
        tip.push_str("\nOpen as a tab");
    }
    tip
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_marks_show_changes_then_open_tab() {
        let p = &crate::theme::LIGHT;
        assert_eq!(repo_marks(p, true, true), vec![p.dirty, p.accent]);
        assert_eq!(repo_marks(p, false, true), vec![p.accent]);
        assert!(repo_marks(p, false, false).is_empty());
    }
}
