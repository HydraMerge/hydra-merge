//! Left panel: branches, remotes, tags, stashes, submodules and worktrees.

use super::toolbar::{menu_entries, worktree_entries};
use super::elide::{fit, Elide};
use super::tree::{build_tree, Node};
use super::wt_status::{self, WtState};
use super::*;

impl WorktreeBarApp {
    pub(super) fn locations(&mut self, ui: &mut Ui, p: &Palette, repos: &[Repo]) -> Option<Action> {
        let shown = Panel::left("locations")
            .resizable(true)
            .default_size(LOCATIONS_DEFAULT_WIDTH)
            .min_size(LOCATIONS_MIN_WIDTH)
            .frame(panel_frame(p.sidebar_bg).inner_margin(Margin {
                left: 0,
                right: COLUMN_GAP,
                top: 0,
                bottom: 0,
            }))
            .show(ui, |ui| {
                ui.visuals_mut().widgets.noninteractive.fg_stroke.color = p.sidebar_fg;
                ui.visuals_mut().indent_has_left_vline = false;
                ui.visuals_mut().widgets.inactive.fg_stroke.color = p.sidebar_fg;
                self.locations_header(ui, p);
                Frame::new()
                    .inner_margin(Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                if self.search_open || !self.query.is_empty() {
                    ui.scope(|ui| {
                        ui.visuals_mut().extreme_bg_color = p.input_bg;
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut self.query)
                                .hint_text("Filter locations")
                                .desired_width(f32::INFINITY)
                                .id_salt("loc-search"),
                        );
                        if self.search_open && self.query.is_empty() && !edit.has_focus() {
                            edit.request_focus();
                        }
                    });
                }
                ui.add_space(2.0);
                let repo = self.active_repo(repos).cloned();
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match &repo {
                        Some(repo) => self.location_lists(ui, p, repo),
                        None => {
                            ui.label(RichText::new("No repository open").color(p.text_muted));
                            None
                        }
                    })
                    .inner
                    })
                    .inner
            });
        paint_column_edge(ui, p, shown.response.rect);
        shown.inner
    }

    /// Same band as Repositories: title on the left, search on the right.
    fn locations_header(&mut self, ui: &mut Ui, p: &Palette) {
        let rect = column_header(ui, p, "Locations");
        let icon =
            Rect::from_center_size(pos2(rect.right() - 8.0, rect.center().y), vec2(16.0, 16.0));
        let response = ui
            .interact(icon, Id::new("loc-search-toggle"), Sense::click())
            .on_hover_text("Filter locations");
        let color = if response.hovered() {
            p.text_strong
        } else {
            p.text_muted
        };
        super::layout::paint_magnifier(ui.painter(), icon.center(), color);
        if response.clicked() {
            self.search_open = !self.search_open;
            if !self.search_open {
                self.query.clear();
            }
        }
        ui.add_space(2.0);
    }

    pub(super) fn location_lists(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        repo: &Repo,
    ) -> Option<Action> {
        let query = self.query.clone();
        // Worktrees first: they are what this window is for; branches follow.
        let worktrees = self.worktree_section(ui, p, repo, &query);
        let list = self
            .branches
            .as_ref()
            .filter(|state| state.repo_root == repo.root)
            .map(|state| state.list.clone());
        let Some(list) = list else {
            if crate::trust::is_blocked(&self.config, repo) {
                // Not read at all, so there is no count to show.
                section_title(ui, p, "Branches");
                ui.label(RichText::new("Not trusted").color(p.text_muted));
            } else {
                section_header(ui, p, "Branches", 0);
                super::layout::loading_dots(ui, p);
            }
            return worktrees;
        };
        let mut action = worktrees.or(self.local_branch_section(ui, p, repo, &list, &query));
        action = action.or(self.remote_section(ui, p, repo, &list, &query));
        action = action.or(self.tag_section(ui, p, repo, &list, &query));
        action = action.or(self.stash_section(ui, p, repo, &list, &query));
        action.or(self.submodule_section(ui, p, repo, &list, &query))
    }

    pub(super) fn local_branch_section(
        &self,
        ui: &mut Ui,
        p: &Palette,
        repo: &Repo,
        list: &BranchList,
        query: &str,
    ) -> Option<Action> {
        let names: Vec<&str> = list
            .local
            .iter()
            .map(String::as_str)
            .filter(|name| name_matches(name, query))
            .collect();
        let id = Id::new(("branches", &repo.root));
        let searching = !query.is_empty();
        section(ui, p, id, "Branches", names.len(), searching, |ui| {
            let mut leaf = |ui: &mut Ui, label: &str, full: &str| {
                self.local_leaf(ui, p, repo, list, label, full)
            };
            tree_rows(ui, p, &build_tree(&names), id, searching, &mut leaf)
        })
    }

    /// A checked-out branch selects its worktree; a free one offers Create.
    fn local_leaf(
        &self,
        ui: &mut Ui,
        p: &Palette,
        repo: &Repo,
        list: &BranchList,
        label: &str,
        full: &str,
    ) -> Option<Action> {
        let used = list
            .used
            .iter()
            .find(|(branch, _)| branch == full)
            .map(|(_, path)| path);
        let selected = match used {
            Some(path) => self.selected.as_deref() == Some(path.as_path()),
            None => self.selected.is_none() && self.chosen_branch == full,
        };
        let response = location_row(ui, p, label, selected, &[]).on_hover_text(full);
        let mut menu = None;
        response.context_menu(|ui| menu = self.local_menu(ui, repo, used, full));
        let clicked = response.clicked().then(|| match used {
            Some(path) => Action::Select(path.clone()),
            None => Action::PickBranch(full.to_string()),
        });
        menu.or(clicked)
    }

    /// Right click on a local branch: its worktree, or Create for a free branch.
    fn local_menu(
        &self,
        ui: &mut Ui,
        repo: &Repo,
        used: Option<&PathBuf>,
        branch: &str,
    ) -> Option<Action> {
        let busy = self.busy();
        let mut items: Vec<(String, bool, Action)> = Vec::new();
        match used {
            Some(path) => {
                items.push((
                    format!("Open in {}", self.editors.current.name()),
                    !busy,
                    Action::Open(path.clone()),
                ));
                items.push(("Show Worktree".into(), true, Action::Select(path.clone())));
            }
            None => items.push((
                format!("Create Worktree for {branch}"),
                !busy && repo.layout == Layout::Bare,
                Action::Create(repo.root.clone(), branch.to_string()),
            )),
        }
        if let Some(compare) = self.compare_with(repo, branch) {
            items.push((format!("Compare with {branch}"), !busy, compare));
        }
        if let Some(path) = self.selected.clone() {
            let current = self
                .selected_worktree(repo)
                .and_then(|wt| wt.branch.clone());
            if current.as_deref() != Some(branch) {
                items.push((
                    format!("Merge {branch} into this worktree"),
                    !busy,
                    merge_confirm(&path, branch),
                ));
                items.push((
                    format!("Rebase this worktree onto {branch}"),
                    !busy,
                    rebase_confirm(&path, branch),
                ));
            }
        }
        if used.is_none() {
            items.push((
                format!("Delete Branch {branch}…"),
                !busy,
                delete_branch_confirm(&repo.root, branch),
            ));
        }
        items.push((
            format!("Copy '{branch}'"),
            true,
            Action::Copy(branch.to_string()),
        ));
        menu_items(ui, items)
    }

    pub(super) fn remote_section(
        &self,
        ui: &mut Ui,
        p: &Palette,
        repo: &Repo,
        list: &BranchList,
        query: &str,
    ) -> Option<Action> {
        let shown = matching_remotes(list, query);
        let count: usize = shown.iter().map(|(_, branches)| branches.len()).sum();
        let id = Id::new(("remotes", &repo.root));
        let searching = !query.is_empty();
        section(ui, p, id, "Remotes", count, searching, |ui| {
            let mut action = None;
            for (remote, branches) in &shown {
                let text =
                    RichText::new(format!("{remote} ({})", branches.len())).color(p.sidebar_fg);
                let mut header = CollapsingHeader::new(text)
                    .id_salt(id.with(*remote))
                    .default_open(true);
                if searching {
                    header = header.open(Some(true));
                }
                let mut leaf = |ui: &mut Ui, label: &str, full: &str| {
                    self.remote_leaf(ui, p, repo, remote, label, full)
                };
                let tree = build_tree(branches);
                let picked = header
                    .show(ui, |ui| {
                        tree_rows(ui, p, &tree, id.with(*remote), searching, &mut leaf)
                    })
                    .body_returned
                    .flatten();
                action = action.or(picked);
            }
            action
        })
    }

    fn remote_leaf(
        &self,
        ui: &mut Ui,
        p: &Palette,
        repo: &Repo,
        remote: &str,
        label: &str,
        full: &str,
    ) -> Option<Action> {
        let selected = self
            .chosen_remote
            .as_ref()
            .is_some_and(|(name, item)| name == remote && item == full);
        let response =
            location_row(ui, p, label, selected, &[]).on_hover_text(format!("{remote}/{full}"));
        let mut menu = None;
        response.context_menu(|ui| menu = self.remote_menu(ui, repo, remote, full));
        let clicked = response.clicked().then(|| Action::PickRemote {
            remote: remote.to_string(),
            branch: full.to_string(),
        });
        menu.or(clicked)
    }

    /// Right click on a remote branch. Checkout is "Create Worktree" here.
    /// Merge and rebase use the remote-tracking name `remote/branch`.
    fn remote_menu(&self, ui: &mut Ui, repo: &Repo, remote: &str, branch: &str) -> Option<Action> {
        let name = format!("{remote}/{branch}");
        let busy = self.busy();
        let bare = repo.layout == Layout::Bare;
        let (root, remote_s, branch_s) =
            (repo.root.clone(), remote.to_string(), branch.to_string());
        let items = vec![
            (
                format!("Create Local Branch from {name}"),
                !busy && bare,
                Action::CreateBranch {
                    root: root.clone(),
                    remote: remote_s.clone(),
                    branch: branch_s.clone(),
                },
            ),
            (
                format!("Create Worktree for {name}"),
                !busy && bare,
                Action::CreateRemote {
                    root,
                    remote: remote_s,
                    branch: branch_s,
                },
            ),
            (
                format!("Delete {name}…"),
                !busy,
                delete_remote_confirm(&repo.root, remote, branch),
            ),
        ];
        let mut items = items;
        if let Some(path) = self.selected.clone() {
            items.push((
                format!("Merge {name} into this worktree"),
                !busy,
                merge_confirm(&path, &name),
            ));
            items.push((
                format!("Rebase this worktree onto {name}"),
                !busy,
                rebase_confirm(&path, &name),
            ));
        }
        items.push((format!("Copy '{name}'"), true, Action::Copy(name.clone())));
        let compare = self.compare_with(repo, &name);
        let items = items
            .into_iter()
            .chain(compare.map(|action| (format!("Compare with {name}"), !busy, action)))
            .collect();
        menu_items(ui, items)
    }

    pub(super) fn tag_section(
        &self,
        ui: &mut Ui,
        p: &Palette,
        repo: &Repo,
        list: &BranchList,
        query: &str,
    ) -> Option<Action> {
        let tags: Vec<_> = list
            .tags
            .iter()
            .filter(|tag| name_matches(&tag.name, query))
            .collect();
        let id = Id::new(("tags", &repo.root));
        section(ui, p, id, "Tags", tags.len(), !query.is_empty(), |ui| {
            first(tags.iter().map(|tag| {
                let selected = self
                    .chosen_tag
                    .as_ref()
                    .is_some_and(|(name, _)| name == &tag.name);
                location_row(ui, p, &tag.name, selected, &[])
                    .on_hover_text(&tag.commit)
                    .clicked()
                    .then(|| Action::PickTag {
                        name: tag.name.clone(),
                        commit: tag.commit.clone(),
                    })
            }))
        })
    }

    pub(super) fn stash_section(
        &self,
        ui: &mut Ui,
        p: &Palette,
        repo: &Repo,
        list: &BranchList,
        query: &str,
    ) -> Option<Action> {
        let stashes: Vec<_> = list
            .stashes
            .iter()
            .filter(|stash| name_matches(&stash.subject, query) || name_matches(&stash.id, query))
            .collect();
        let worktree = self
            .selected
            .clone()
            .or_else(|| first_openable(std::slice::from_ref(repo), &repo.root));
        let id = Id::new(("stashes", &repo.root));
        section(
            ui,
            p,
            id,
            "Stashes",
            stashes.len(),
            !query.is_empty(),
            |ui| {
                first(stashes.iter().map(|stash| {
                    let selected = self.chosen_stash.as_deref() == Some(stash.id.as_str());
                    let label = format!("{} {}", stash.id, stash.subject);
                    location_row(ui, p, &label, selected, &[])
                        .clicked()
                        .then(|| Action::PickStash {
                            id: stash.id.clone(),
                            worktree: worktree.clone(),
                        })
                }))
            },
        )
    }

    pub(super) fn submodule_section(
        &self,
        ui: &mut Ui,
        p: &Palette,
        repo: &Repo,
        list: &BranchList,
        query: &str,
    ) -> Option<Action> {
        let items: Vec<_> = list
            .submodules
            .iter()
            .filter(|item| name_matches(&item.path, query))
            .collect();
        let worktree = self
            .selected
            .clone()
            .or_else(|| first_openable(std::slice::from_ref(repo), &repo.root));
        let id = Id::new(("submodules", &repo.root));
        section(
            ui,
            p,
            id,
            "Submodules",
            items.len(),
            !query.is_empty(),
            |ui| {
                let worktree = worktree.as_ref()?;
                first(items.iter().map(|item| {
                    location_row(ui, p, &item.path, false, &[])
                        .on_hover_text(&item.commit)
                        .clicked()
                        .then(|| Action::OpenSubmodule {
                            worktree: worktree.clone(),
                            rel: item.path.clone(),
                        })
                }))
            },
        )
    }

    pub(super) fn worktree_section(
        &self,
        ui: &mut Ui,
        p: &Palette,
        repo: &Repo,
        query: &str,
    ) -> Option<Action> {
        let worktrees: Vec<&Worktree> = repo
            .worktrees
            .iter()
            .filter(|wt| openable(wt) && name_matches(&worktree_name(wt), query))
            .collect();
        let id = Id::new(("worktrees", &repo.root));
        section(
            ui,
            p,
            id,
            "Worktrees",
            worktrees.len(),
            !query.is_empty(),
            |ui| {
                first(
                    worktrees
                        .iter()
                        .map(|wt| self.worktree_row(ui, p, repo, wt)),
                )
            },
        )
    }

    pub(super) fn worktree_row(
        &self,
        ui: &mut Ui,
        p: &Palette,
        repo: &Repo,
        wt: &Worktree,
    ) -> Option<Action> {
        let name = worktree_name(wt);
        let selected = self.selected.as_deref() == Some(wt.path.as_path());
        let state = wt_status::state_of(wt, repo.layout == Layout::Bare);
        let tip = format!("{}\n{}", wt.path.display(), wt_status::describe(state));
        let response = status_row(ui, p, &name, selected, &[], Some(state)).on_hover_text(tip);
        let mut menu = None;
        response.context_menu(|ui| menu = self.worktree_menu(ui, repo, wt));
        let clicked = response.clicked().then(|| Action::Select(wt.path.clone()));
        menu.or(clicked)
    }

    /// Right click on a worktree: the same commands, names and checks as ⋯ in the toolbar.
    fn worktree_menu(&self, ui: &mut Ui, repo: &Repo, wt: &Worktree) -> Option<Action> {
        let blocked = crate::trust::is_blocked(&self.config, repo);
        let entries = worktree_entries(
            self.busy(),
            blocked,
            Some((repo, wt)),
            self.editors.current.name(),
        );
        menu_entries(ui, entries)
    }
}

/// Remotes with the branches that match `query`; a matching remote name keeps all its branches.
fn matching_remotes<'a>(list: &'a BranchList, query: &str) -> Vec<(&'a str, Vec<&'a str>)> {
    list.remotes
        .iter()
        .map(|remote| {
            let remote_hit = name_matches(&remote.name, query);
            let branches: Vec<&str> = remote
                .branches
                .iter()
                .map(String::as_str)
                .filter(|branch| remote_hit || name_matches(branch, query))
                .collect();
            (remote.name.as_str(), branches)
        })
        .filter(|(name, branches)| !branches.is_empty() || name_matches(name, query))
        .collect()
}

/// Buttons of a context menu; the clicked one closes the menu and returns its action.
fn menu_items(ui: &mut Ui, items: Vec<(String, bool, Action)>) -> Option<Action> {
    let mut picked = None;
    for (label, enabled, action) in items {
        if ui.add_enabled(enabled, Button::new(label)).clicked() {
            picked = Some(action);
            ui.close();
        }
    }
    picked
}

/// A collapsible section, open at first and forced open while the search box has text.
fn section<R>(
    ui: &mut Ui,
    p: &Palette,
    id: Id,
    title: &str,
    count: usize,
    searching: bool,
    add: impl FnOnce(&mut Ui) -> Option<R>,
) -> Option<R> {
    ui.add_space(2.0);
    let text = RichText::new(format!("{title} ({count})"))
        .size(11.5)
        .strong()
        .color(p.sidebar_heading_fg);
    // Empty sections start folded.
    let mut header = CollapsingHeader::new(text)
        .id_salt(id)
        .default_open(count > 0);
    if searching {
        header = header.open(Some(true));
    }
    header.show(ui, add).body_returned.flatten()
}

/// Rows of a branch tree: folders collapse (closed at first), leaves call `leaf`.
fn tree_rows(
    ui: &mut Ui,
    p: &Palette,
    nodes: &[Node<'_>],
    id: Id,
    searching: bool,
    leaf: &mut dyn FnMut(&mut Ui, &str, &str) -> Option<Action>,
) -> Option<Action> {
    let mut action = None;
    for node in nodes {
        let picked = match node {
            Node::Leaf { label, full } => leaf(ui, label, full),
            Node::Folder {
                name,
                path,
                children,
                count,
            } => {
                let text = RichText::new(format!("{name} ({count})")).color(p.sidebar_fg);
                let mut header = CollapsingHeader::new(text)
                    .id_salt(id.with(*path))
                    .default_open(false);
                if searching {
                    header = header.open(Some(true));
                }
                header
                    .show(ui, |ui| tree_rows(ui, p, children, id, searching, leaf))
                    .body_returned
                    .flatten()
            }
        };
        action = action.or(picked);
    }
    action
}

pub(super) fn section_header(ui: &mut Ui, p: &Palette, title: &str, count: usize) {
    section_title(ui, p, &format!("{title} ({count})"));
}

/// A section heading as it is, for a list that has no count (yet).
pub(super) fn section_title(ui: &mut Ui, p: &Palette, text: &str) {
    ui.add_space(8.0);
    ui.label(
        RichText::new(text)
            .size(12.0)
            .strong()
            .color(p.sidebar_heading_fg),
    );
    ui.add_space(2.0);
}

/// Shared title band for the two left columns.
pub(super) fn column_header(ui: &mut Ui, p: &Palette, title: &str) -> Rect {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, p.header_bg);
    ui.painter().text(
        pos2(rect.left() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(13.0),
        p.sidebar_fg,
    );
    ui.painter()
        .hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, p.border));
    rect
}

/// Width of the dark strip between the left columns and the next pane.
pub(super) const COLUMN_GAP: i8 = 8;

pub(super) fn paint_column_edge(ui: &Ui, p: &Palette, rect: Rect) {
    let gap = rect.right() - COLUMN_GAP as f32;
    let gutter = Rect::from_min_max(pos2(gap, rect.top()), rect.right_bottom());
    ui.painter().rect_filled(gutter, 0.0, p.bg);
    ui.painter()
        .vline(gap, rect.y_range(), Stroke::new(1.0, p.border));
}

pub(super) fn location_row(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    selected: bool,
    marks: &[Color32],
) -> egui::Response {
    status_row(ui, p, label, selected, marks, None)
}

/// A list row; `lead` puts a worktree state icon before the name. A name that does not fit is
/// shortened with `…` (in the middle when it has `/`), and the full name goes to the tooltip.
pub(super) fn status_row(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    selected: bool,
    marks: &[Color32],
    lead: Option<WtState>,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::click());
    let fill = if selected {
        p.row_selected_bg
    } else if response.hovered() {
        p.row_hover_bg
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 3.0, fill);
    }
    let color = if selected {
        p.row_selected_fg
    } else {
        p.sidebar_fg
    };
    let mut text_rect = rect.shrink2(Vec2::new(8.0, 1.0));
    if let Some(state) = lead {
        let at = pos2(text_rect.left() + wt_status::ICON_WIDTH / 2.0 - 2.0, rect.center().y);
        wt_status::paint(ui.painter(), p, at, state);
        text_rect.min.x += wt_status::ICON_WIDTH + 2.0;
    }
    // Marks sit at the right end; the name stops before them.
    text_rect.max.x -= marks.len() as f32 * 12.0;
    let font = FontId::proportional(13.0);
    let shown = fit(ui, label, &font, text_rect.width(), Elide::for_name(label));
    ui.painter().with_clip_rect(text_rect).text(
        text_rect.left_center(),
        Align2::LEFT_CENTER,
        &shown,
        font,
        color,
    );
    let mut x = rect.right() - 10.0;
    for mark_color in marks.iter().rev() {
        let center = egui::pos2(x, rect.center().y);
        ui.painter().circle_filled(center, 3.5, *mark_color);
        x -= 12.0;
    }
    if shown != label {
        return response.on_hover_text(label);
    }
    response
}

