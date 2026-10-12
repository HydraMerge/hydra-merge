//! Background jobs: start, poll, and apply their results to the window state.

use super::*;

impl WorktreeBarApp {
    pub(super) fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Runs `work` on a thread. At most one job runs at a time.
    pub(super) fn spawn(
        &mut self,
        ctx: &Context,
        work: impl FnOnce(Config, Vec<PathBuf>) -> JobResult + Send + 'static,
    ) {
        if self.busy() {
            return;
        }
        let (tx, rx) = channel();
        let (config, extra) = (self.config.clone(), self.extra_roots.clone());
        let ctx = ctx.clone();
        thread::spawn(move || {
            // The receiver is gone only when the window closed.
            let _ = tx.send(work(config, extra));
            ctx.request_repaint();
        });
        self.job = Some(rx);
    }

    pub(super) fn start_scan(&mut self, ctx: &Context) {
        self.spawn(ctx, |config, extra| {
            // The list comes first; `git status` of each worktree follows on the status lane.
            JobResult::Scanned(scan_all_with(&config, &extra, Status::Skip))
        });
    }

    /// Refreshes the repository that contains `path` without walking every discovery root.
    pub(super) fn start_reload(&mut self, ctx: &Context, path: PathBuf) {
        let repo = self
            .repos
            .iter()
            .filter(|repo| path.starts_with(&repo.root))
            .max_by_key(|repo| repo.root.components().count())
            .cloned();
        let Some(repo) = repo else {
            self.hold_status = false;
            return;
        };
        self.spawn(ctx, move |config, _| {
            JobResult::Reloaded(reload_repo(&config, &repo))
        });
    }

    pub(super) fn start_open(&mut self, ctx: &Context, path: PathBuf) {
        let editor = self.editors.current.clone();
        self.spawn(ctx, move |config, extra| {
            JobResult::Opened(open_worktree_with(&config, &extra, &path, &editor))
        });
    }

    pub(super) fn start_choose_editor(&mut self, ctx: &Context) {
        self.spawn(ctx, |_, _| JobResult::EditorAppChosen(choose_app()));
    }

    fn on_editor_app_chosen(&mut self, result: AppResult<Option<PathBuf>>) {
        match result {
            Ok(Some(path)) => match Editor::app(&path) {
                Some(editor) => self.set_editor(editor),
                None => self.set_error(AppError::new(
                    ErrorCode::MergeNotFound,
                    format!("Not an application: {}", path.display()),
                )),
            },
            Ok(None) => {}
            Err(e) => self.set_error(e),
        }
    }

    pub(super) fn set_editor(&mut self, editor: Editor) {
        self.status = format!("Open uses {}", editor.name());
        self.editors.choose(editor);
    }

    pub(super) fn start_remove(&mut self, ctx: &Context, path: PathBuf) {
        self.spawn(ctx, move |config, extra| {
            JobResult::Removed(remove_worktree_with(&config, &extra, &path))
        });
    }

    pub(super) fn start_create(&mut self, ctx: &Context, root: PathBuf, branch: String) {
        self.spawn(ctx, move |config, _| {
            JobResult::Created(create_worktree(&config, &root, &branch))
        });
    }

    pub(super) fn start_add_path(&mut self, ctx: &Context) {
        let Some(dialog) = self.open_dialog.as_mut() else {
            return;
        };
        dialog.error = None;
        let path = PathBuf::from(dialog.path.trim());
        self.spawn(ctx, move |config, _| {
            JobResult::PathAdded(open_path(&config, &path))
        });
    }

    pub(super) fn poll(&mut self, ctx: &Context) {
        let Some(rx) = &self.job else { return };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.job = None;
                self.set_error(AppError::new(
                    ErrorCode::Io,
                    "A background task stopped unexpectedly",
                ));
                return;
            }
        };
        self.job = None;
        self.apply_result(ctx, result);
    }

    pub(super) fn apply_result(&mut self, ctx: &Context, result: JobResult) {
        match result {
            JobResult::Scanned(r) => self.on_scanned(ctx, r),
            JobResult::Branches(root, r) => self.on_branches(root, r),
            JobResult::Opened(r) => self.finish(r, "Opened", false, ctx),
            JobResult::Removed(r) => self.finish(r, "Removed", true, ctx),
            JobResult::Created(r) => self.on_created(ctx, r),
            JobResult::PathAdded(r) => self.on_path_added(r),
            JobResult::Reloaded(r) => self.on_reloaded(r),
            JobResult::View(path, r) => self.on_view(path, r),
            JobResult::Patch(path, hash, r) => self.on_patch(path, hash, r),
            JobResult::Changed(r) => self.on_changed(ctx, r),
            JobResult::Pushed(r) => self.on_pushed(ctx, r),
            JobResult::Done(r) => self.on_done(r),
            JobResult::FolderChosen(r) => self.on_folder_chosen(r),
            JobResult::EditorAppChosen(r) => self.on_editor_app_chosen(r),
            JobResult::Compared(key, r) => self.on_compared(key, r),
            JobResult::Searched(path, query, r) => self.on_searched(path, query, r),
        }
    }

    pub(super) fn start_change(
        &mut self,
        ctx: &Context,
        status: String,
        clear_message: bool,
        select_newest: bool,
        work: impl FnOnce(&Config, &[PathBuf]) -> AppResult<PathBuf> + Send + 'static,
    ) {
        self.spawn(ctx, move |config, extra| {
            let result = work(&config, &extra).map(|path| ChangeDone {
                status,
                path,
                clear_message,
                select_newest,
            });
            JobResult::Changed(result)
        });
    }

    pub(super) fn start_push(&mut self, ctx: &Context, path: PathBuf) {
        self.spawn(ctx, move |config, extra| {
            JobResult::Pushed(push_worktree(&config, &extra, &path))
        });
    }

    /// Like any write, plus the server's link (GitLab: "create a merge request") kept for a button.
    fn on_pushed(&mut self, ctx: &Context, result: AppResult<PushDone>) {
        match result {
            Ok(done) => {
                self.panes.link = done.link;
                self.error = None;
                self.status = format!("Pushed {}", done.path.display());
                self.hold_status = true;
                self.start_reload(ctx, done.path);
            }
            Err(e) => self.set_error(e),
        }
    }

    pub(super) fn on_changed(&mut self, ctx: &Context, result: AppResult<ChangeDone>) {
        match result {
            Ok(done) => {
                self.error = None;
                self.status = done.status;
                self.hold_status = true;
                if done.clear_message {
                    self.commit_message.clear();
                }
                if done.select_newest {
                    self.open_commit = None;
                    self.patch = None;
                }
                self.start_reload(ctx, done.path);
            }
            Err(e) => self.set_error(e),
        }
    }

    /// Shows a result path in the status bar; `rescan` refreshes the list afterwards.
    pub(super) fn finish(
        &mut self,
        result: AppResult<PathBuf>,
        verb: &str,
        rescan: bool,
        ctx: &Context,
    ) {
        match result {
            Ok(path) => {
                self.error = None;
                self.status = format!("{verb} {}", path.display());
                if rescan {
                    self.hold_status = true;
                    self.start_reload(ctx, path);
                }
            }
            Err(e) => self.set_error(e),
        }
    }

    pub(super) fn on_created(&mut self, ctx: &Context, result: AppResult<Worktree>) {
        match result {
            Ok(wt) => {
                self.status = format!("Created {}", wt.path.display());
                self.error = None;
                self.hold_status = true;
                self.selected = Some(wt.path.clone());
                self.start_reload(ctx, wt.path);
            }
            Err(e) => self.set_error(e),
        }
    }

    pub(super) fn on_path_added(&mut self, result: AppResult<Repo>) {
        match result {
            Ok(repo) => {
                if !self.extra_roots.contains(&repo.root) {
                    self.extra_roots = [self.extra_roots.clone(), vec![repo.root.clone()]].concat();
                }
                self.open_dialog = None;
                self.status = format!("Added {}", repo.name);
                self.error = None;
                self.selected = repo
                    .worktrees
                    .iter()
                    .find(|wt| openable(wt))
                    .map(|wt| wt.path.clone());
                self.active_root = Some(repo.root.clone());
                self.reveal_tab = true;
                self.clear_location_picks();
                self.replace_repo(repo);
            }
            // While the window is open the reason shows there, next to the path.
            Err(e) => match self.open_dialog.as_mut() {
                Some(dialog) => dialog.error = Some(e.message),
                None => self.set_error(e),
            },
        }
    }

    /// The folder picker starts in the first discovery root (or the typed folder when it exists).
    pub(super) fn start_choose_folder(&mut self, ctx: &Context) {
        let typed = self
            .open_dialog
            .as_ref()
            .map(|d| PathBuf::from(d.path.trim()))
            .filter(|p| p.is_absolute() && p.is_dir());
        let start = typed
            .or_else(|| self.config.roots.first().cloned())
            .unwrap_or_else(|| PathBuf::from("/"));
        self.spawn(ctx, move |_, _| {
            JobResult::FolderChosen(choose_folder(&start))
        });
    }

    pub(super) fn on_folder_chosen(&mut self, result: AppResult<Option<PathBuf>>) {
        let Some(dialog) = self.open_dialog.as_mut() else {
            return;
        };
        match result {
            Ok(Some(path)) => {
                dialog.path = path.display().to_string();
                dialog.error = None;
            }
            Ok(None) => {}
            Err(e) => dialog.error = Some(e.message),
        }
    }

    pub(super) fn on_reloaded(&mut self, result: AppResult<Repo>) {
        match result {
            Ok(repo) => {
                self.hold_status = false;
                self.error = None;
                self.replace_repo(repo);
            }
            Err(e) => {
                self.hold_status = false;
                self.set_error(e);
            }
        }
    }

    pub(super) fn replace_repo(&mut self, repo: Repo) {
        let root = repo.root.clone();
        self.watch_baseline(&repo);
        let mut repos: Vec<Repo> = self
            .repos
            .iter()
            .filter(|known| known.root != repo.root)
            .cloned()
            .collect();
        repos.push(repo);
        repos.sort_by_key(|item| item.name.to_lowercase());
        let snapshot: Arc<[Repo]> = Arc::from(repos);
        let keep = self
            .selected
            .as_deref()
            .is_some_and(|path| find_selectable(&snapshot, path));
        self.selected = if keep { self.selected.take() } else { None };
        self.status_lane_skip(&root);
        self.repos = snapshot;
        self.branches = None;
        self.view = None;
        self.invalidate();
        self.sync_active();
    }

    pub(super) fn on_scanned(&mut self, ctx: &Context, result: AppResult<Vec<Repo>>) {
        match result {
            Ok(repos) => {
                let snapshot: Arc<[Repo]> = Arc::from(repos);
                if self.hold_status {
                    self.hold_status = false;
                } else {
                    self.status =
                        format!("Scanned {} repositories{STATUS_PENDING}", snapshot.len());
                }
                self.error = None;
                let keep = self
                    .selected
                    .as_deref()
                    .is_some_and(|p| find_selectable(&snapshot, p));
                self.selected = if keep { self.selected.take() } else { None };
                self.repos = snapshot;
                self.branches = None;
                self.view = None;
                self.invalidate();
                // New repositories, new reference points for the outside-change watch.
                self.forget_fingerprints();
                self.apply_pending_restore();
                self.sync_active();
                self.start_status_lane(ctx);
            }
            Err(e) => {
                self.hold_status = false;
                self.set_error(e);
            }
        }
    }

    pub(super) fn on_branches(&mut self, repo_root: PathBuf, result: AppResult<BranchList>) {
        // A failure keeps an empty list, so it is not retried every frame.
        let list = match result {
            Ok(list) => list,
            Err(e) => {
                self.set_error(e);
                BranchList::default()
            }
        };
        self.keep_branches(&repo_root, &list);
        if self.active_root.as_deref() == Some(repo_root.as_path()) {
            self.show_branches(repo_root, list);
        }
    }

    /// Shows `list`, dropping picks that are no longer in it.
    fn show_branches(&mut self, repo_root: PathBuf, list: BranchList) {
        if !list.available.contains(&self.chosen_branch) {
            self.chosen_branch.clear();
        }
        let remote_ok = self.chosen_remote.as_ref().is_some_and(|(remote, branch)| {
            list.remotes
                .iter()
                .any(|item| &item.name == remote && item.branches.iter().any(|name| name == branch))
        });
        if self.chosen_remote.is_some() && !remote_ok {
            self.chosen_remote = None;
        }
        let tag_ok = self
            .chosen_tag
            .as_ref()
            .is_some_and(|(name, _)| list.tags.iter().any(|tag| &tag.name == name));
        if self.chosen_tag.is_some() && !tag_ok {
            self.chosen_tag = None;
        }
        let stash_ok = self
            .chosen_stash
            .as_ref()
            .is_some_and(|id| list.stashes.iter().any(|stash| &stash.id == id));
        if self.chosen_stash.is_some() && !stash_ok {
            self.chosen_stash = None;
        }
        self.branches = Some(BranchState { repo_root, list });
    }

    pub(super) fn on_view(&mut self, path: PathBuf, result: AppResult<Arc<ViewData>>) {
        self.keep_view(&path, &result);
        // The selection moved on while this was loading: it stays in the cache only.
        if self.selected.as_deref() == Some(path.as_path()) {
            self.show_view(path, result);
        }
    }

    pub(super) fn show_view(&mut self, path: PathBuf, result: AppResult<Arc<ViewData>>) {
        // A commit stays open while it is still listed; otherwise the working copy row is
        // selected. A failed read stays in the panel.
        // A commit found by "Search all history" stays open although it is older than the list.
        let found = |hash: &String| {
            self.search_hits()
                .and_then(|r| r.as_ref().ok())
                .is_some_and(|hits| hits.iter().any(|c| &c.hash == hash))
        };
        let still = match &result {
            Ok(data) => self.open_commit.as_ref().is_some_and(|hash| {
                data.view.commits.iter().any(|commit| &commit.hash == hash) || found(hash)
            }),
            Err(_) => false,
        };
        // Picked files keep their paths; an index from the older list would point elsewhere.
        self.panes.selection_anchor = None;
        if !still {
            self.open_commit = None;
            self.patch = None;
        }
        self.view = Some(ViewState { path, result });
    }

    pub(super) fn on_patch(&mut self, path: PathBuf, hash: String, diff: AppResult<Arc<DiffView>>) {
        self.keep_patch(&hash, &diff);
        if self.open_commit.as_deref() == Some(hash.as_str()) {
            self.patch = Some(PatchState { path, hash, diff });
        }
    }

    pub(super) fn set_error(&mut self, error: AppError) {
        eprintln!("error: {:?}: {}", error.code, error.message);
        self.error = Some(error);
    }

    /// Keeps the open tab on a repository that still exists, and selects its first worktree.
    pub(super) fn sync_active(&mut self) {
        let repos = Arc::clone(&self.repos);
        let still = self
            .active_root
            .as_ref()
            .is_some_and(|root| repos.iter().any(|repo| &repo.root == root));
        if !still {
            self.active_root = repos
                .iter()
                .find(|repo| !self.closed_tabs.contains(&repo.root))
                .map(|repo| repo.root.clone());
            self.reveal_tab = true;
            self.clear_location_picks();
            self.selected = self
                .active_root
                .as_ref()
                .and_then(|root| first_openable(&repos, root));
            return;
        }
        if let Some(path) = self.selected.clone() {
            if let Some(repo) = repo_of(&repos, &path) {
                self.active_root = Some(repo.root.clone());
            }
        } else if !self.has_location_pick() {
            if let Some(root) = self.active_root.clone() {
                self.selected = first_openable(&repos, &root);
            }
        }
    }

    /// Runs an action that was asked for while another job was busy.
    pub(super) fn run_queued(&mut self, ctx: &Context) {
        if self.busy() {
            return;
        }
        if let Some(action) = self.queued.take() {
            self.apply(ctx, action);
        }
    }

    pub(super) fn active_repo<'a>(&self, repos: &'a [Repo]) -> Option<&'a Repo> {
        let root = self.active_root.as_deref()?;
        repos.iter().find(|repo| repo.root == root)
    }

    // ---- theme ---------------------------------------------------------
}
