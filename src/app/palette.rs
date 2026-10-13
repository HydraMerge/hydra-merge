//! Command palette, as the field in the middle of the HydraMerge toolbar: type to filter
//! commands by fuzzy match, ↑ ↓ to choose, Enter to run, Esc to close.

use super::history::can_unstage;
use super::locations::location_row;
use super::toolbar::remove_confirm;
use super::*;

const PALETTE_WIDTH: f32 = 560.0;
const LIST_HEIGHT: f32 = 340.0;
const RECENT_LIMIT: usize = 10;
/// Rows drawn for a query; the rest are reached by typing more.
const SHOWN_LIMIT: usize = 200;

/// One entry of the palette.
pub(super) struct Command {
    pub label: String,
    pub enabled: bool,
    pub action: Action,
}

#[derive(Default)]
pub(super) struct PaletteState {
    pub query: String,
    pub selected: usize,
}

fn command(label: impl Into<String>, enabled: bool, action: Action) -> Command {
    Command {
        label: label.into(),
        enabled,
        action,
    }
}

/// Case-insensitive subsequence match. Consecutive letters and letters at word starts score
/// higher, so "pu" ranks "Push" and "Pull" above "Open Path…". `None` when it does not match.
pub(super) fn fuzzy_score(query: &str, text: &str) -> Option<i64> {
    let query: Vec<char> = query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if query.is_empty() {
        return Some(0);
    }
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let mut score = 0i64;
    let mut at = 0usize;
    let mut last: Option<usize> = None;
    for wanted in &query {
        let found = (at..text.len()).find(|i| text[*i] == *wanted)?;
        let word_start = found == 0 || !text[found - 1].is_alphanumeric();
        score += 10;
        score += if last == Some(found.wrapping_sub(1)) {
            15
        } else {
            0
        };
        score += if word_start { 20 } else { 0 };
        score -= (found - at) as i64;
        last = Some(found);
        at = found + 1;
    }
    Some(score - text.len() as i64 / 4)
}

/// Indices of `labels` that match `query`, best first; ties keep the list order.
pub(super) fn ranked(labels: &[&str], query: &str) -> Vec<usize> {
    let mut hits: Vec<(i64, usize)> = labels
        .iter()
        .enumerate()
        .filter_map(|(i, label)| fuzzy_score(query, label).map(|score| (score, i)))
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    hits.into_iter().map(|(_, i)| i).collect()
}

impl WorktreeBarApp {
    /// Commands for the current window: worktree actions first, then places to go to.
    pub(super) fn commands(&self, repos: &[Repo], dark: bool) -> Vec<Command> {
        let mut list = self.general_commands(dark);
        let skins = std::iter::once(crate::skin::AUTO.to_string())
            .chain(self.skins.list.iter().map(|s| s.name.clone()));
        list.extend(
            skins.map(|name| command(format!("Skin: {name}"), true, Action::SetSkin(name))),
        );
        list.extend(self.editors.known.iter().map(|editor| {
            let label = format!("Open With: {}", editor.name());
            command(label, true, Action::SetEditor(editor.clone()))
        }));
        let Some(repo) = self.active_repo(repos) else {
            return list;
        };
        if let Some(wt) = self.selected_worktree(repo) {
            list.splice(0..0, self.worktree_commands(repo, &wt));
        }
        if !repo.code_sources.is_empty() && self.config.trusted.is_known(&repo.root) {
            let revoke = Action::UntrustRepo(repo.root.clone());
            list.push(command("Stop Trusting This Repository", true, revoke));
        }
        list.extend(self.location_commands(repo));
        for other in repos {
            let go = Action::Activate(other.root.clone());
            list.push(command(format!("Repository: {}", other.name), true, go));
        }
        list
    }

    fn worktree_commands(&self, repo: &Repo, wt: &Worktree) -> Vec<Command> {
        // Launching an editor, Terminal or Finder runs no git; the rest waits for trust.
        let idle = !self.busy();
        let free = idle && !crate::trust::is_blocked(&self.config, repo);
        let path = wt.path.clone();
        let staged = self
            .view
            .as_ref()
            .filter(|view| view.path == wt.path)
            .and_then(|view| view.result.as_ref().ok())
            .is_some_and(|data| data.view.files.iter().any(|f| can_unstage(&f.code)));
        let can_commit = free && staged && !self.commit_message.trim().is_empty();
        let removable =
            free && repo.layout == Layout::Bare && wt.dirty == Some(false) && !wt.locked;
        vec![
            command("Fetch", free, Action::Fetch(path.clone())),
            command("Pull", free, Action::Pull(path.clone())),
            command("Push", free, Action::Push(path.clone())),
            command(
                format!("Open in {}", self.editors.current.name()),
                idle,
                Action::Open(path.clone()),
            ),
            command("Open in Terminal", idle, Action::OpenTerminal(path.clone())),
            command(
                crate::os::reveal_in_file_manager(),
                idle,
                Action::RevealInFinder(path.clone()),
            ),
            command("Stage All", free, Action::StageAll(path.clone())),
            command("Commit", can_commit, Action::Commit(path.clone())),
            command("Show Working Copy", true, Action::PickWorkingCopy),
            command(
                "Copy Worktree Path",
                true,
                Action::Copy(path.display().to_string()),
            ),
            command("Remove Worktree…", removable, remove_confirm(wt)),
        ]
    }

    fn general_commands(&self, dark: bool) -> Vec<Command> {
        let free = !self.busy();
        let theme = if dark {
            "Switch to Light Theme"
        } else {
            "Switch to Dark Theme"
        };
        let auto = if self.autoscan {
            "Scan at Startup: Turn Off"
        } else {
            "Scan at Startup: Turn On"
        };
        vec![
            command("Navigate Back", self.nav.can_back(), Action::NavBack),
            command(
                "Navigate Forward",
                self.nav.can_forward(),
                Action::NavForward,
            ),
            command("Rescan Repositories", free, Action::Rescan),
            command(theme, true, Action::ToggleTheme),
            command("Toggle Locations", true, Action::ToggleLocations),
            command("Open Path…", true, Action::OpenRepoDialog),
            command(auto, free, Action::ToggleAutoscan),
            command("Create Skin from Current…", true, Action::CreateSkin),
            command("Open With: Other App…", free, Action::ChooseEditorApp),
            command("Open Skins Folder", true, Action::OpenSkinsFolder),
            command(
                format!("Show Log Path ({})", crate::os::shortcut_log()),
                true,
                Action::ShowLogPath,
            ),
            command(
                if self.auto_fetch {
                    "Auto-fetch: Turn Off"
                } else {
                    "Auto-fetch: Turn On (every 5 min)"
                },
                true,
                Action::ToggleAutoFetch,
            ),
        ]
    }

    /// Worktrees, free branches, remote branches and tags of the open repository.
    fn location_commands(&self, repo: &Repo) -> Vec<Command> {
        let free = !self.busy();
        let bare = repo.layout == Layout::Bare;
        let mut list: Vec<Command> = repo
            .worktrees
            .iter()
            .filter(|wt| openable(wt))
            .map(|wt| {
                command(
                    format!("Worktree: {}", worktree_name(wt)),
                    true,
                    Action::Select(wt.path.clone()),
                )
            })
            .collect();
        let Some(state) = self.branches.as_ref().filter(|b| b.repo_root == repo.root) else {
            return list;
        };
        for branch in &state.list.available {
            let create = Action::Create(repo.root.clone(), branch.clone());
            list.push(command(
                format!("Create Worktree: {branch}"),
                free && bare,
                create,
            ));
        }
        for remote in &state.list.remotes {
            for branch in &remote.branches {
                list.extend(remote_commands(
                    repo,
                    &remote.name,
                    branch,
                    free && bare,
                    free,
                ));
            }
        }
        list.extend(self.compare_commands(repo, &state.list));
        for tag in &state.list.tags {
            let pick = Action::PickTag {
                name: tag.name.clone(),
                commit: tag.commit.clone(),
            };
            list.push(command(format!("Tag: {}", tag.name), true, pick));
        }
        list
    }

    /// Draws the palette under the toolbar field while it is open.
    pub(super) fn palette_ui(
        &mut self,
        ctx: &Context,
        p: &Palette,
        repos: &[Repo],
    ) -> Option<Action> {
        self.palette.as_ref()?;
        let dark = ThemeChoice::from_egui(ctx.theme()) == ThemeChoice::Dark;
        let commands = self.commands(repos, dark);
        let rows = self.palette_rows(&commands);
        let (picked, close) = self.palette_keys(ctx, &commands, &rows);
        if close {
            self.palette = None;
            return None;
        }
        let anchor = self.panes.palette_anchor.unwrap_or(Rect::NOTHING);
        let width = PALETTE_WIDTH.min(ctx.content_rect().width() - 16.0);
        let left = (anchor.center().x - width / 2.0).max(8.0);
        let area = egui::Area::new(Id::new("command-palette"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(left, anchor.top()))
            .show(ctx, |ui| self.palette_body(ui, p, width, &commands, &rows));
        let clicked = area.inner;
        if area.response.clicked_elsewhere() && clicked.is_none() {
            self.palette = None;
        }
        let index = picked.or(clicked)?;
        self.run_command(commands, index)
    }

    /// Indices of the commands shown: matches of the query, or recent commands when it is empty.
    fn palette_rows(&self, commands: &[Command]) -> Vec<usize> {
        let query = self
            .palette
            .as_ref()
            .map_or("", |state| state.query.as_str());
        if query.trim().is_empty() {
            return self
                .panes
                .palette_recent
                .iter()
                .filter_map(|label| commands.iter().position(|c| &c.label == label && c.enabled))
                .collect();
        }
        // Commands that cannot run now are left out.
        let labels: Vec<&str> = commands
            .iter()
            .map(|c| if c.enabled { c.label.as_str() } else { "" })
            .collect();
        let mut rows: Vec<usize> = ranked(&labels, query)
            .into_iter()
            .filter(|i| commands[*i].enabled)
            .collect();
        rows.truncate(SHOWN_LIMIT);
        rows
    }

    /// ↑ ↓ move the choice, Enter runs it, Esc closes. Returns (command to run, close).
    fn palette_keys(
        &mut self,
        ctx: &Context,
        commands: &[Command],
        rows: &[usize],
    ) -> (Option<usize>, bool) {
        let (up, down, enter, escape) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        let Some(state) = self.palette.as_mut() else {
            return (None, true);
        };
        let last = rows.len().saturating_sub(1);
        state.selected = state.selected.min(last);
        if down {
            state.selected = (state.selected + 1).min(last);
        }
        if up {
            state.selected = state.selected.saturating_sub(1);
        }
        let chosen = rows
            .get(state.selected)
            .copied()
            .filter(|i| enter && commands[*i].enabled);
        (chosen, escape)
    }

    fn palette_body(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        width: f32,
        commands: &[Command],
        rows: &[usize],
    ) -> Option<usize> {
        let frame = Frame::popup(ui.style())
            .fill(p.input_bg)
            .stroke(Stroke::new(1.0, p.border))
            .inner_margin(Margin::same(6));
        frame
            .show(ui, |ui| {
                ui.set_width(width - 12.0);
                let state = self.palette.get_or_insert_with(PaletteState::default);
                let edit = ui.add(
                    TextEdit::singleline(&mut state.query)
                        .id_salt("palette-query")
                        .hint_text("Type a command")
                        .desired_width(f32::INFINITY),
                );
                if edit.changed() {
                    state.selected = 0;
                }
                edit.request_focus();
                let (selected, empty_query) = (state.selected, state.query.trim().is_empty());
                ui.add_space(4.0);
                if rows.is_empty() {
                    let text = if empty_query {
                        "No previous command history"
                    } else {
                        "No matching commands"
                    };
                    ui.label(RichText::new(text).italics().color(p.text_muted));
                    return None;
                }
                palette_list(ui, p, commands, rows, selected)
            })
            .inner
    }

    /// Runs command `index` and remembers it on top of the recent list.
    fn run_command(&mut self, commands: Vec<Command>, index: usize) -> Option<Action> {
        let chosen = commands.into_iter().nth(index)?;
        self.palette = None;
        self.panes
            .palette_recent
            .retain(|label| label != &chosen.label);
        self.panes.palette_recent.insert(0, chosen.label);
        self.panes.palette_recent.truncate(RECENT_LIMIT);
        Some(chosen.action)
    }
}

impl WorktreeBarApp {
    /// "Compare with: <branch>" for every other local and remote branch.
    fn compare_commands(&self, repo: &Repo, list: &BranchList) -> Vec<Command> {
        let free = !self.busy();
        let remotes = list
            .remotes
            .iter()
            .flat_map(|r| r.branches.iter().map(move |b| format!("{}/{b}", r.name)));
        list.local
            .iter()
            .cloned()
            .chain(remotes)
            .filter_map(|other| {
                let action = self.compare_with(repo, &other)?;
                Some(command(format!("Compare with: {other}"), free, action))
            })
            .collect()
    }
}

fn remote_commands(
    repo: &Repo,
    remote: &str,
    branch: &str,
    bare: bool,
    free: bool,
) -> Vec<Command> {
    let name = format!("{remote}/{branch}");
    let target = || (repo.root.clone(), remote.to_string(), branch.to_string());
    let (root, r, b) = target();
    let worktree = Action::CreateRemote {
        root,
        remote: r,
        branch: b,
    };
    let (root, r, b) = target();
    let local = Action::CreateBranch {
        root,
        remote: r,
        branch: b,
    };
    vec![
        command(format!("Create Worktree from {name}"), bare, worktree),
        command(format!("Create Local Branch from {name}"), bare, local),
        command(
            format!("Delete Remote Branch {name}…"),
            free,
            delete_remote_confirm(&repo.root, remote, branch),
        ),
    ]
}

/// The rows under the query; a click returns the command's index.
fn palette_list(
    ui: &mut Ui,
    p: &Palette,
    commands: &[Command],
    rows: &[usize],
    selected: usize,
) -> Option<usize> {
    let mut clicked = None;
    let height = (rows.len() as f32 * ROW_HEIGHT).min(LIST_HEIGHT);
    ScrollArea::vertical()
        .max_height(LIST_HEIGHT)
        .min_scrolled_height(height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (n, index) in rows.iter().enumerate() {
                let response = location_row(ui, p, &commands[*index].label, n == selected, &[]);
                if n == selected {
                    response.scroll_to_me(None);
                }
                if response.clicked() {
                    clicked = Some(*index);
                }
            }
        });
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matches_subsequences_and_prefers_word_starts() {
        assert!(fuzzy_score("psh", "Push").is_some());
        assert!(fuzzy_score("xyz", "Push").is_none());
        assert!(fuzzy_score("", "anything").is_some());
        assert!(
            fuzzy_score("PUSH", "push").is_some(),
            "case does not matter"
        );
        let labels = ["Open Path…", "Push", "Pull", "Copy Worktree Path"];
        let order = ranked(&labels, "pu");
        assert_eq!(order[..2], [1, 2], "Push and Pull before Open Path…");
        assert_eq!(ranked(&labels, "cwp"), vec![3]);
    }

    #[test]
    fn worktree_names_rank_by_closeness() {
        let labels = [
            "Worktree: feature/login",
            "Worktree: master",
            "Create Worktree: mr/162",
        ];
        assert_eq!(ranked(&labels, "wt mas")[0], 1);
        assert_eq!(ranked(&labels, "162"), vec![2]);
    }
}
