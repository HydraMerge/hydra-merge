//! The Summary pane of the working copy for "N unstaged files":
//! a borderless commit message, the author line with the Commit button, and collapsible
//! full-width sections of staged, unstaged and untracked files with their diffs.

use eframe::egui::collapsing_header::CollapsingState;

use super::history::{can_discard, can_unstage, change_row, Side};
use super::*;

const SECTION_HEIGHT: f32 = 24.0;
const COMMIT_BUTTON_WIDTH: f32 = 230.0;
/// The pane's inner margin; section bars reach over it to the pane edges.
const BLEED: f32 = 12.0;

/// The bar of one Summary section.
struct SectionHead<'a> {
    id: Id,
    title: &'a str,
    count: usize,
    /// Buttons on the right of the bar, right to left.
    buttons: Vec<(String, Action)>,
}

impl WorktreeBarApp {
    /// A section with the group's files (click, ⌘-click and Shift-click pick several) and, when
    /// there is one, its diff. `group` is (0 staged / 1 unstaged / 2 untracked, files).
    fn group_section(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        path: &Path,
        group: (u8, Group<'_>),
        busy: bool,
    ) -> Vec<Action> {
        let (kind, group) = group;
        let picked: Vec<&FileChange> = group
            .files
            .iter()
            .copied()
            .filter(|f| self.panes.file_selection.contains(&(kind, f.path.clone())))
            .collect();
        let head = SectionHead {
            id: Id::new(("summary-section", group.title, path)),
            title: group.title,
            count: group.files.len(),
            buttons: selection_buttons(path, kind, &picked, group.all),
        };
        let mut clicks: Vec<(usize, Modifiers)> = Vec::new();
        let actions = section(ui, p, head, busy, |ui| {
            let mut actions = Vec::new();
            for (n, file) in group.files.iter().enumerate() {
                let selected = picked.iter().any(|f| f.path == file.path);
                let row = change_row(ui, p, path, file, group.side, (selected, busy));
                actions.extend(row.actions);
                clicks.extend(row.clicked.map(|mods| (n, mods)));
            }
            if let Some(diff) = group.diff.filter(|d| !d.rows.is_empty()) {
                ui.add_space(6.0);
                diff_view::show(ui, p, diff, group.title);
            }
            actions
        });
        for (n, mods) in clicks {
            self.pick_file(kind, group.files, n, mods);
        }
        actions
    }

    /// Click: only this file. ⌘-click: add or remove it. Shift-click: the range from the last
    /// plain click. Files of one list at a time.
    fn pick_file(&mut self, kind: u8, files: &[&FileChange], index: usize, mods: Modifiers) {
        let key = (kind, files[index].path.clone());
        self.panes.file_selection.retain(|(k, _)| *k == kind);
        // The anchor may come from a longer list read before the last refresh.
        let anchor = self
            .panes
            .selection_anchor
            .filter(|(k, at)| mods.shift && *k == kind && *at < files.len());
        match anchor {
            Some((_, anchor)) => {
                let (from, to) = (anchor.min(index), anchor.max(index));
                let range = files.get(from..=to).unwrap_or_default();
                self.panes.file_selection = range.iter().map(|f| (kind, f.path.clone())).collect();
            }
            None if mods.command => {
                if !self.panes.file_selection.contains(&key) {
                    self.panes.file_selection.push(key);
                } else {
                    self.panes.file_selection.retain(|item| item != &key);
                }
                self.panes.selection_anchor = Some((kind, index));
            }
            None => {
                self.panes.file_selection = vec![key];
                self.panes.selection_anchor = Some((kind, index));
            }
        }
    }

    pub(super) fn summary_pane(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        repo: &Repo,
        wt: &Worktree,
        data: &ViewData,
    ) -> Vec<Action> {
        let busy = self.busy();
        let files = &data.view.files;
        if files.is_empty() {
            return self.clean_card(ui, p, repo, wt, data.view.sparse.as_deref());
        }
        let staged: Vec<&FileChange> = files.iter().filter(|f| can_unstage(&f.code)).collect();
        let untracked: Vec<&FileChange> = files.iter().filter(|f| f.code == "??").collect();
        let unstaged: Vec<&FileChange> = files.iter().filter(|f| can_discard(&f.code)).collect();
        let mut out = Vec::new();
        out.extend(self.commit_box(ui, p, wt, &data.view, staged.len(), busy));
        ui.add_space(10.0);
        let path = wt.path.as_path();
        let lists = [staged.as_slice(), unstaged.as_slice(), untracked.as_slice()];
        for (index, group) in groups(data, lists, path).into_iter().enumerate() {
            if !group.files.is_empty() {
                out.extend(self.group_section(ui, p, path, (index as u8, group), busy));
            }
        }
        out
    }

    /// The message in a framed field (it shares `detail_bg` with the pane in Merge Light, so a
    /// borderless field was invisible), the author line, and the Commit button on its right.
    fn commit_box(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        wt: &Worktree,
        view: &WorktreeView,
        staged: usize,
        busy: bool,
    ) -> Option<Action> {
        let id = Id::new("commit-message");
        let focused = ui.memory(|m| m.has_focus(id));
        let ring = if focused { p.focus_ring } else { p.border };
        Frame::new()
            .fill(p.input_bg)
            .stroke(Stroke::new(1.0, ring))
            .corner_radius(4.0)
            .inner_margin(Margin::same(6))
            .show(ui, |ui| {
                ui.add_enabled(
                    !busy,
                    TextEdit::multiline(&mut self.commit_message)
                        .id(id)
                        .hint_text("Commit message")
                        .font(TextStyle::Monospace)
                        .frame(Frame::NONE)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                );
            });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let author = view
                .author
                .clone()
                .unwrap_or_else(|| format!("Commit to {}", worktree_name(wt)));
            ui.label(RichText::new(author).color(p.text_muted))
                .on_hover_text(wt.path.display().to_string());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let blocker = commit_blocker(busy, staged, &self.commit_message);
                let button = Button::new(RichText::new(commit_label(staged)).color(p.button_fg))
                    .fill(p.button_bg)
                    .min_size(vec2(COMMIT_BUTTON_WIDTH, 22.0));
                ui.add_enabled(blocker.is_none(), button)
                    .on_disabled_hover_text(blocker.unwrap_or_default())
                    .clicked()
                    .then(|| Action::Commit(wt.path.clone()))
            })
            .inner
        })
        .inner
    }
}

fn commit_label(staged: usize) -> String {
    match staged {
        0 => "Nothing to commit".to_string(),
        1 => "Commit 1 file".to_string(),
        n => format!("Commit {n} files"),
    }
}

/// Why Commit is off, shown on hover; `None` when it can run.
fn commit_blocker(busy: bool, staged: usize, message: &str) -> Option<&'static str> {
    if busy {
        Some("Wait for the running command")
    } else if staged == 0 {
        Some("Stage files first: Stage in a file row, or Stage All")
    } else if message.trim().is_empty() {
        Some("Enter a commit message")
    } else {
        None
    }
}

/// One kind of changed files in the Summary pane.
struct Group<'a> {
    title: &'a str,
    files: &'a [&'a FileChange],
    side: Side,
    diff: Option<&'a DiffView>,
    all: Option<Action>,
}

/// Staged, unstaged (with Stage All) and untracked files, in order.
fn groups<'a>(data: &'a ViewData, lists: [&'a [&'a FileChange]; 3], path: &Path) -> [Group<'a>; 3] {
    let [staged, unstaged, untracked] = lists;
    let group = |title, files, side, diff, all| Group {
        title,
        files,
        side,
        diff,
        all,
    };
    [
        group(
            "Staged Files",
            staged,
            Side::Staged,
            Some(&data.staged),
            None,
        ),
        group(
            "Unstaged Files",
            unstaged,
            Side::Unstaged,
            Some(&data.unstaged),
            Some(Action::StageAll(path.to_path_buf())),
        ),
        group("Untracked Files", untracked, Side::Unstaged, None, None),
    ]
}

/// Buttons for the picked files of one list, or `all` (Stage All) when none is picked.
fn selection_buttons(
    path: &Path,
    kind: u8,
    picked: &[&FileChange],
    all: Option<Action>,
) -> Vec<(String, Action)> {
    if picked.is_empty() {
        return all
            .map(|a| ("Stage All".to_string(), a))
            .into_iter()
            .collect();
    }
    let n = picked.len();
    let rels: Vec<String> = picked.iter().map(|f| f.path.clone()).collect();
    let path = path.to_path_buf();
    match kind {
        0 => {
            // A staged rename is unstaged with its old path too.
            let origs = picked.iter().filter_map(|f| f.orig_path.clone());
            let rels = rels.into_iter().chain(origs).collect();
            vec![(
                format!("Unstage Selected ({n})"),
                Action::UnstagePaths { path, rels },
            )]
        }
        1 => vec![
            (
                format!("Stage Selected ({n})"),
                Action::StagePaths {
                    path: path.clone(),
                    rels: rels.clone(),
                },
            ),
            (
                "Discard Selected…".to_string(),
                discard_many_confirm(path, rels),
            ),
        ],
        _ => vec![(
            format!("Stage Selected ({n})"),
            Action::StagePaths { path, rels },
        )],
    }
}

fn discard_many_confirm(path: PathBuf, rels: Vec<String>) -> Action {
    Action::Ask(Confirm {
        title: "Discard changes?",
        subject: rels.join("\n"),
        note: Some("Restores these tracked files from the last commit."),
        button: "Discard",
        then: Box::new(Action::DiscardPaths { path, rels }),
    })
}

/// A full-width gray bar ("▾ Unstaged Files  2   Stage All") that folds its body.
fn section(
    ui: &mut Ui,
    p: &Palette,
    head: SectionHead<'_>,
    busy: bool,
    body: impl FnOnce(&mut Ui) -> Vec<Action>,
) -> Vec<Action> {
    let mut state = CollapsingState::load_with_default_open(ui.ctx(), head.id, true);
    let (rect, response) = section_bar(ui, p, &head, state.is_open());
    let mut out = Vec::new();
    let mut right = rect.right();
    for (label, action) in head.buttons {
        let font = FontId::proportional(11.0);
        let text_width = ui
            .painter()
            .layout_no_wrap(label.clone(), font, p.text)
            .size()
            .x;
        let width = text_width + 2.0 * ui.spacing().button_padding.x + 8.0;
        let at = Rect::from_min_max(
            pos2(right - width, rect.top() + 3.0),
            pos2(right, rect.bottom() - 3.0),
        );
        right -= width + 6.0;
        let button = Button::new(RichText::new(label).color(p.text))
            .small()
            .wrap_mode(egui::TextWrapMode::Extend);
        if ui.put(at, button).clicked() && !busy {
            out.push(action);
        }
    }
    if response.clicked() {
        state.toggle(ui);
    }
    let shown = state.show_body_unindented(ui, |ui| {
        ui.add_space(4.0);
        body(ui)
    });
    out.extend(shown.map(|r| r.inner).unwrap_or_default());
    ui.add_space(6.0);
    out
}

/// Paints the bar: triangle, title and count badge; returns its rect and click response.
fn section_bar(
    ui: &mut Ui,
    p: &Palette,
    head: &SectionHead<'_>,
    open: bool,
) -> (Rect, egui::Response) {
    let size = vec2(ui.available_width(), SECTION_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    ui.painter()
        .rect_filled(rect.expand2(vec2(BLEED, 0.0)), 0.0, p.section_bg);
    triangle(ui, p, pos2(rect.left() + 4.0, rect.center().y), open);
    let font = FontId::proportional(12.0);
    let title_at = pos2(rect.left() + 16.0, rect.center().y);
    let title = ui.painter().text(
        title_at,
        Align2::LEFT_CENTER,
        head.title,
        font,
        p.text_strong,
    );
    count_badge(
        ui,
        p,
        pos2(title.right() + 8.0, rect.center().y),
        head.count,
    );
    (rect, response)
}

fn triangle(ui: &Ui, p: &Palette, at: Pos2, open: bool) {
    let points = if open {
        vec![
            at + vec2(-3.5, -2.0),
            at + vec2(3.5, -2.0),
            at + vec2(0.0, 2.5),
        ]
    } else {
        vec![
            at + vec2(-2.0, -3.5),
            at + vec2(-2.0, 3.5),
            at + vec2(2.5, 0.0),
        ]
    };
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        p.text_muted,
        Stroke::NONE,
    ));
}

fn count_badge(ui: &Ui, p: &Palette, left_center: Pos2, count: usize) {
    let font = FontId::proportional(11.0);
    let galley = ui
        .painter()
        .layout_no_wrap(count.to_string(), font, p.text_strong);
    let size = vec2(galley.size().x + 8.0, 15.0);
    let badge = Rect::from_min_size(left_center - vec2(0.0, size.y / 2.0), size);
    ui.painter().rect_filled(badge, 3.0, p.button_bg);
    ui.painter()
        .galley(badge.center() - galley.size() / 2.0, galley, p.text_strong);
}

#[cfg(test)]
mod commit_tests {
    use super::*;

    #[test]
    fn commit_says_what_is_missing() {
        assert_eq!(commit_blocker(false, 0, "msg"), Some("Stage files first: Stage in a file row, or Stage All"));
        assert_eq!(commit_blocker(false, 2, "  \n"), Some("Enter a commit message"));
        assert_eq!(commit_blocker(true, 2, "msg"), Some("Wait for the running command"));
        assert_eq!(commit_blocker(false, 2, "msg"), None);
        assert_eq!(commit_label(2), "Commit 2 files");
    }
}
