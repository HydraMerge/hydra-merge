//! Compare two refs (`base...target`), search all history, and quick outside actions
//! (Terminal, Finder, a link from the server) whose result is a line in the status bar.

use super::layout::loading_dots;
use super::*;
use crate::git::{compare_refs, open_link, open_terminal, reveal_in_finder, search_log};

impl WorktreeBarApp {
    /// "Compare <current branch> with <other>": `None` when there is no current branch or it is
    /// the same ref.
    pub(super) fn compare_with(&self, repo: &Repo, other: &str) -> Option<Action> {
        let wt = self.selected_worktree(repo)?;
        let base = wt.branch?;
        (base != other).then(|| {
            Action::Compare(CompareKey {
                root: repo.root.clone(),
                base,
                target: other.to_string(),
            })
        })
    }

    pub(super) fn start_compare(&mut self, ctx: &Context, key: CompareKey) {
        self.clear_location_picks();
        self.panes.compare = Some(CompareState {
            key: key.clone(),
            diff: None,
        });
        self.spawn(ctx, move |config, _| {
            let diff = compare_refs(&config, &key.root, &key.base, &key.target)
                .map(|text| Arc::new(parse_diff(&text)));
            JobResult::Compared(key, diff)
        });
    }

    pub(super) fn on_compared(&mut self, key: CompareKey, diff: AppResult<Arc<DiffView>>) {
        if let Some(state) = self.panes.compare.as_mut().filter(|state| state.key == key) {
            state.diff = Some(diff);
        }
    }

    /// Right pane of a comparison: what `target` changed since it left `base`.
    pub(super) fn compare_pane(&self, ui: &mut Ui, p: &Palette) -> Vec<Action> {
        let Some(state) = &self.panes.compare else {
            return Vec::new();
        };
        let key = &state.key;
        let mut out = Vec::new();
        ui.horizontal(|ui| {
            let title = format!("{} … {}", key.base, key.target);
            ui.label(RichText::new(title).heading().color(p.text_strong));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() {
                    out.push(Action::CloseCompare);
                }
            });
        });
        let note = format!("Changes on {} since it left {}", key.target, key.base);
        ui.label(RichText::new(note).color(p.text_muted));
        ui.add_space(8.0);
        match &state.diff {
            None => loading_dots(ui, p),
            Some(Err(error)) => {
                ui.label(RichText::new(&error.message).color(p.dirty));
            }
            Some(Ok(diff)) if diff.rows.is_empty() => {
                let same = format!("{} has no changes that {} lacks.", key.target, key.base);
                ui.label(RichText::new(same).color(p.text_muted));
            }
            Some(Ok(diff)) => {
                let files = diff
                    .rows
                    .iter()
                    .filter(|row| row.kind == RowKind::File)
                    .count();
                let count = format!("{files} file{} changed", if files == 1 { "" } else { "s" });
                ui.label(RichText::new(count).small().color(p.sidebar_heading_fg));
                diff_view::show(ui, p, diff, "compare");
            }
        }
        out
    }

    /// `git log --grep` over all branches of the open worktree.
    pub(super) fn start_search(&mut self, ctx: &Context, query: String) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        self.spawn(ctx, move |config, extra| {
            let found = search_log(&config, &extra, &path, &query);
            JobResult::Searched(path, query, found)
        });
    }

    pub(super) fn on_searched(
        &mut self,
        path: PathBuf,
        query: String,
        result: AppResult<Vec<CommitLine>>,
    ) {
        self.panes.searched = Some(SearchState {
            path,
            query,
            result,
        });
    }

    /// Commits found for the current filter of the open worktree, when a search ran for it.
    pub(super) fn search_hits(&self) -> Option<&AppResult<Vec<CommitLine>>> {
        let state = self.panes.searched.as_ref()?;
        let same_path = self.selected.as_ref() == Some(&state.path);
        let same_query = state.query == self.panes.history_query.trim();
        (same_path && same_query).then_some(&state.result)
    }

    /// Terminal, Finder and links run on the action job; the status line says what happened.
    pub(super) fn start_outside(&mut self, ctx: &Context, action: Action) {
        self.spawn(ctx, move |config, extra| {
            let done = match action {
                Action::OpenTerminal(path) => open_terminal(&config, &extra, &path)
                    .map(|p| format!("Opened Terminal at {}", p.display())),
                Action::RevealInFinder(path) => reveal_in_finder(&config, &extra, &path)
                    .map(|p| {
                        format!(
                            "Showed {} in {}",
                            p.display(),
                            crate::os::file_manager_name()
                        )
                    }),
                Action::OpenLink(url) => open_link(&url).map(|()| format!("Opened {url}")),
                _ => Ok(String::new()),
            };
            JobResult::Done(done)
        });
    }

    pub(super) fn on_done(&mut self, result: AppResult<String>) {
        match result {
            Ok(text) => {
                self.error = None;
                self.status = text;
            }
            Err(e) => self.set_error(e),
        }
    }
}
