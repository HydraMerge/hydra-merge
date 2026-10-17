//! The window: panels, background jobs, errors shown in the window.

mod actions;
mod background;
mod central;
mod clean_card;
mod sparse_dialog;
mod commits;
mod compare;
mod dialogs;
mod elide;
mod history;
mod history_filter;
mod jobs;
mod lanes;
mod layout;
mod locations;
mod nav;
mod palette;
mod repo_column;
mod skins;
mod summary;
mod toolbar;
mod top;
mod tree;
mod updates;
mod wt_status;

use locations::*;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;

use eframe::egui::scroll_area::ScrollBarVisibility;
use eframe::egui::{
    self, pos2, vec2, Align2, Button, CentralPanel, CollapsingHeader, Color32, Context, FontId,
    Frame, Id, Key, Margin, Modifiers, Panel, Pos2, Rect, RichText, ScrollArea, Sense, Spinner,
    Stroke, TextEdit, TextStyle, Ui, Vec2,
};

use crate::datefmt::DateFormat;
use crate::diff::{parse_diff, DiffView, RowKind};
use crate::diff_view;
use crate::discover::{open_path, reload_repo, scan_all_with};
use crate::git::Status;
use crate::git::{
    apply_stash, branches, cherry_pick, choose_app, choose_folder, commit_patch, commit_worktree,
    create_sparse_worktree, create_tracking_branch, create_tracking_worktree, create_worktree,
    delete_local_branch, show_all_files, top_folders,
    delete_remote_branch, merge_ref, rebase_onto, revert_commit,
    discard_file, discard_paths, drop_stash, fetch_worktree, inspect_worktree, open_child_dir_with,
    open_worktree_with, pull_worktree, push_worktree, remove_worktree_with, stage_file,
    stage_paths, unstage_paths, BranchList, CommitLine, Editor, FileChange, PushDone, WorktreeView,
};
use crate::graph::{layout, GraphRow};
use crate::model::{AppError, AppResult, Config, ErrorCode, Layout, Repo, Worktree};
use crate::theme::{self, Palette, ThemeChoice};
use lanes::STATUS_PENDING;

const PANEL_MARGIN: i8 = 8;
const STRIPE_WIDTH: f32 = 4.0;
const LOCATIONS_DEFAULT_WIDTH: f32 = 220.0;
const LOCATIONS_MIN_WIDTH: f32 = 160.0;
const REPOS_DEFAULT_WIDTH: f32 = 200.0;
const REPOS_MIN_WIDTH: f32 = 140.0;
const ROW_HEIGHT: f32 = 21.0;
const TAB_HEIGHT: f32 = 36.0;
/// Wider graphs are clipped; their subjects still line up.
const MAX_LANES: usize = 10;

/// Open and Remove are never offered for the `.bare` entry.
pub fn openable(wt: &Worktree) -> bool {
    !wt.bare
}

enum JobResult {
    Scanned(AppResult<Vec<Repo>>),
    /// Every result carries the key it was asked for, so a late answer never lands on a newer selection.
    Branches(PathBuf, AppResult<BranchList>),
    Opened(AppResult<PathBuf>),
    Created(AppResult<Worktree>),
    Removed(AppResult<PathBuf>),
    PathAdded(AppResult<Repo>),
    Reloaded(AppResult<Repo>),
    View(PathBuf, AppResult<Arc<ViewData>>),
    Patch(PathBuf, String, AppResult<Arc<DiffView>>),
    Changed(AppResult<ChangeDone>),
    Pushed(AppResult<PushDone>),
    /// A quick outside action (Terminal, Finder, a link): the text for the status line.
    Done(AppResult<String>),
    FolderChosen(AppResult<Option<PathBuf>>),
    ScanFolderChosen(AppResult<Option<PathBuf>>),
    EditorAppChosen(AppResult<Option<PathBuf>>),
    Compared(CompareKey, AppResult<Arc<DiffView>>),
    Searched(PathBuf, String, AppResult<Vec<CommitLine>>),
    /// Top-level folders of a branch, for the sparse worktree window: (repo root, branch, folders).
    SparseFolders(PathBuf, String, AppResult<Vec<String>>),
}

/// State of the panes that is not git data: comparison, history filter, picked files.
#[derive(Default)]
struct PaneState {
    compare: Option<CompareState>,
    /// Filter of the commit list (subject, author, hash, branch).
    history_query: String,
    history_search_open: bool,
    /// Focus the filter field on the next frame (it was just opened).
    history_focus: bool,
    searched: Option<SearchState>,
    /// Files picked in the Summary lists: (0 staged / 1 unstaged / 2 untracked, path).
    file_selection: Vec<(u8, String)>,
    /// Last file clicked without Shift, for Shift-click ranges: (list, index).
    selection_anchor: Option<(u8, usize)>,
    /// A link the server printed after the last push (merge request page), shown as a button.
    link: Option<String>,
    /// Labels of commands run from the palette, newest first.
    palette_recent: Vec<String>,
    /// The toolbar field the palette opens under (last frame's position).
    palette_anchor: Option<Rect>,
    /// The window is too narrow for Fetch / Pull / Push labels: icons only.
    toolbar_compact: bool,
    /// Filter of the Repositories column.
    repo_query: String,
    repo_search_open: bool,
    /// Focus the Repositories filter on the next frame (⌘L or the magnifier).
    repo_focus: bool,
}

/// The "Create Sparse Worktree" window: the branch's top-level folders, each with a checkbox.
struct SparseDialog {
    root: PathBuf,
    branch: String,
    folders: Vec<(String, bool)>,
}

/// The "Open Repository" window: a path typed or picked in the folder picker.
#[derive(Default)]
struct OpenDialog {
    path: String,
    /// Why the last Open failed, shown in the window.
    error: Option<String>,
    focus: bool,
}

/// Two refs of one repository compared as `base...target`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CompareKey {
    root: PathBuf,
    base: String,
    target: String,
}

struct CompareState {
    key: CompareKey,
    /// `None` while git runs.
    diff: Option<AppResult<Arc<DiffView>>>,
}

/// Commits found by "Search all history" for one worktree and query.
struct SearchState {
    path: PathBuf,
    query: String,
    result: AppResult<Vec<CommitLine>>,
}

struct ChangeDone {
    status: String,
    path: PathBuf,
    clear_message: bool,
    select_newest: bool,
}

/// What the UI asks for; applied after drawing so no borrow outlives a closure.
enum Action {
    Select(PathBuf),
    Activate(PathBuf),
    PickBranch(String),
    PickCommit(String),
    /// The "N changes" row at the top of the commit list.
    PickWorkingCopy,
    /// Hides a repository tab; the ▾ list reopens it.
    CloseTab(PathBuf),
    ToggleLocations,
    NavBack,
    NavForward,
    OpenPalette,
    /// ⌘F: open the commit list filter.
    FindInHistory,
    /// ⌘L: open the Repositories filter.
    FindRepository,
    /// The "Open Repository" window.
    OpenRepoDialog,
    /// The macOS folder picker for that window.
    ChooseFolder,
    /// Pick the folder whose repositories are scanned.
    ChooseScanFolder,
    /// Forget the chosen scan folder and use the default roots again.
    ResetScanFolder,
    /// How much is written to `hydra-merge.log`.
    SetLogLevel(crate::log::Level),
    /// How commit dates are written; kept in the session.
    SetDateFormat(DateFormat),
    /// Puts the log file path on the status line. Bound to ⌘⇧L / Ctrl+Shift+L.
    ShowLogPath,
    /// Asks GitHub for a newer release now.
    CheckUpdate,
    /// Downloads the offered release and replaces this copy.
    InstallUpdate,
    /// Hides the offered release until a newer one exists.
    DismissUpdate,
    CloseRepoDialog,
    Open(PathBuf),
    /// Trust the repository at this root as it is now: its config may run programs.
    TrustRepo(PathBuf),
    /// Forget that trust; the question is asked again.
    UntrustRepo(PathBuf),
    /// Shows a confirmation; its button applies `Confirm::then`.
    Ask(Confirm),
    Remove(PathBuf),
    Create(PathBuf, String),
    /// Reads the branch's top-level folders, then opens the sparse worktree window.
    OpenSparse(PathBuf, String),
    /// `create_sparse_worktree` for (repo root, branch, folders).
    CreateSparse(PathBuf, String, Vec<String>),
    CloseSparse,
    /// `git sparse-checkout disable` in that worktree.
    ShowAllFiles(PathBuf),
    AddPath,
    Rescan,
    ToggleAutoscan,
    ToggleTheme,
    DismissError,
    Stage {
        path: PathBuf,
        rel: String,
    },
    StageAll(PathBuf),
    Unstage {
        path: PathBuf,
        rel: String,
        /// Old path of a staged rename: both sides are unstaged together.
        orig: Option<String>,
    },
    Discard {
        path: PathBuf,
        rel: String,
    },
    Commit(PathBuf),
    Push(PathBuf),
    Pull(PathBuf),
    Fetch(PathBuf),
    PickRemote {
        remote: String,
        branch: String,
    },
    CreateRemote {
        root: PathBuf,
        remote: String,
        branch: String,
    },
    PickTag {
        name: String,
        commit: String,
    },
    PickStash {
        id: String,
        worktree: Option<PathBuf>,
    },
    ApplyStash {
        worktree: PathBuf,
        id: String,
    },
    DropStash {
        worktree: PathBuf,
        id: String,
    },
    OpenSubmodule {
        worktree: PathBuf,
        rel: String,
    },
    DeleteRemote {
        root: PathBuf,
        remote: String,
        branch: String,
    },
    /// `git cherry-pick` of `hash` into the open worktree.
    CherryPick { path: PathBuf, hash: String },
    /// `git revert --no-edit` of `hash` in the open worktree.
    RevertCommit { path: PathBuf, hash: String },
    /// `git merge --no-edit` of `branch` into the open worktree.
    MergeRef { path: PathBuf, branch: String },
    /// `git rebase` of the open worktree onto `branch`.
    RebaseOnto { path: PathBuf, branch: String },
    /// `git branch -d` when no worktree has the branch checked out.
    DeleteBranch { root: PathBuf, branch: String },
    /// `git branch --track <branch> <remote>/<branch>`, no worktree.
    CreateBranch {
        root: PathBuf,
        remote: String,
        branch: String,
    },
    /// Puts text on the clipboard.
    Copy(String),
    OpenTerminal(PathBuf),
    RevealInFinder(PathBuf),
    /// Opens a link the server printed (merge request page).
    OpenLink(String),
    DismissLink,
    Compare(CompareKey),
    CloseCompare,
    /// `git log --grep` over all branches for the query in the commit list filter.
    SearchHistory(String),
    ToggleAutoFetch,
    /// What Open, "Open in …" and new skin files open with.
    SetEditor(Editor),
    /// The macOS application picker for Open With → Other App….
    ChooseEditorApp,
    SetSkin(String),
    /// Writes the current colours to a new `.skin` file and opens it.
    CreateSkin,
    OpenSkinsFolder,
    StagePaths {
        path: PathBuf,
        rels: Vec<String>,
    },
    UnstagePaths {
        path: PathBuf,
        rels: Vec<String>,
    },
    DiscardPaths {
        path: PathBuf,
        rels: Vec<String>,
    },
}

/// Asks before `git push --delete`; used by the detail button and the context menu.
fn delete_remote_confirm(root: &Path, remote: &str, branch: &str) -> Action {
    Action::Ask(Confirm {
        title: "Delete remote branch?",
        subject: format!("{remote}/{branch}"),
        note: Some("Runs git push --delete. The branch disappears on the server for everyone."),
        button: "Delete",
        then: Box::new(Action::DeleteRemote {
            root: root.to_path_buf(),
            remote: remote.to_string(),
            branch: branch.to_string(),
        }),
    })
}

fn merge_confirm(path: &Path, branch: &str) -> Action {
    Action::Ask(Confirm {
        title: "Merge into this worktree?",
        subject: branch.to_string(),
        note: Some("Merges that branch into the one checked out here."),
        button: "Merge",
        then: Box::new(Action::MergeRef {
            path: path.to_path_buf(),
            branch: branch.to_string(),
        }),
    })
}

fn rebase_confirm(path: &Path, branch: &str) -> Action {
    Action::Ask(Confirm {
        title: "Rebase this worktree?",
        subject: branch.to_string(),
        note: Some("Replays the checked-out commits on top of that branch."),
        button: "Rebase",
        then: Box::new(Action::RebaseOnto {
            path: path.to_path_buf(),
            branch: branch.to_string(),
        }),
    })
}

fn delete_branch_confirm(root: &Path, branch: &str) -> Action {
    Action::Ask(Confirm {
        title: "Delete this branch?",
        subject: branch.to_string(),
        note: Some("Deletes a branch that is not checked out. Git refuses one that is not merged."),
        button: "Delete",
        then: Box::new(Action::DeleteBranch {
            root: root.to_path_buf(),
            branch: branch.to_string(),
        }),
    })
}

/// A destructive action waiting for the user's yes.
struct Confirm {
    title: &'static str,
    /// Path, file or ref the action touches, shown in full.
    subject: String,
    note: Option<&'static str>,
    button: &'static str,
    then: Box<Action>,
}

struct BranchState {
    repo_root: PathBuf,
    list: BranchList,
}

struct ViewState {
    path: PathBuf,
    result: AppResult<Arc<ViewData>>,
}

/// One worktree's status and history, prepared on the job thread so drawing only reads it.
struct ViewData {
    view: WorktreeView,
    /// One row per entry of `view.commits`.
    graph: Vec<GraphRow>,
    /// Lanes of the widest row, capped, so every subject starts at the same x.
    graph_width: usize,
    /// `git diff` and `git diff --cached`, parsed and highlighted.
    unstaged: DiffView,
    staged: DiffView,
}

impl ViewData {
    fn build(view: WorktreeView) -> ViewData {
        let hashes: Vec<&str> = view.commits.iter().map(|c| c.full.as_str()).collect();
        let parents: Vec<Vec<&str>> = view
            .commits
            .iter()
            .map(|c| c.parents.iter().map(String::as_str).collect())
            .collect();
        let graph = layout(&hashes, &parents);
        let graph_width = graph
            .iter()
            .map(|row| row.width)
            .max()
            .unwrap_or(1)
            .clamp(1, MAX_LANES);
        let unstaged = parse_diff(&view.unstaged_diff);
        let staged = parse_diff(&view.staged_diff);
        ViewData {
            view,
            graph,
            graph_width,
            unstaged,
            staged,
        }
    }
}

#[derive(Clone)]
struct PatchState {
    path: PathBuf,
    hash: String,
    /// Parsed and highlighted on the job thread; shared so a frame never copies the patch.
    diff: AppResult<Arc<DiffView>>,
}

pub struct WorktreeBarApp {
    config: Config,
    repos: Arc<[Repo]>,
    extra_roots: Vec<PathBuf>,
    /// Repository shown in the tab strip selection and in Locations.
    active_root: Option<PathBuf>,
    selected: Option<PathBuf>,
    branches: Option<BranchState>,
    view: Option<ViewState>,
    open_commit: Option<String>,
    patch: Option<PatchState>,
    chosen_branch: String,
    chosen_remote: Option<(String, String)>,
    chosen_tag: Option<(String, String)>,
    chosen_stash: Option<String>,
    stash_worktree: Option<PathBuf>,
    commit_message: String,
    query: String,
    job: Option<Receiver<JobResult>>,
    error: Option<AppError>,
    status: String,
    /// "Open Repository" window while it is shown.
    open_dialog: Option<OpenDialog>,
    sparse_dialog: Option<SparseDialog>,
    confirm: Option<Confirm>,
    /// Width of the history column set by dragging; `None` follows the window width.
    history_width: Option<f32>,
    /// Scroll the active tab into view on the next frame (it changed without a click).
    reveal_tab: bool,
    /// Repositories hidden from the top tab strip. They stay in the left list.
    closed_tabs: Vec<PathBuf>,
    locations_open: bool,
    /// The filter box under "Locations" is shown.
    search_open: bool,
    nav: nav::NavHistory,
    palette: Option<palette::PaletteState>,
    /// Last title sent to the window, so it is only sent when it changes.
    window_title: String,
    /// Tab loads (branches, history, patches) run here, apart from the action job.
    loader: Option<Receiver<(u64, JobResult)>>,
    /// Bumped by every scan, reload and write; older load answers are dropped.
    data_gen: u64,
    caches: lanes::Caches,
    status_lane: Option<lanes::StatusLane>,
    /// Key of the automatic load in flight (branches, view or patch).
    loading: Option<String>,
    /// A load whose job crashed; it is not started again until something else is loaded.
    failed_load: Option<String>,
    /// The last action asked for while a job was running; it runs when the job ends.
    queued: Option<Action>,
    /// Comparison, history filter and picked files of the open worktree.
    panes: PaneState,
    auto_fetch: bool,
    /// How commit dates are written (⚙ → Preferences → Date Format).
    date_format: DateFormat,
    background: background::Background,
    skins: skins::Skins,
    editors: Editors,
    /// Tab and worktree of the last session, applied after the first scan.
    pending_restore: Option<(Option<PathBuf>, Option<PathBuf>)>,
    /// Keeps the action status when a follow-up scan finishes.
    hold_status: bool,
    /// Walk discovery roots when the window opens. Off unless `autoscan.txt` says `on`.
    autoscan: bool,
    /// Scan folders chosen in Settings. Empty means `default_roots`.
    scan_roots: Vec<PathBuf>,
    /// Discovery roots from the environment, before a chosen scan folder replaces them.
    default_roots: Vec<PathBuf>,
    theme_dir: Option<PathBuf>,
    theme_warned: bool,
    /// A newer release the user has not dismissed.
    update_offer: Option<crate::update::Offer>,
    update_busy: bool,
    update_rx: Receiver<updates::UpdateMsg>,
    update_tx: std::sync::mpsc::Sender<updates::UpdateMsg>,
}

impl WorktreeBarApp {
    /// `settings_dir` holds `theme.txt` and `autoscan.txt`; `None` keeps nothing on disk.
    /// Tests pass a temporary directory instead of reading `$HOME`.
    pub fn with_settings_dir(
        cc: &eframe::CreationContext<'_>,
        config: Config,
        theme_dir: Option<PathBuf>,
    ) -> WorktreeBarApp {
        let stored = theme_dir.as_deref().and_then(theme::load_theme_choice);
        let autoscan = theme_dir.as_deref().is_some_and(theme::load_autoscan);
        theme::install(&cc.egui_ctx, stored);
        if let Some(dir) = theme_dir.as_deref() {
            let path = crate::log::open(dir);
            crate::log_info!("log file {}", path.display());
        }
        let session = theme_dir.as_deref().map(crate::session::load_session);
        let mut app = WorktreeBarApp::blank(config, theme_dir, autoscan);
        if let Some(session) = session {
            app.restore(session);
        }
        app.init_skins(&cc.egui_ctx);
        if app.autoscan {
            app.start_scan(&cc.egui_ctx);
        }
        app
    }

    /// The real window: also looks for a newer release once a day.
    pub fn new(cc: &eframe::CreationContext<'_>, config: Config) -> WorktreeBarApp {
        let mut app = WorktreeBarApp::with_settings_dir(
            cc,
            config,
            crate::os::home_dir().map(|home| theme::config_dir(&home)),
        );
        app.schedule_update_check(&cc.egui_ctx, false);
        app
    }

    /// Empty window state: no repositories yet, nothing selected.
    fn blank(config: Config, theme_dir: Option<PathBuf>, autoscan: bool) -> WorktreeBarApp {
        let default_roots = config.roots.clone();
        let (update_tx, update_rx) = channel();
        WorktreeBarApp {
            config,
            repos: Arc::from(Vec::new()),
            extra_roots: Vec::new(),
            scan_roots: Vec::new(),
            default_roots,
            active_root: None,
            selected: None,
            branches: None,
            view: None,
            open_commit: None,
            patch: None,
            chosen_branch: String::new(),
            chosen_remote: None,
            chosen_tag: None,
            chosen_stash: None,
            stash_worktree: None,
            commit_message: String::new(),
            query: String::new(),
            job: None,
            error: None,
            status: if autoscan { "" } else { "Auto-scan is off" }.to_string(),
            open_dialog: None,
            sparse_dialog: None,
            confirm: None,
            history_width: None,
            reveal_tab: true,
            closed_tabs: Vec::new(),
            locations_open: true,
            search_open: false,
            nav: nav::NavHistory::default(),
            palette: None,
            window_title: String::new(),
            loader: None,
            data_gen: 0,
            caches: lanes::Caches::default(),
            status_lane: None,
            loading: None,
            failed_load: None,
            queued: None,
            panes: PaneState::default(),
            auto_fetch: false,
            date_format: DateFormat::default(),
            background: background::Background::default(),
            skins: skins::Skins::default(),
            editors: Editors::installed(),
            pending_restore: None,
            hold_status: false,
            autoscan,
            theme_dir,
            theme_warned: false,
            update_offer: None,
            update_busy: false,
            update_rx,
            update_tx,
        }
    }

    // ---- jobs ----------------------------------------------------------
}

/// The editor that Open uses, and the ones Open With offers.
struct Editors {
    current: Editor,
    /// Sublime Text, installed ones, and any app picked before.
    known: Vec<Editor>,
}

impl Editors {
    fn installed() -> Editors {
        let home = crate::os::home_dir();
        Editors {
            current: Editor::default(),
            known: crate::git::installed_editors(home.as_deref()),
        }
    }

    /// Makes `editor` the one Open uses, adding it to the list when it is new.
    fn choose(&mut self, editor: Editor) {
        if !self.known.contains(&editor) {
            self.known.push(editor.clone());
        }
        self.current = editor;
    }
}

/// `~/repos/x/master` for a path under the home directory.
pub(super) fn tilde_path(path: &Path) -> String {
    let home = crate::os::home_dir();
    match home
        .as_deref()
        .and_then(|home| path.strip_prefix(home).ok())
    {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

fn find_selectable(repos: &[Repo], path: &Path) -> bool {
    repos
        .iter()
        .flat_map(|r| &r.worktrees)
        .any(|w| w.path == path && openable(w))
}

fn repo_of<'a>(repos: &'a [Repo], path: &Path) -> Option<&'a Repo> {
    repos
        .iter()
        .find(|repo| repo.worktrees.iter().any(|wt| wt.path == path))
}

fn first_openable(repos: &[Repo], root: &Path) -> Option<PathBuf> {
    repos
        .iter()
        .find(|repo| repo.root == root)?
        .worktrees
        .iter()
        .find(|wt| openable(wt))
        .map(|wt| wt.path.clone())
}

fn name_matches(name: &str, query: &str) -> bool {
    query.is_empty()
        || name
            .to_ascii_lowercase()
            .contains(&query.to_ascii_lowercase())
}

fn worktree_name(wt: &Worktree) -> String {
    wt.branch
        .clone()
        .unwrap_or_else(|| "(detached)".to_string())
}

/// Worktrees of one repository, in list order. `step` moves from the current row.
fn neighbour(
    repo: Option<&Repo>,
    query: &str,
    current: Option<&Path>,
    step: i32,
) -> Option<PathBuf> {
    let rows: Vec<&PathBuf> = repo?
        .worktrees
        .iter()
        .filter(|wt| openable(wt) && name_matches(&worktree_name(wt), query))
        .map(|wt| &wt.path)
        .collect();
    let at = current.and_then(|c| rows.iter().position(|p| p.as_path() == c));
    let next = match at {
        Some(i) => (i as i32 + step).clamp(0, rows.len() as i32 - 1) as usize,
        None if step > 0 => 0,
        None => rows.len().checked_sub(1)?,
    };
    rows.get(next).map(|p| (*p).clone())
}

/// Tab text is the repository folder. A repeated folder name keeps the group prefix.
fn tab_label(repos: &[Repo], repo: &Repo) -> String {
    let leaf = repo.name.rsplit('/').next().unwrap_or(repo.name.as_str());
    let shared = repos
        .iter()
        .filter(|other| other.name.rsplit('/').next() == Some(leaf))
        .count()
        > 1;
    if shared {
        repo.name.clone()
    } else {
        leaf.to_string()
    }
}

impl eframe::App for WorktreeBarApp {
    /// The session is written once more when the window closes.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_on_exit();
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        self.poll_update(&ctx);
        self.poll_loader(&ctx);
        self.poll_status_lane();
        self.background_tick(&ctx);
        self.run_queued(&ctx);
        let repos = Arc::clone(&self.repos);
        self.load_if_needed(&ctx, &repos);
        self.update_title(&ctx, &repos);
        let palette = self.current_palette(&ctx);
        let p = &palette;
        let mut actions: Vec<Action> = self.handle_keys(&ctx, &repos).into_iter().collect();
        actions.extend(self.tab_bar(ui, p, &repos));
        actions.extend(self.toolbar(ui, p, &repos));
        actions.extend(self.bottom_bar(ui, p));
        actions.extend(self.repo_sidebar(ui, p, &repos));
        if self.locations_open {
            actions.extend(self.locations(ui, p, &repos));
        }
        actions.extend(self.central(ui, p, &repos));
        actions.extend(self.confirm_dialog(&ctx, p));
        actions.extend(self.open_repo_dialog(&ctx, p));
        actions.extend(self.sparse_dialog(&ctx, p));
        actions.extend(self.palette_ui(&ctx, p, &repos));
        // Back / forward move through the history themselves; anything else that changes the
        // shown place is recorded so ◀ can return to it.
        let navigating = actions
            .iter()
            .any(|a| matches!(a, Action::NavBack | Action::NavForward));
        let before = self.place();
        for action in actions {
            self.apply(&ctx, action);
        }
        if !navigating {
            let now = self.place();
            self.nav.record(before, &now);
        }
    }
}

// ---- drawing -------------------------------------------------------------

/// Draws every item; the first requested action wins.
fn first(actions: impl Iterator<Item = Option<Action>>) -> Option<Action> {
    actions.fold(None, Option::or)
}

fn panel_frame(fill: egui::Color32) -> Frame {
    Frame::new()
        .fill(fill)
        .inner_margin(Margin::same(PANEL_MARGIN))
}
