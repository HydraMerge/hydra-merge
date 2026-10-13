//! The toolbar under the tabs: sidebar toggle on the left, the checked-out
//! branch in the middle, Fetch / Pull / Push / Open and the ⋯ menu on the right.

use super::top::{icon_button, icon_button_at, icon_button_enabled, paint_icon, Icon};
use super::*;
use crate::trust::is_blocked;

const TOOLBAR_HEIGHT: f32 = 26.0;
const FIELD_WIDTH: f32 = 340.0;
/// Room kept for the buttons on the right before the centred field.
const RIGHT_TOOLS: f32 = 300.0;

impl WorktreeBarApp {
    pub(super) fn toolbar(&mut self, ui: &mut Ui, p: &Palette, repos: &[Repo]) -> Vec<Action> {
        let mut out = Vec::new();
        let wt = self
            .active_repo(repos)
            .and_then(|repo| self.selected_worktree(repo).map(|wt| (repo.clone(), wt)));
        let frame = Frame::new()
            .fill(p.toolbar_bg)
            .inner_margin(Margin::symmetric(8, 4));
        let shown = Panel::top("toolbar").frame(frame).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(TOOLBAR_HEIGHT);
                if icon_button(ui, p, Icon::Sidebar, "Show or hide Locations").clicked() {
                    out.push(Action::ToggleLocations);
                }
                ui.add_space(6.0);
                out.extend(self.nav_buttons(ui, p));
                out.extend(self.center_tools(ui, p, wt.as_ref()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    out.extend(self.settings_menu(ui, p));
                    ui.add_space(4.0);
                    out.extend(self.sync_tools(ui, wt.as_ref()));
                });
            });
        });
        let bottom = shown.response.rect.bottom() - 0.5;
        ui.painter().hline(
            shown.response.rect.x_range(),
            bottom,
            Stroke::new(1.0, p.border),
        );
        out
    }

    /// The palette field centred on the window, with ⋯ next to it.
    fn center_tools(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        wt: Option<&(Repo, Worktree)>,
    ) -> Vec<Action> {
        let mut out = Vec::new();
        let center = (ui.available_width() - RIGHT_TOOLS).max(0.0);
        let (area, _) = ui.allocate_exact_size(vec2(center, TOOLBAR_HEIGHT), Sense::hover());
        let middle = ui
            .ctx()
            .content_rect()
            .center()
            .x
            .clamp(area.left(), area.right());
        let width = FIELD_WIDTH.min(area.width());
        let x = (middle - width / 2.0).clamp(area.left(), area.right() - width);
        let field = Rect::from_min_size(pos2(x, area.center().y - 11.0), vec2(width, 22.0));
        self.panes.palette_anchor = Some(field);
        if branch_field(ui, p, field, wt.map(|(_, wt)| wt)).clicked() {
            out.push(Action::OpenPalette);
        }
        let more = Rect::from_min_size(pos2(field.right() + 6.0, field.top()), vec2(26.0, 22.0));
        out.extend(self.more_menu(ui, p, more, wt));
        out
    }

    /// ◀ ▶ through the places shown before; faded when there is nowhere to go.
    fn nav_buttons(&self, ui: &mut Ui, p: &Palette) -> Option<Action> {
        let (back, forward) = (self.nav.can_back(), self.nav.can_forward());
        let back_tip = format!("Back ({})", crate::os::shortcut_back());
        let forward_tip = format!("Forward ({})", crate::os::shortcut_forward());
        let back = icon_button_enabled(ui, p, Icon::Back, &back_tip, back).clicked();
        let forward = icon_button_enabled(ui, p, Icon::Forward, &forward_tip, forward).clicked();
        if back {
            Some(Action::NavBack)
        } else {
            forward.then_some(Action::NavForward)
        }
    }

    /// Open, Push, Pull, Fetch (right to left), enabled for a selected worktree. Push, Pull and
    /// Fetch run git, so they wait while the repository is not trusted; Open only starts the
    /// editor.
    fn sync_tools(&self, ui: &mut Ui, wt: Option<&(Repo, Worktree)>) -> Vec<Action> {
        let Some((repo, wt)) = wt else {
            return Vec::new();
        };
        let idle = !self.busy();
        let blocked = is_blocked(&self.config, repo);
        let path = wt.path.clone();
        let open_hint = format!("Open in {} (⚙ → Open With)", self.editors.current.name());
        // (label, hint, action, runs git)
        let tools = [
            (
                "Open",
                open_hint.as_str(),
                Action::Open(path.clone()),
                false,
            ),
            ("↑", "Push", Action::Push(path.clone()), true),
            ("↓", "Pull", Action::Pull(path.clone()), true),
            ("↻", "Fetch", Action::Fetch(path), true),
        ];
        let mut out = Vec::new();
        for (label, hint, action, runs_git) in tools {
            let hint = if runs_git && blocked {
                format!("{hint}: trust the repository first")
            } else {
                hint.to_string()
            };
            let button = Button::new(RichText::new(label).size(13.0)).min_size(vec2(30.0, 22.0));
            let enabled = idle && !(runs_git && blocked);
            if ui
                .add_enabled(enabled, button)
                .on_hover_text(hint)
                .clicked()
            {
                out.push(action);
            }
        }
        out
    }

    /// Rescan, Auto, theme, Open path…, Remove worktree…
    /// ⋯ next to the field: only about the shown worktree.
    fn more_menu(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        at: Rect,
        wt: Option<&(Repo, Worktree)>,
    ) -> Option<Action> {
        let button = icon_button_at(ui, p, at, Icon::More, "Worktree");
        let blocked = wt.is_some_and(|(repo, _)| is_blocked(&self.config, repo));
        let entries = worktree_entries(self.busy(), blocked, wt, self.editors.current.name());
        egui::Popup::menu(&button)
            .show(|ui| menu_entries(ui, entries))
            .and_then(|shown| shown.inner)
    }

    /// ⚙ → Open With: what the Open button and "Open in …" use.
    fn editor_menu(&self, ui: &mut Ui) -> Option<Action> {
        let mut picked = None;
        let label = format!("Open With: {}", self.editors.current.name());
        ui.menu_button(label, |ui| {
            for editor in &self.editors.known {
                let mark = if *editor == self.editors.current {
                    "✓ "
                } else {
                    "    "
                };
                if ui.button(format!("{mark}{}", editor.name())).clicked() {
                    picked = Some(Action::SetEditor(editor.clone()));
                    ui.close();
                }
            }
            ui.separator();
            if ui
                .add_enabled(!self.busy(), Button::new("Other App…"))
                .clicked()
            {
                picked = Some(Action::ChooseEditorApp);
                ui.close();
            }
        });
        picked
    }

    /// The gear at the right end: the app, not one worktree — scanning, options, theme.
    fn settings_menu(&mut self, ui: &mut Ui, p: &Palette) -> Option<Action> {
        let busy = self.busy();
        let check = |on: bool, label: &str| {
            if on {
                format!("✓ {label}")
            } else {
                label.to_string()
            }
        };
        let scan = self
            .config
            .roots
            .iter()
            .map(|path| tilde_path(path))
            .collect::<Vec<_>>()
            .join(", ");
        let entries = vec![
            Some(("Rescan Repositories".to_string(), !busy, Action::Rescan)),
            Some((
                format!("Scan Folder… ({scan})"),
                !busy,
                Action::ChooseScanFolder,
            )),
            Some((
                "Open Repository by Path…".to_string(),
                true,
                Action::OpenRepoDialog,
            )),
        ];
        let button = icon_button(ui, p, Icon::Gear, "Settings");
        egui::Popup::menu(&button)
            .show(|ui| {
                let picked = menu_entries(ui, entries);
                picked
                    .or(self.preferences_menu(ui, check))
                    .or(self.editor_menu(ui))
            })
            .and_then(|shown| shown.inner)
    }

    /// The less common gear commands: startup, fetch, updates, log, theme, skin.
    fn preferences_menu(
        &self,
        ui: &mut Ui,
        check: impl Fn(bool, &str) -> String,
    ) -> Option<Action> {
        let busy = self.busy();
        let theme = match ThemeChoice::from_egui(ui.ctx().theme()) {
            ThemeChoice::Dark => "Switch to Light Theme",
            ThemeChoice::Light => "Switch to Dark Theme",
        };
        let mut picked = None;
        ui.menu_button("Preferences", |ui| {
            if !self.scan_roots.is_empty()
                && ui
                    .add_enabled(!busy, Button::new("Use Default Scan Folder"))
                    .clicked()
            {
                picked = Some(Action::ResetScanFolder);
                ui.close();
            }
            if ui
                .add_enabled(!busy, Button::new(check(self.autoscan, "Scan at Startup")))
                .clicked()
            {
                picked = Some(Action::ToggleAutoscan);
                ui.close();
            }
            if ui
                .button(check(
                    self.auto_fetch,
                    "Fetch Automatically (every 5 min)",
                ))
                .clicked()
            {
                picked = Some(Action::ToggleAutoFetch);
                ui.close();
            }
            if ui.button("Check for Updates").clicked() {
                picked = Some(Action::CheckUpdate);
                ui.close();
            }
            ui.separator();
            if picked.is_none() {
                picked = log_level_menu(ui);
            }
            let show_log = format!("Show Log Path ({})", crate::os::shortcut_log());
            if ui.button(show_log).clicked() {
                picked = Some(Action::ShowLogPath);
                ui.close();
            }
            ui.separator();
            if ui.button(theme).clicked() {
                picked = Some(Action::ToggleTheme);
                ui.close();
            }
            if picked.is_none() {
                picked = self.skin_menu(ui);
            }
        });
        picked
    }
}

/// ⚙ → Log Level: error, warn, info, debug. A mark sits on the active one.
fn log_level_menu(ui: &mut Ui) -> Option<Action> {
    let current = crate::log::level();
    let mut picked = None;
    ui.menu_button("Log Level", |ui| {
        for level in crate::log::Level::ALL {
            let mark = if level == current { "✓ " } else { "" };
            if ui.button(format!("{mark}{}", level.as_str())).clicked() {
                picked = Some(Action::SetLogLevel(level));
                ui.close();
            }
        }
    });
    picked
}

/// The ⋯ menu entries of the shown worktree: Terminal, Finder, Remove.
/// One menu entry: label, enabled, action; `None` draws a separator.
type Entry = Option<(String, bool, Action)>;

/// Copy path; Finder, the chosen editor, Terminal; Remove — greyed out without a worktree.
fn worktree_entries(
    busy: bool,
    blocked: bool,
    wt: Option<&(Repo, Worktree)>,
    editor: &str,
) -> Vec<Entry> {
    let open_in = format!("Open Worktree in {editor}");
    let Some((repo, wt)) = wt else {
        let off = |label: &str| Some((label.to_string(), false, Action::DismissLink));
        return vec![
            off("Copy Worktree Path"),
            None,
            off(crate::os::reveal_in_file_manager()),
            off(&open_in),
            off("Open Worktree in Terminal"),
        ];
    };
    // Removing reads the worktree's status: not before the repository is trusted.
    let can_remove = !busy
        && !blocked
        && openable(wt)
        && repo.layout == Layout::Bare
        && wt.dirty == Some(false)
        && !wt.locked;
    let path = wt.path.clone();
    let entry =
        |label: &str, enabled: bool, action: Action| Some((label.to_string(), enabled, action));
    vec![
        entry(
            "Copy Worktree Path",
            true,
            Action::Copy(path.display().to_string()),
        ),
        None,
        entry(
            crate::os::reveal_in_file_manager(),
            !busy,
            Action::RevealInFinder(path.clone()),
        ),
        entry(&open_in, !busy, Action::Open(path.clone())),
        entry(
            "Open Worktree in Terminal",
            !busy,
            Action::OpenTerminal(path),
        ),
        None,
        entry("Remove Worktree…", can_remove, remove_confirm(wt)),
    ]
}

fn menu_entries(ui: &mut Ui, entries: Vec<Entry>) -> Option<Action> {
    let mut picked = None;
    for entry in entries {
        let Some((label, enabled, action)) = entry else {
            ui.separator();
            continue;
        };
        if ui.add_enabled(enabled, Button::new(label)).clicked() {
            picked = Some(action);
            ui.close();
        }
    }
    picked
}

/// Asks before `git worktree remove`.
pub(super) fn remove_confirm(wt: &Worktree) -> Action {
    Action::Ask(Confirm {
        title: "Remove worktree?",
        subject: wt.path.display().to_string(),
        note: None,
        button: "Remove",
        then: Box::new(Action::Remove(wt.path.clone())),
    })
}

/// The field in the middle: branch of the shown worktree with a clean/dirty dot.
/// The field in the middle: `>_`, the branch of the shown worktree and a clean/modified dot.
/// A click opens the command palette (⌘⇧P).
fn branch_field(ui: &mut Ui, p: &Palette, rect: Rect, wt: Option<&Worktree>) -> egui::Response {
    let response = ui
        .interact(rect, Id::new("branch-field"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::Text);
    let painter = ui.painter();
    let stroke = Stroke::new(1.0, p.border);
    painter.rect(rect, 4.0, p.input_bg, stroke, egui::StrokeKind::Inside);
    let prompt = Rect::from_min_size(rect.min + vec2(2.0, 0.0), vec2(24.0, rect.height()));
    paint_icon(ui, p, prompt, Icon::Prompt);
    let Some(wt) = wt else {
        return response.on_hover_text(format!(
            "Command palette ({})",
            crate::os::shortcut_palette()
        ));
    };
    let (state, color) = match wt.dirty {
        Some(true) => ("modified", p.dirty),
        Some(false) => ("clean", p.clean),
        None => ("unknown", p.text_muted),
    };
    let painter = ui.painter();
    painter.circle_filled(pos2(rect.right() - 12.0, rect.center().y), 3.5, color);
    let font = FontId::proportional(12.0);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        worktree_name(wt),
        font,
        p.text_strong,
    );
    let layout = if wt.layout_correct {
        "branch folder"
    } else {
        "path differs from the layout"
    };
    let hint = format!(
        "{}\n{state}, {layout}\nClick or {}: command palette",
        wt.path.display(),
        crate::os::shortcut_palette()
    );
    response.on_hover_text(hint)
}
