//! Background work besides the action job, so a click never waits behind it:
//! - the load lane reads branches, history and patches of the open tab, one at a time;
//! - the status lane reads `git status` of every worktree after a scan (the "index").
//!
//! Loaded data is kept per repository / worktree / commit. Coming back to a tab shows the kept
//! data at once while the lane reads it again; a write or a scan starts a new generation, and
//! answers of an older generation are dropped.

use std::collections::{HashMap, HashSet};

use super::*;
use crate::discover::{apply_statuses, read_statuses};
use crate::trust::is_blocked;

/// Entries kept per cache; past that the cache starts over (what is on screen stays).
const CACHE_LIMIT: usize = 64;

#[derive(Default)]
pub(super) struct Caches {
    branches: HashMap<PathBuf, BranchList>,
    views: HashMap<PathBuf, AppResult<Arc<ViewData>>>,
    /// Commits do not change, so patches are kept by hash and never read twice.
    patches: HashMap<String, AppResult<Arc<DiffView>>>,
    /// Loads read from git in this generation; the others are read again in the background.
    fresh: HashSet<String>,
}

/// Status of every worktree, read after a scan without blocking the window.
pub(super) struct StatusLane {
    rx: Receiver<HashMap<PathBuf, Option<bool>>>,
    /// Repositories reloaded while it ran: their own status is newer than the lane's.
    newer: Vec<PathBuf>,
}

fn insert_bounded<K: std::hash::Hash + Eq, V>(map: &mut HashMap<K, V>, key: K, value: V) {
    if map.len() >= CACHE_LIMIT {
        map.clear();
    }
    map.insert(key, value);
}

fn branches_key(root: &Path) -> String {
    format!("branches {}", root.display())
}

fn view_key(path: &Path) -> String {
    format!("view {}", path.display())
}

impl WorktreeBarApp {
    /// Data changed on disk: every cached entry is read again before it counts as fresh.
    pub(super) fn invalidate(&mut self) {
        self.data_gen += 1;
        self.caches.fresh.clear();
        self.failed_load = None;
    }

    /// The tab or worktree is shown again: read it again in the background.
    pub(super) fn refresh_branches(&mut self, root: &Path) {
        self.caches.fresh.remove(&branches_key(root));
    }

    pub(super) fn refresh_view(&mut self, path: &Path) {
        self.caches.fresh.remove(&view_key(path));
    }

    /// Runs one load at a time on its own lane; the answer carries the generation it began in.
    fn spawn_loader(
        &mut self,
        ctx: &Context,
        key: String,
        work: impl FnOnce(Config, Vec<PathBuf>) -> JobResult + Send + 'static,
    ) {
        if self.loader.is_some() || self.failed_load.as_ref() == Some(&key) {
            return;
        }
        let (tx, rx) = channel();
        let (config, extra, generation) =
            (self.config.clone(), self.extra_roots.clone(), self.data_gen);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send((generation, work(config, extra)));
            ctx.request_repaint();
        });
        self.loader = Some(rx);
        self.loading = Some(key);
    }

    pub(super) fn poll_loader(&mut self, ctx: &Context) {
        let Some(rx) = &self.loader else { return };
        let (generation, result) = match rx.try_recv() {
            Ok(answer) => answer,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.loader = None;
                // A load that crashed is not restarted on the next frame.
                self.failed_load = self.loading.take();
                let lost = AppError::new(ErrorCode::Io, "A background load stopped unexpectedly");
                self.set_error(lost);
                return;
            }
        };
        self.loader = None;
        let key = self.loading.take();
        if generation != self.data_gen {
            return; // Read before the last write or scan: it is read again.
        }
        if let Some(key) = key {
            self.caches.fresh.insert(key);
        }
        self.apply_result(ctx, result);
    }

    /// Shows kept data for the open tab at once, then starts the next load it still needs.
    pub(super) fn load_if_needed(&mut self, ctx: &Context, repos: &[Repo]) {
        let Some(repo) = self.active_repo(repos).cloned() else {
            return;
        };
        // Nothing is read in a repository whose config can run programs until it is trusted.
        if is_blocked(&self.config, &repo) {
            return;
        }
        self.show_cached(&repo.root);
        if self.loader.is_some() {
            return;
        }
        let key = branches_key(&repo.root);
        if !self.caches.fresh.contains(&key) {
            return self.spawn_loader(ctx, key, move |config, _| {
                let loaded = branches(&config, &repo);
                JobResult::Branches(repo.root, loaded)
            });
        }
        let Some(path) = self.selected.clone() else {
            return;
        };
        let key = view_key(&path);
        if !self.caches.fresh.contains(&key) {
            return self.spawn_loader(ctx, key, move |config, extra| {
                let loaded =
                    inspect_worktree(&config, &extra, &path).map(|v| Arc::new(ViewData::build(v)));
                JobResult::View(path, loaded)
            });
        }
        let Some(hash) = self.open_commit.clone() else {
            return;
        };
        if !self.caches.patches.contains_key(&hash) {
            self.spawn_loader(ctx, format!("patch {hash}"), move |config, extra| {
                let loaded = commit_patch(&config, &extra, &path, &hash)
                    .map(|text| Arc::new(parse_diff(&text)));
                JobResult::Patch(path, hash, loaded)
            });
        }
    }

    /// Fills what the window shows from the caches when it shows something else.
    fn show_cached(&mut self, root: &Path) {
        let shown = self.branches.as_ref().is_some_and(|b| b.repo_root == root);
        if let (false, Some(list)) = (shown, self.caches.branches.get(root)) {
            self.branches = Some(BranchState {
                repo_root: root.to_path_buf(),
                list: list.clone(),
            });
        }
        let Some(path) = self.selected.clone() else {
            return;
        };
        let shown = self.view.as_ref().is_some_and(|v| v.path == path);
        if let (false, Some(result)) = (shown, self.caches.views.get(&path).cloned()) {
            self.show_view(path.clone(), result);
        }
        let Some(hash) = self.open_commit.clone() else {
            return;
        };
        let shown = self
            .patch
            .as_ref()
            .is_some_and(|p| p.path == path && p.hash == hash);
        if let (false, Some(diff)) = (shown, self.caches.patches.get(&hash).cloned()) {
            self.patch = Some(PatchState { path, hash, diff });
        }
    }

    pub(super) fn keep_branches(&mut self, root: &Path, list: &BranchList) {
        insert_bounded(&mut self.caches.branches, root.to_path_buf(), list.clone());
    }

    pub(super) fn keep_view(&mut self, path: &Path, result: &AppResult<Arc<ViewData>>) {
        insert_bounded(&mut self.caches.views, path.to_path_buf(), result.clone());
    }

    pub(super) fn keep_patch(&mut self, hash: &str, diff: &AppResult<Arc<DiffView>>) {
        insert_bounded(&mut self.caches.patches, hash.to_string(), diff.clone());
    }

    /// Reads the status of every worktree of the current snapshot on a few threads.
    pub(super) fn start_status_lane(&mut self, ctx: &Context) {
        let snapshot = Arc::clone(&self.repos);
        let config = self.config.clone();
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(read_statuses(&config, &snapshot));
            ctx.request_repaint();
        });
        self.status_lane = Some(StatusLane {
            rx,
            newer: Vec::new(),
        });
    }

    /// A repository was reloaded with its own status while the lane still runs.
    pub(super) fn status_lane_skip(&mut self, root: &Path) {
        if let Some(lane) = self.status_lane.as_mut() {
            lane.newer.push(root.to_path_buf());
        }
    }

    pub(super) fn poll_status_lane(&mut self) {
        let Some(lane) = &self.status_lane else {
            return;
        };
        let statuses = match lane.rx.try_recv() {
            Ok(statuses) => statuses,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => HashMap::new(),
        };
        let newer = lane.newer.clone();
        self.status_lane = None;
        let statuses: HashMap<PathBuf, Option<bool>> = statuses
            .into_iter()
            .filter(|(path, _)| !newer.iter().any(|root| path.starts_with(root)))
            .collect();
        self.repos = Arc::from(apply_statuses(&self.repos, &statuses));
        if let Some(done) = self.status.strip_suffix(STATUS_PENDING) {
            self.status = done.to_string();
        }
    }
}

/// Appended to the status line while the status lane runs.
pub(super) const STATUS_PENDING: &str = " · reading changes…";
