//! Main area: the commit list and the right pane (Summary, a commit, or a picked location).

use super::history::commit_detail;
use super::layout::{
    history_width, in_column, loading_dots, pane_header, splitter, PANE_HEADER, SPLITTER_WIDTH,
};
use super::*;

/// Inner margin of the right pane.
const PANE_MARGIN: f32 = 12.0;

impl WorktreeBarApp {
    pub(super) fn central(&mut self, ui: &mut Ui, p: &Palette, repos: &[Repo]) -> Vec<Action> {
        let mut out = Vec::new();
        CentralPanel::default_margins()
            .frame(Frame::new().fill(p.bg))
            .show(ui, |ui| {
                let full = ui.available_rect_before_wrap();
                ui.allocate_rect(full, Sense::hover());
                let Some(repo) = self.active_repo(repos).cloned() else {
                    let roots = self
                        .config
                        .roots
                        .iter()
                        .map(|path| tilde_path(path))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let hint = if self.autoscan {
                        format!("Repositories under {roots} are listed on the left. Open one to show it here.")
                    } else {
                        "Auto-scan is off. Use Rescan or Scan Folder… in Settings, or turn Scan at Startup on.".to_string()
                    };
                    in_column(ui, full, "empty", PANE_MARGIN, |ui| {
                        ui.label(RichText::new(hint).color(p.text_muted))
                    });
                    return;
                };
                if crate::trust::is_blocked(&self.config, &repo) {
                    out.extend(in_column(ui, full, "untrusted", PANE_MARGIN, |ui| {
                        trust_card(ui, p, &repo)
                    }));
                    return;
                }
                match self.selected_worktree(&repo) {
                    Some(wt) => out.extend(self.worktree_panes(ui, p, &repo, &wt, full)),
                    None => out.extend(in_column(ui, full, "pick", PANE_MARGIN, |ui| {
                        self.pick_detail(ui, p, &repo).unwrap_or_else(|| {
                            let hint = "Select a worktree, branch, tag, or stash.";
                            ui.label(RichText::new(hint).color(p.text_muted));
                            Vec::new()
                        })
                    })),
                }
            });
        out
    }

    /// Commit list on the left, splitter, right pane.
    fn worktree_panes(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        repo: &Repo,
        wt: &Worktree,
        full: Rect,
    ) -> Vec<Action> {
        let width = history_width(self.history_width, full.width());
        let (left, split, right) = columns(full, width);
        let (left_head, left_body) = header_split(left);
        let (right_head, right_body) = header_split(right);
        ui.painter().rect_filled(right_body, 0.0, p.detail_bg);
        let half = left.width() / 2.0;
        pane_header(ui, p, left_head, &[("Commits", true)], (half, p.bg));
        self.history_search_toggle(ui, p, left_head);
        let left_body = self.history_filter_row(ui, p, left_body);
        let title = self.right_title();
        pane_header(ui, p, right_head, &[(title, true)], (90.0, p.detail_bg));
        let loaded = self
            .view
            .as_ref()
            .filter(|view| view.path == wt.path)
            .map(|view| view.result.clone());
        let mut out = Vec::new();
        match loaded {
            None => {
                in_column(ui, left_body, "loading", PANE_MARGIN, |ui| {
                    loading_dots(ui, p)
                });
            }
            Some(Err(error)) => {
                in_column(ui, left_body, "load-error", PANE_MARGIN, |ui| {
                    ui.label(RichText::new(&error.message).color(p.dirty))
                });
            }
            Some(Ok(data)) => {
                out.extend(self.commit_list(ui, p, left_body, &data));
                out.extend(in_column(ui, right_body, "right-pane", PANE_MARGIN, |ui| {
                    self.right_pane(ui, p, repo, wt, &data)
                }));
            }
        }
        if let Some(dragged) = splitter(ui, p, split, width) {
            self.history_width = Some(history_width(Some(dragged), full.width()));
        }
        out
    }

    /// A picked location first, then the open commit, otherwise the working copy Summary.
    fn right_pane(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        repo: &Repo,
        wt: &Worktree,
        data: &ViewData,
    ) -> Vec<Action> {
        if self
            .panes
            .compare
            .as_ref()
            .is_some_and(|c| c.key.root == repo.root)
        {
            return self.compare_pane(ui, p);
        }
        if let Some(actions) = self.pick_detail(ui, p, repo) {
            return actions;
        }
        let open = self.open_commit.as_deref();
        // A commit found by "Search all history" may be older than the loaded list.
        let found = self.search_hits().and_then(|r| r.as_ref().ok());
        let commit = open.and_then(|hash| {
            let listed = data.view.commits.iter().chain(found.into_iter().flatten());
            listed.into_iter().find(|c| c.hash == hash)
        });
        if let Some(commit) = commit {
            commit_detail(ui, p, commit, self.patch.as_ref(), &wt.path, self.date_format);
            return Vec::new();
        }
        self.summary_pane(ui, p, repo, wt, data)
    }

    fn right_title(&self) -> &'static str {
        if self.panes.compare.is_some() {
            "Compare"
        } else if !self.chosen_branch.is_empty() {
            "Branch"
        } else if self.chosen_remote.is_some() {
            "Remote Branch"
        } else if self.chosen_tag.is_some() {
            "Tag"
        } else if self.chosen_stash.is_some() {
            "Stash"
        } else if self.open_commit.is_some() {
            "Commit"
        } else {
            "Summary"
        }
    }

    /// Detail of a branch, remote branch, tag or stash picked in Locations.
    fn pick_detail(&self, ui: &mut Ui, p: &Palette, repo: &Repo) -> Option<Vec<Action>> {
        if repo.layout == Layout::Bare && !self.chosen_branch.is_empty() {
            return Some(self.branch_detail(ui, p, repo));
        }
        if let Some((remote, branch)) = &self.chosen_remote {
            return Some(self.remote_detail(ui, p, repo, remote, branch));
        }
        if let Some((name, commit)) = &self.chosen_tag {
            self.tag_detail(ui, p, name, commit);
            return Some(Vec::new());
        }
        let id = self.chosen_stash.as_ref()?;
        Some(self.stash_detail(ui, p, repo, id))
    }

    pub(super) fn selected_worktree(&self, repo: &Repo) -> Option<Worktree> {
        let path = self.selected.as_deref()?;
        repo.worktrees.iter().find(|wt| wt.path == path).cloned()
    }

    pub(super) fn branch_detail(&self, ui: &mut Ui, p: &Palette, repo: &Repo) -> Vec<Action> {
        ui.label(
            RichText::new(&self.chosen_branch)
                .heading()
                .color(p.text_strong),
        );
        ui.add_space(4.0);
        ui.label(RichText::new("Not checked out").color(p.text_muted));
        ui.add_space(12.0);
        let busy = self.busy();
        let label = RichText::new("Create worktree").color(p.button_primary_fg);
        let clicked = ui
            .add_enabled(!busy, Button::new(label).fill(p.button_primary_bg))
            .clicked();
        clicked
            .then(|| Action::Create(repo.root.clone(), self.chosen_branch.clone()))
            .into_iter()
            .collect()
    }

    pub(super) fn remote_detail(
        &self,
        ui: &mut Ui,
        p: &Palette,
        repo: &Repo,
        remote: &str,
        branch: &str,
    ) -> Vec<Action> {
        let name = format!("{remote}/{branch}");
        let bare = repo.layout == Layout::Bare;
        let hint = if bare {
            "Remote branch. Create makes a local branch and a worktree from it."
        } else {
            "Remote branch. Create is only available for a .bare layout."
        };
        detail_title(ui, p, &name, hint);
        let busy = self.busy();
        let mut out = Vec::new();
        ui.horizontal(|ui| {
            let primary = (p.button_primary_fg, p.button_primary_bg);
            if bare && filled_button(ui, "Create worktree", primary, !busy) {
                out.push(Action::CreateRemote {
                    root: repo.root.clone(),
                    remote: remote.to_string(),
                    branch: branch.to_string(),
                });
            }
            let danger = (p.button_danger_fg, p.button_danger_bg);
            if filled_button(ui, "Delete remote branch", danger, !busy) {
                out.push(delete_remote_confirm(&repo.root, remote, branch));
            }
        });
        out
    }

    pub(super) fn tag_detail(&self, ui: &mut Ui, p: &Palette, name: &str, commit: &str) {
        ui.label(RichText::new(name).heading().color(p.text_strong));
        ui.add_space(4.0);
        ui.label(RichText::new(commit).monospace().color(p.text_muted));
    }

    fn stash_subject(&self, repo: &Repo, id: &str) -> String {
        self.branches
            .as_ref()
            .filter(|state| state.repo_root == repo.root)
            .and_then(|state| state.list.stashes.iter().find(|stash| stash.id == id))
            .map(|stash| stash.subject.clone())
            .unwrap_or_default()
    }

    pub(super) fn stash_detail(
        &self,
        ui: &mut Ui,
        p: &Palette,
        repo: &Repo,
        id: &str,
    ) -> Vec<Action> {
        let subject = self.stash_subject(repo, id);
        detail_title(ui, p, id, &subject);
        let Some(worktree) = self.stash_worktree.clone() else {
            ui.label(RichText::new("Open a worktree before applying a stash.").color(p.text_muted));
            return Vec::new();
        };
        let busy = self.busy();
        let mut action = None;
        ui.horizontal(|ui| {
            if filled_button(
                ui,
                "Apply",
                (p.button_primary_fg, p.button_primary_bg),
                !busy,
            ) {
                action = Some(Action::ApplyStash {
                    worktree: worktree.clone(),
                    id: id.to_string(),
                });
            }
            if filled_button(ui, "Drop", (p.button_danger_fg, p.button_danger_bg), !busy) {
                action = Some(Action::Ask(Confirm {
                    title: "Drop stash?",
                    subject: id.to_string(),
                    note: None,
                    button: "Drop",
                    then: Box::new(Action::DropStash {
                        worktree,
                        id: id.to_string(),
                    }),
                }));
            }
        });
        action.into_iter().collect()
    }
}

/// Commit list, splitter and right pane side by side in `full`.
fn columns(full: Rect, width: f32) -> (Rect, Rect, Rect) {
    let left = Rect::from_min_size(full.min, vec2(width, full.height()));
    let split = Rect::from_min_size(
        pos2(left.right(), full.top()),
        vec2(SPLITTER_WIDTH, full.height()),
    );
    let right = Rect::from_min_max(pos2(split.right(), full.top()), full.max);
    (left, split, right)
}

/// The pane-title strip at the top of `rect`, and the rest below it.
fn header_split(rect: Rect) -> (Rect, Rect) {
    let head = Rect::from_min_size(rect.min, vec2(rect.width(), PANE_HEADER));
    let body = Rect::from_min_max(pos2(rect.left(), head.bottom()), rect.max);
    (head, body)
}

/// Heading and one line under it, as at the top of every detail view.
fn detail_title(ui: &mut Ui, p: &Palette, title: &str, line: &str) {
    ui.label(RichText::new(title).heading().color(p.text_strong));
    ui.add_space(4.0);
    ui.label(RichText::new(line).color(p.text_muted));
    ui.add_space(12.0);
}

/// A button with its own fill; `colors` is (text, fill). Returns whether it was clicked.
fn filled_button(ui: &mut Ui, label: &str, colors: (Color32, Color32), enabled: bool) -> bool {
    let (fg, fill) = colors;
    let button = Button::new(RichText::new(label).color(fg)).fill(fill);
    ui.add_enabled(enabled, button).clicked()
}

/// What stands in for the repository while it is not trusted: what can run there, and the button.
fn trust_card(ui: &mut Ui, p: &Palette, repo: &Repo) -> Vec<Action> {
    ui.label(
        RichText::new("This repository is not trusted")
            .heading()
            .color(p.text_strong),
    );
    ui.add_space(6.0);
    let why = "Its own config or hooks can start programs on this computer. Nothing of it is read \
               or run until you trust it: no status, history, diff or fetch.";
    ui.label(RichText::new(why).color(p.text_muted));
    ui.add_space(10.0);
    ui.label(RichText::new("What can run").color(p.text));
    for name in &repo.code_sources {
        ui.label(RichText::new(name).monospace().color(p.dirty));
    }
    ui.add_space(10.0);
    let note = "Trust it only if you know where this folder came from. If any of these settings \
                changes later, you are asked again.";
    ui.label(RichText::new(note).color(p.text_muted));
    ui.add_space(12.0);
    let label = RichText::new("Trust this repository").color(p.button_primary_fg);
    let clicked = ui
        .add(Button::new(label).fill(p.button_primary_bg))
        .clicked();
    clicked
        .then(|| Action::TrustRepo(repo.root.clone()))
        .into_iter()
        .collect()
}
