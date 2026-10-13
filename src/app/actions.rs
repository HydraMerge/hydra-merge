//! What the UI asked for, applied after drawing; keyboard and theme toggles.

use super::*;

impl WorktreeBarApp {
    pub(super) fn toggle_autoscan(&mut self, ctx: &Context) {
        self.autoscan = !self.autoscan;
        if let Some(dir) = &self.theme_dir {
            if let Err(error) = theme::save_autoscan(dir, self.autoscan) {
                self.set_error(error);
            }
        }
        if self.autoscan {
            // Through `apply`, so a scan asked for during a load waits for it.
            self.apply(ctx, Action::Rescan);
        } else {
            self.status = "Auto-scan is off".to_string();
        }
    }

    pub(super) fn toggle_theme(&mut self, ctx: &Context) {
        let next = ThemeChoice::from_egui(ctx.theme()).opposite();
        // The light/dark switch goes back to the Merge skins, which follow it.
        if self.skins.chosen != crate::skin::AUTO {
            self.set_skin(ctx, crate::skin::AUTO.to_string());
        }
        ctx.set_theme(next.to_preference());
        let saved = match &self.theme_dir {
            Some(dir) => theme::save_theme_choice(dir, next),
            None if self.theme_warned => Ok(()),
            None => Err(AppError::new(
                ErrorCode::Io,
                "The home directory is not set: the theme choice is not saved",
            )),
        };
        self.theme_warned = self.theme_dir.is_none();
        if let Err(e) = saved {
            self.set_error(e);
        }
    }

    // ---- actions -------------------------------------------------------

    pub(super) fn apply(&mut self, ctx: &Context, action: Action) {
        let Some(action) = self.unless_busy(action) else {
            return;
        };
        match action {
            Action::Select(_)
            | Action::PickBranch(_)
            | Action::PickCommit(_)
            | Action::PickWorkingCopy
            | Action::PickRemote { .. }
            | Action::PickTag { .. }
            | Action::PickStash { .. } => self.pick(action),
            Action::CloseTab(_)
            | Action::ToggleLocations
            | Action::NavBack
            | Action::NavForward
            | Action::OpenPalette
            | Action::FindInHistory
            | Action::FindRepository
            | Action::OpenRepoDialog
            | Action::CloseRepoDialog => self.window_action(action),
            Action::Activate(root) => self.activate(root),
            Action::TrustRepo(root) => self.set_trust(ctx, root, true),
            Action::UntrustRepo(root) => self.set_trust(ctx, root, false),
            Action::Open(path) => self.start_open(ctx, path),
            Action::Ask(confirm) => self.confirm = Some(confirm),
            Action::Remove(path) => self.start_remove(ctx, path),
            Action::Create(root, branch) => self.start_create(ctx, root, branch),
            Action::CreateRemote {
                root,
                remote,
                branch,
            } => self.start_create_remote(ctx, root, remote, branch),
            Action::Rescan => self.start_scan(ctx),
            Action::ChooseScanFolder => self.start_choose_scan_folder(ctx),
            Action::ResetScanFolder => self.reset_scan_folder(ctx),
            Action::SetLogLevel(level) => self.set_log_level(level),
            Action::ShowLogPath => self.show_log_path(),
            Action::CheckUpdate => self.schedule_update_check(ctx, true),
            Action::InstallUpdate => self.install_update(ctx),
            Action::DismissUpdate => self.dismiss_update(),
            Action::ToggleAutoscan => self.toggle_autoscan(ctx),
            Action::ToggleTheme => self.toggle_theme(ctx),
            Action::DismissError => self.error = None,
            Action::Commit(path) => self.commit(ctx, path),
            Action::Push(path) => self.start_push(ctx, path),
            tool if is_tool(&tool) => self.tool_action(ctx, tool),
            write => self.write(ctx, write),
        }
    }

    /// Records the user's decision about a repository, then reads everything again: the trusted
    /// repository's status and history load, a revoked one goes back to the question.
    fn set_trust(&mut self, ctx: &Context, root: PathBuf, trust: bool) {
        let Some(repo) = self.repos.iter().find(|repo| repo.root == root).cloned() else {
            return;
        };
        if trust {
            // The fingerprint of what the window showed: a config changed since then no longer
            // matches and is asked about again.
            self.config.trusted.trust(&repo.root, &repo.trust_key);
            self.status = format!("Trusted {}", repo.name);
        } else {
            self.config.trusted.revoke(&repo.root);
            self.status = format!("No longer trusting {}", repo.name);
        }
        self.hold_status = true;
        self.apply(ctx, Action::Rescan);
    }

    fn set_log_level(&mut self, level: crate::log::Level) {
        crate::log::set_level(level);
        crate::log_info!("log level {}", level.as_str());
        self.status = format!("Log level: {}", level.as_str());
        self.hold_status = true;
    }

    fn show_log_path(&mut self) {
        self.status = match crate::log::path() {
            Some(path) => format!("Log: {}", tilde_path(&path)),
            None => "Log file is not open".to_string(),
        };
        self.hold_status = true;
    }

    /// One job runs at a time: an action that needs one waits while another runs, instead of
    /// being lost. `None` when it was put in the queue.
    fn unless_busy(&mut self, action: Action) -> Option<Action> {
        if !(self.busy() && needs_job(&action)) {
            return Some(action);
        }
        self.status = "Waiting for the current task…".to_string();
        self.queued = Some(action);
        None
    }

    /// Outside apps and links, comparison, history search and auto-fetch.
    fn tool_action(&mut self, ctx: &Context, action: Action) {
        match action {
            Action::SetSkin(name) => self.set_skin(ctx, name),
            Action::SetEditor(editor) => self.set_editor(editor),
            Action::ChooseEditorApp => self.start_choose_editor(ctx),
            Action::CreateSkin => self.create_skin(ctx),
            Action::OpenSkinsFolder => self.open_skins_folder(),
            Action::AddPath => self.start_add_path(ctx),
            Action::ChooseFolder => self.start_choose_folder(ctx),
            Action::Copy(text) => {
                self.status = format!("Copied {text}");
                ctx.copy_text(text);
            }
            Action::OpenSubmodule { worktree, rel } => {
                let editor = self.editors.current.clone();
                self.spawn(ctx, move |config, extra| {
                    let opened = open_child_dir_with(&config, &extra, &worktree, &rel, &editor);
                    JobResult::Opened(opened)
                });
            }
            Action::OpenTerminal(_) | Action::RevealInFinder(_) | Action::OpenLink(_) => {
                self.start_outside(ctx, action)
            }
            Action::DismissLink => self.panes.link = None,
            Action::Compare(key) => self.start_compare(ctx, key),
            Action::CloseCompare => self.panes.compare = None,
            Action::SearchHistory(query) => self.start_search(ctx, query),
            Action::ToggleAutoFetch => {
                self.auto_fetch = !self.auto_fetch;
                let state = if self.auto_fetch {
                    "on: every 5 minutes"
                } else {
                    "off"
                };
                self.status = format!("Auto-fetch {state}");
            }
            _ => {}
        }
    }

    /// Tabs, panels, the palette and back / forward: window state only, no git.
    fn window_action(&mut self, action: Action) {
        match action {
            Action::CloseTab(root) => self.close_tab(root),
            Action::ToggleLocations => self.locations_open = !self.locations_open,
            Action::CloseRepoDialog => self.open_dialog = None,
            Action::OpenRepoDialog => {
                self.open_dialog = Some(OpenDialog {
                    focus: true,
                    ..OpenDialog::default()
                });
            }
            Action::OpenPalette => self.palette = Some(palette::PaletteState::default()),
            Action::FindInHistory => self.open_history_filter(),
            Action::FindRepository => {
                self.panes.repo_search_open = true;
                self.panes.repo_focus = true;
            }
            Action::NavBack => {
                if let Some(place) = self.nav.back(self.place()) {
                    self.go_to(place);
                }
            }
            Action::NavForward => {
                if let Some(place) = self.nav.forward(self.place()) {
                    self.go_to(place);
                }
            }
            _ => {}
        }
    }

    /// Selection in Locations, history or tabs. Nothing runs in the background.
    /// Selection in Locations or the commit list. The shown worktree stays: a picked branch,
    /// tag or stash opens in the right pane next to its commit list.
    fn pick(&mut self, action: Action) {
        self.clear_location_picks();
        match action {
            Action::PickWorkingCopy => {
                self.open_commit = None;
                self.patch = None;
            }
            Action::Select(path) => self.select(path),
            Action::PickBranch(name) => self.chosen_branch = name,
            Action::PickCommit(hash) => {
                if self.open_commit.as_deref() != Some(hash.as_str()) {
                    self.patch = None;
                    self.open_commit = Some(hash);
                }
            }
            Action::PickRemote { remote, branch } => self.chosen_remote = Some((remote, branch)),
            Action::PickTag { name, commit } => self.chosen_tag = Some((name, commit)),
            Action::PickStash { id, worktree } => {
                self.stash_worktree = worktree;
                self.chosen_stash = Some(id);
            }
            _ => {}
        }
    }

    fn select(&mut self, path: PathBuf) {
        if let Some(repo) = repo_of(&self.repos, &path) {
            self.active_root = Some(repo.root.clone());
        }
        if self.selected.as_ref() != Some(&path) {
            self.view = None;
            self.open_commit = None;
            self.patch = None;
            // Shown from the cache at once, read again in the background.
            self.refresh_view(&path);
            self.panes.file_selection.clear();
            self.panes.selection_anchor = None;
        }
        self.selected = Some(path);
    }

    fn commit(&mut self, ctx: &Context, path: PathBuf) {
        let message = self.commit_message.clone();
        if message.trim().is_empty() {
            self.set_error(AppError::new(
                ErrorCode::GitFailed,
                "Commit message is empty",
            ));
            return;
        }
        let status = format!("Committed {}", path.display());
        self.start_change(ctx, status, true, true, move |config, extra| {
            commit_worktree(config, extra, &path, &message)
        });
    }

    /// Writes to a worktree or a remote; afterwards only that repository is reloaded.
    fn write(&mut self, ctx: &Context, action: Action) {
        if let Action::DropStash { .. } = action {
            self.chosen_stash = None;
        }
        if let Action::DeleteRemote { .. } = action {
            self.chosen_remote = None;
        }
        // Picked files are staged, unstaged or discarded now; the lists change under them.
        self.panes.file_selection.clear();
        self.panes.selection_anchor = None;
        if let Some((status, work)) = write_job(action) {
            self.start_change(ctx, status, false, false, work);
        }
    }

    /// What the window shows now, for the back / forward history.
    pub(super) fn place(&self) -> nav::Place {
        nav::Place {
            root: self.active_root.clone(),
            selected: self.selected.clone(),
            commit: self.open_commit.clone(),
            branch: self.chosen_branch.clone(),
            remote: self.chosen_remote.clone(),
            tag: self.chosen_tag.clone(),
            stash: self.chosen_stash.clone(),
        }
    }

    /// Shows `place` again. Data that belongs to another worktree or repository is reloaded.
    fn go_to(&mut self, place: nav::Place) {
        if place.root != self.active_root {
            self.branches = None;
            if let Some(root) = &place.root {
                self.refresh_branches(root);
            }
            self.reveal_tab = true;
            if let Some(root) = &place.root {
                self.closed_tabs.retain(|closed| closed != root);
            }
        }
        if place.selected != self.selected {
            self.view = None;
            if let Some(path) = &place.selected {
                self.refresh_view(path);
            }
        }
        if place.commit != self.open_commit {
            self.patch = None;
        }
        self.active_root = place.root;
        self.selected = place.selected;
        self.open_commit = place.commit;
        self.chosen_branch = place.branch;
        self.chosen_remote = place.remote;
        self.chosen_tag = place.tag;
        self.chosen_stash = place.stash;
    }

    /// The window title shows the open worktree path.
    pub(super) fn update_title(&mut self, ctx: &Context, repos: &[Repo]) {
        let title = self
            .active_repo(repos)
            .and_then(|repo| self.selected_worktree(repo))
            .map_or_else(|| "HydraMerge".to_string(), |wt| tilde_path(&wt.path));
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
    }

    pub(super) fn has_location_pick(&self) -> bool {
        !self.chosen_branch.is_empty()
            || self.chosen_remote.is_some()
            || self.chosen_tag.is_some()
            || self.chosen_stash.is_some()
    }

    pub(super) fn clear_location_picks(&mut self) {
        self.panes.compare = None;
        self.chosen_branch.clear();
        self.chosen_remote = None;
        self.chosen_tag = None;
        self.chosen_stash = None;
    }

    pub(super) fn start_create_remote(
        &mut self,
        ctx: &Context,
        root: PathBuf,
        remote: String,
        branch: String,
    ) {
        self.spawn(ctx, move |config, _| {
            JobResult::Created(create_tracking_worktree(&config, &root, &remote, &branch))
        });
    }

    pub(super) fn handle_keys(&mut self, ctx: &Context, repos: &[Repo]) -> Option<Action> {
        if let Some(action) = global_keys(ctx) {
            return Some(action);
        }
        if ctx.egui_wants_keyboard_input() {
            return None;
        }
        let (rescan, down, up, enter) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::R),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
            )
        });
        if rescan {
            return Some(Action::Rescan);
        }
        if down || up {
            let step = if down { 1 } else { -1 };
            let active = self.active_repo(repos);
            return neighbour(active, &self.query, self.selected.as_deref(), step)
                .map(Action::Select);
        }
        if !enter {
            return None;
        }
        if let Some(path) = &self.selected {
            return Some(Action::Open(path.clone()));
        }
        if self.chosen_branch.is_empty() {
            return None;
        }
        self.active_root
            .clone()
            .map(|root| Action::Create(root, self.chosen_branch.clone()))
    }

    /// Hides the tab; the next open tab becomes active when it was the active one.
    fn close_tab(&mut self, root: PathBuf) {
        if !self.closed_tabs.contains(&root) {
            self.closed_tabs.push(root.clone());
        }
        if self.active_root.as_ref() != Some(&root) {
            return;
        }
        let next = self
            .repos
            .iter()
            .find(|repo| !self.closed_tabs.contains(&repo.root))
            .map(|repo| repo.root.clone());
        self.active_root = None;
        self.selected = None;
        self.view = None;
        self.branches = None;
        self.clear_location_picks();
        if let Some(next) = next {
            self.activate(next);
        }
    }

    pub(super) fn activate(&mut self, root: PathBuf) {
        self.closed_tabs.retain(|closed| closed != &root);
        self.reveal_tab = true;
        let same = self.active_root.as_ref() == Some(&root);
        self.active_root = Some(root.clone());
        self.clear_location_picks();
        if !same {
            self.branches = None;
            self.refresh_branches(&root);
        }
        let stays = self
            .selected
            .as_ref()
            .is_some_and(|path| repo_of(&self.repos, path).is_some_and(|repo| repo.root == root));
        if !stays {
            self.selected = first_openable(&self.repos, &root);
            self.view = None;
            if let Some(path) = self.selected.clone() {
                self.refresh_view(&path);
            }
        }
    }
}

type WriteWork = Box<dyn FnOnce(&Config, &[PathBuf]) -> AppResult<PathBuf> + Send>;

/// Status text and git work of a write action; `None` for any other action.
fn write_job(action: Action) -> Option<(String, WriteWork)> {
    match action {
        Action::Stage { .. }
        | Action::StagePaths { .. }
        | Action::UnstagePaths { .. }
        | Action::DiscardPaths { .. }
        | Action::StageAll(_)
        | Action::Unstage { .. }
        | Action::Discard { .. } => file_job(action),
        _ => repo_job(action),
    }
}

/// Stage, unstage and discard of files in one worktree.
fn file_job(action: Action) -> Option<(String, WriteWork)> {
    let job: (String, WriteWork) = match action {
        Action::StagePaths { path, rels } => (
            format!("Staged {} item(s)", rels.len()),
            Box::new(move |c, e| {
                let rels: Vec<&str> = rels.iter().map(String::as_str).collect();
                stage_paths(c, e, &path, &rels)
            }),
        ),
        Action::UnstagePaths { path, rels } => (
            format!("Unstaged {} file(s)", rels.len()),
            Box::new(move |c, e| {
                let rels: Vec<&str> = rels.iter().map(String::as_str).collect();
                unstage_paths(c, e, &path, &rels)
            }),
        ),
        Action::DiscardPaths { path, rels } => (
            format!("Discarded {} file(s)", rels.len()),
            Box::new(move |c, e| {
                let rels: Vec<&str> = rels.iter().map(String::as_str).collect();
                discard_paths(c, e, &path, &rels)
            }),
        ),
        Action::Stage { path, rel } => (
            format!("Staged {rel}"),
            Box::new(move |c, e| stage_file(c, e, &path, Some(&rel))),
        ),
        Action::StageAll(path) => (
            "Staged all changes".to_string(),
            Box::new(move |c, e| stage_file(c, e, &path, None)),
        ),
        Action::Unstage { path, rel, orig } => (
            format!("Unstaged {rel}"),
            Box::new(move |c, e| {
                let rels: Vec<&str> = std::iter::once(rel.as_str())
                    .chain(orig.as_deref())
                    .collect();
                unstage_paths(c, e, &path, &rels)
            }),
        ),
        Action::Discard { path, rel } => (
            format!("Discarded {rel}"),
            Box::new(move |c, e| discard_file(c, e, &path, &rel)),
        ),
        _ => return None,
    };
    Some(job)
}

/// Network sync, stashes and remote branches.
fn repo_job(action: Action) -> Option<(String, WriteWork)> {
    let job: (String, WriteWork) = match action {
        Action::Pull(path) => (
            format!("Pulled {}", path.display()),
            Box::new(move |c, e| pull_worktree(c, e, &path)),
        ),
        Action::Fetch(path) => (
            format!("Fetched {}", path.display()),
            Box::new(move |c, e| fetch_worktree(c, e, &path)),
        ),
        Action::ApplyStash { worktree, id } => (
            format!("Applied {id}"),
            Box::new(move |c, e| apply_stash(c, e, &worktree, &id)),
        ),
        Action::DropStash { worktree, id } => (
            format!("Dropped {id}"),
            Box::new(move |c, e| drop_stash(c, e, &worktree, &id)),
        ),
        Action::CreateBranch {
            root,
            remote,
            branch,
        } => (
            format!("Created branch {branch}"),
            Box::new(move |c, _| create_tracking_branch(c, &root, &remote, &branch)),
        ),
        Action::DeleteRemote {
            root,
            remote,
            branch,
        } => (
            format!("Deleted {remote}/{branch}"),
            Box::new(move |c, _| delete_remote_branch(c, &root, &remote, &branch)),
        ),
        _ => return None,
    };
    Some(job)
}

/// Palette, back / forward, find, and the log path, even while typing.
fn global_keys(ctx: &Context) -> Option<Action> {
    let shift_command = Modifiers::COMMAND | Modifiers::SHIFT;
    if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::F)) {
        return Some(Action::FindInHistory);
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::L)) {
        return Some(Action::FindRepository);
    }
    let (palette, back, forward, log_path) = ctx.input_mut(|i| {
        (
            i.consume_key(shift_command, Key::P),
            i.consume_key(Modifiers::COMMAND, Key::OpenBracket),
            i.consume_key(Modifiers::COMMAND, Key::CloseBracket),
            i.consume_key(shift_command, Key::L),
        )
    });
    if palette {
        Some(Action::OpenPalette)
    } else if log_path {
        Some(Action::ShowLogPath)
    } else if back {
        Some(Action::NavBack)
    } else {
        forward.then_some(Action::NavForward)
    }
}

/// Outside apps, links, comparison, search, skins, copying: handled by `tool_action`.
fn is_tool(action: &Action) -> bool {
    matches!(
        action,
        Action::OpenTerminal(_)
            | Action::RevealInFinder(_)
            | Action::OpenLink(_)
            | Action::DismissLink
            | Action::Compare(_)
            | Action::CloseCompare
            | Action::SearchHistory(_)
            | Action::ToggleAutoFetch
            | Action::Copy(_)
            | Action::SetSkin(_)
            | Action::CreateSkin
            | Action::OpenSkinsFolder
            | Action::SetEditor(_)
            | Action::ChooseEditorApp
            | Action::AddPath
            | Action::ChooseFolder
            | Action::OpenSubmodule { .. }
    )
}

/// Actions that start a background job (and so must wait while another one runs).
fn needs_job(action: &Action) -> bool {
    !matches!(
        action,
        Action::Select(_)
            | Action::Activate(_)
            | Action::TrustRepo(_)
            | Action::UntrustRepo(_)
            | Action::PickBranch(_)
            | Action::PickCommit(_)
            | Action::PickWorkingCopy
            | Action::CloseTab(_)
            | Action::ToggleLocations
            | Action::NavBack
            | Action::NavForward
            | Action::OpenPalette
            | Action::FindInHistory
            | Action::FindRepository
            | Action::OpenRepoDialog
            | Action::CloseRepoDialog
            | Action::PickRemote { .. }
            | Action::PickTag { .. }
            | Action::PickStash { .. }
            | Action::Ask(_)
            | Action::ToggleAutoscan
            | Action::ToggleTheme
            | Action::DismissError
            | Action::Copy(_)
            | Action::SetSkin(_)
            | Action::CreateSkin
            | Action::OpenSkinsFolder
            | Action::SetEditor(_)
            | Action::DismissLink
            | Action::CloseCompare
            | Action::ToggleAutoFetch
    )
}
