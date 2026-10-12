//! What the window does on its own, once a second at most:
//! - notices changes made outside it (terminal, glab, scripts) and reloads that repository;
//! - reads the open worktree again when the window gets focus and every few seconds;
//! - fetches the open repository on a timer when Auto-fetch is on;
//! - keeps the session (tabs, selection, window) on disk.

use std::collections::HashMap;
use std::time::Duration;

use super::*;
use crate::git::fetch_repo;
use crate::session::{save_session, Session};
use crate::watch::repo_fingerprint;

/// Seconds between fingerprint checks of the open repository, and of all of them.
const WATCH_ACTIVE: f64 = 3.0;
const WATCH_ALL: f64 = 30.0;
/// Seconds between re-reads of the open worktree while the window has focus.
const VIEW_REFRESH: f64 = 5.0;
/// Seconds between automatic fetches of one repository.
pub(super) const AUTO_FETCH_EVERY: f64 = 300.0;
const SESSION_SAVE_EVERY: f64 = 2.0;
/// Seconds between looks at the skins folder, so a saved `.skin` shows quickly.
const SKIN_CHECK_EVERY: f64 = 1.5;

#[derive(Default)]
pub(super) struct Background {
    fingerprints: HashMap<PathBuf, u64>,
    last_active_watch: f64,
    last_full_watch: f64,
    last_view_refresh: f64,
    was_focused: bool,
    last_fetch: HashMap<PathBuf, f64>,
    fetch_rx: Option<Receiver<(PathBuf, AppResult<PathBuf>)>>,
    saved: Option<Session>,
    saved_at: f64,
    /// The session as of the last frame, for saving when the window closes.
    latest: Option<Session>,
    /// The window was checked against the screen once.
    placed: bool,
    last_skin_check: f64,
}

impl WorktreeBarApp {
    pub(super) fn background_tick(&mut self, ctx: &Context) {
        let now = ctx.input(|i| i.time);
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        self.focus_changed(focused);
        self.keep_on_screen(ctx);
        self.watch(ctx, now);
        if focused && now - self.background.last_view_refresh >= VIEW_REFRESH {
            self.background.last_view_refresh = now;
            if let Some(path) = self.selected.clone() {
                self.refresh_view(&path);
            }
        }
        self.auto_fetch_tick(ctx, now);
        self.poll_fetch();
        self.save_session_tick(ctx, now);
        if now - self.background.last_skin_check >= SKIN_CHECK_EVERY {
            self.background.last_skin_check = now;
            self.watch_skins(ctx);
        }
        // Keeps the ticks going while nothing else repaints the window.
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    /// Once at start: a window saved on a monitor that is gone is moved back onto the screen.
    fn keep_on_screen(&mut self, ctx: &Context) {
        if self.background.placed {
            return;
        }
        let (outer, monitor) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().monitor_size));
        let (Some(outer), Some(monitor)) = (outer, monitor) else {
            return;
        };
        self.background.placed = true;
        let screen = Rect::from_min_size(Pos2::ZERO, monitor);
        if !screen.intersects(outer.shrink(40.0)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos2(40.0, 40.0)));
        }
    }

    /// Coming back to the window: read the open tab again and look for outside changes now.
    fn focus_changed(&mut self, focused: bool) {
        let regained = focused && !self.background.was_focused;
        self.background.was_focused = focused;
        if !regained {
            return;
        }
        if let Some(root) = self.active_root.clone() {
            self.refresh_branches(&root);
        }
        if let Some(path) = self.selected.clone() {
            self.refresh_view(&path);
        }
        self.background.last_active_watch = f64::NEG_INFINITY;
    }

    /// Compares fingerprints; reloads one changed repository per check.
    fn watch(&mut self, ctx: &Context, now: f64) {
        let full = now - self.background.last_full_watch >= WATCH_ALL;
        let active = now - self.background.last_active_watch >= WATCH_ACTIVE;
        if self.busy() || !(full || active) {
            return;
        }
        self.background.last_active_watch = now;
        if full {
            self.background.last_full_watch = now;
        }
        let repos = Arc::clone(&self.repos);
        let checked = repos
            .iter()
            .filter(|repo| full || self.active_root.as_ref() == Some(&repo.root));
        let changed = checked
            .map(|repo| (repo, repo_fingerprint(repo)))
            .find(|(repo, print)| {
                self.background
                    .fingerprints
                    .get(&repo.root)
                    .is_some_and(|old| old != print)
            });
        for repo in repos.iter() {
            self.background
                .fingerprints
                .entry(repo.root.clone())
                .or_insert_with(|| repo_fingerprint(repo));
        }
        if let Some((repo, print)) = changed {
            self.background
                .fingerprints
                .insert(repo.root.clone(), print);
            self.start_reload(ctx, repo.root.clone());
        }
    }

    pub(super) fn forget_fingerprints(&mut self) {
        self.background.fingerprints.clear();
    }

    /// The repository was read again: its current state is the new reference.
    pub(super) fn watch_baseline(&mut self, repo: &Repo) {
        self.background
            .fingerprints
            .insert(repo.root.clone(), repo_fingerprint(repo));
    }

    fn auto_fetch_tick(&mut self, ctx: &Context, now: f64) {
        if !self.auto_fetch || self.background.fetch_rx.is_some() {
            return;
        }
        let Some(root) = self.active_root.clone() else {
            return;
        };
        let blocked = self
            .repos
            .iter()
            .find(|repo| repo.root == root)
            .is_some_and(|repo| crate::trust::is_blocked(&self.config, repo));
        if blocked {
            return;
        }
        let last = self.background.last_fetch.get(&root).copied();
        if last.is_some_and(|at| now - at < AUTO_FETCH_EVERY) {
            return;
        }
        self.background.last_fetch.insert(root.clone(), now);
        let (tx, rx) = channel();
        let config = self.config.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send((root.clone(), fetch_repo(&config, &root)));
            ctx.request_repaint();
        });
        self.background.fetch_rx = Some(rx);
    }

    /// A failed automatic fetch only shows in the status line; it is tried again next period.
    fn poll_fetch(&mut self) {
        let Some(rx) = &self.background.fetch_rx else {
            return;
        };
        let (root, result) = match rx.try_recv() {
            Ok(answer) => answer,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.background.fetch_rx = None;
                return;
            }
        };
        self.background.fetch_rx = None;
        match result {
            Ok(_) => {
                self.refresh_branches(&root);
                if let Some(path) = self.selected.clone() {
                    self.refresh_view(&path);
                }
            }
            Err(e) => self.status = format!("Auto-fetch failed: {}", e.message),
        }
    }

    /// What `session.txt` keeps.
    pub(super) fn session(&self, ctx: &Context) -> Session {
        let (outer, inner) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect));
        let window = outer
            .zip(inner)
            .map(|(outer, inner)| [outer.min.x, outer.min.y, inner.width(), inner.height()]);
        // Until the first scan applies them, the restored tab and worktree are still the ones
        // to keep; the empty window must not overwrite them.
        let (active_root, selected) = match &self.pending_restore {
            Some((root, selected)) => (root.clone(), selected.clone()),
            None => (self.active_root.clone(), self.selected.clone()),
        };
        Session {
            window,
            active_root,
            selected,
            closed_tabs: self.closed_tabs.clone(),
            extra_roots: self.extra_roots.clone(),
            history_width: self.history_width,
            locations_open: self.locations_open,
            auto_fetch: self.auto_fetch,
            recent_commands: self.panes.palette_recent.clone(),
            editor_app: match &self.editors.current {
                Editor::App { path, .. } => Some(path.clone()),
                Editor::SublimeCli => None,
            },
            trusted: self.config.trusted.entries(),
        }
    }

    /// Saves when something changed, at most every couple of seconds.
    fn save_session_tick(&mut self, ctx: &Context, now: f64) {
        let session = self.session(ctx);
        self.background.latest = Some(session.clone());
        if now - self.background.saved_at < SESSION_SAVE_EVERY {
            return;
        }
        self.background.saved_at = now;
        if self.background.saved.as_ref() != Some(&session) {
            self.save_now(session);
        }
    }

    pub(super) fn save_on_exit(&mut self) {
        if let Some(session) = self.background.latest.take() {
            if self.background.saved.as_ref() != Some(&session) {
                self.save_now(session);
            }
        }
    }

    pub(super) fn save_now(&mut self, session: Session) {
        let Some(dir) = &self.theme_dir else {
            return;
        };
        match save_session(dir, &session) {
            Ok(()) => self.background.saved = Some(session),
            Err(e) => eprintln!("warn: {}", e.message),
        }
    }

    /// Restores what the last session kept; the open tab and worktree wait for the first scan.
    pub(super) fn restore(&mut self, session: Session) {
        self.extra_roots = session.extra_roots;
        self.closed_tabs = session.closed_tabs;
        self.history_width = session.history_width;
        self.locations_open = session.locations_open;
        self.auto_fetch = session.auto_fetch;
        self.panes.palette_recent = session.recent_commands;
        for (root, fingerprint) in &session.trusted {
            self.config.trusted.trust(root, fingerprint);
        }
        self.pending_restore = Some((session.active_root, session.selected));
        if let Some(editor) = session.editor_app.as_deref().and_then(Editor::app) {
            self.editors.choose(editor);
        }
    }

    /// After the first scan: reopen the tab and worktree of the last session when they exist.
    pub(super) fn apply_pending_restore(&mut self) {
        let Some((root, selected)) = self.pending_restore.take() else {
            return;
        };
        let repos = Arc::clone(&self.repos);
        if let Some(root) = root.filter(|r| repos.iter().any(|repo| &repo.root == r)) {
            self.active_root = Some(root);
            self.reveal_tab = true;
        }
        if let Some(path) = selected.filter(|p| find_selectable(&repos, p)) {
            self.selected = Some(path);
        }
    }
}
