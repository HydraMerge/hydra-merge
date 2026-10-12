//! Changed files of the working copy, and the detail pane of one commit.

use super::*;

/// Which list a changed file is drawn in: its buttons follow the side.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Unstaged,
    Staged,
}

/// One changed file: status letters, name, and the buttons its status allows on this side.
/// What a changed-file row reports: its button actions, and the modifiers of a click on the
/// name (for picking several files).
pub(super) struct RowOut {
    pub actions: Vec<Action>,
    pub clicked: Option<Modifiers>,
}

/// One changed file: status letter, name (click to select), and the buttons of its side.
/// Right click offers "Stage Folder" for the folder that holds it.
pub(super) fn change_row(
    ui: &mut Ui,
    p: &Palette,
    path: &Path,
    file: &FileChange,
    side: Side,
    look: (bool, bool),
) -> RowOut {
    let (selected, busy) = look;
    let mut out = RowOut {
        actions: Vec::new(),
        clicked: None,
    };
    ui.horizontal(|ui| {
        let letter = match side {
            Side::Unstaged => file.code.get(1..2),
            Side::Staged => file.code.get(..1),
        };
        let letter = letter.filter(|l| *l != " ").unwrap_or("?");
        let mark = RichText::new(letter).monospace();
        ui.label(mark.color(change_color(p, &file.code)));
        let name = ui.selectable_label(selected, RichText::new(change_label(file)).color(p.text));
        if name.clicked() {
            out.clicked = Some(ui.input(|i| i.modifiers));
        }
        name.context_menu(|ui| out.actions.extend(file_menu(ui, path, file, side)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for (label, color, action) in file_actions(p, path, file, side) {
                let text = RichText::new(label).color(color);
                if ui.add_enabled(!busy, Button::new(text).small()).clicked() {
                    out.actions.push(action);
                }
            }
        });
    });
    out
}

/// Right click on a changed file: stage its whole folder, or copy its path.
fn file_menu(ui: &mut Ui, path: &Path, file: &FileChange, side: Side) -> Option<Action> {
    let folder = Path::new(&file.path)
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| format!("{}/", dir.display()));
    let mut picked = None;
    if let (Side::Unstaged, Some(folder)) = (side, folder) {
        if ui.button(format!("Stage Folder {folder}")).clicked() {
            picked = Some(Action::StagePaths {
                path: path.to_path_buf(),
                rels: vec![folder],
            });
            ui.close();
        }
    }
    if ui.button("Copy Path").clicked() {
        picked = Some(Action::Copy(file.path.clone()));
        ui.close();
    }
    picked
}

/// Buttons of a changed file, right to left: Unstage on the staged side; Stage and, for a
/// tracked file, Discard on the unstaged side.
fn file_actions(
    p: &Palette,
    path: &Path,
    file: &FileChange,
    side: Side,
) -> Vec<(&'static str, Color32, Action)> {
    let wt = path.to_path_buf();
    let rel = file.path.clone();
    match side {
        Side::Staged => vec![(
            "Unstage",
            p.text,
            Action::Unstage {
                path: wt,
                rel,
                orig: file.orig_path.clone(),
            },
        )],
        Side::Unstaged => {
            let mut actions = vec![(
                "Stage",
                p.text,
                Action::Stage {
                    path: wt.clone(),
                    rel,
                },
            )];
            if can_discard(&file.code) {
                actions.push(("Discard", p.dirty, discard_confirm(wt, &file.path)));
            }
            actions
        }
    }
}

fn discard_confirm(path: PathBuf, rel: &str) -> Action {
    Action::Ask(Confirm {
        title: "Discard changes?",
        subject: rel.to_string(),
        note: Some("Restores this tracked file from the last commit."),
        button: "Discard",
        then: Box::new(Action::Discard {
            path,
            rel: rel.to_string(),
        }),
    })
}

/// `old → new` for a rename or copy, the path otherwise.
fn change_label(file: &FileChange) -> String {
    match &file.orig_path {
        Some(orig) => format!("{orig} → {}", file.path),
        None => file.path.clone(),
    }
}

pub(super) fn can_stage(code: &str) -> bool {
    match code.as_bytes().get(1) {
        Some(b' ') => code.as_bytes().first() == Some(&b'?'),
        Some(_) => true,
        None => false,
    }
}

pub(super) fn can_unstage(code: &str) -> bool {
    matches!(code.as_bytes().first(), Some(c) if *c != b' ' && *c != b'?')
}

pub(super) fn can_discard(code: &str) -> bool {
    matches!(code.as_bytes().get(1), Some(c) if *c != b' ' && *c != b'?')
}

pub(super) fn change_color(p: &Palette, code: &str) -> Color32 {
    if code.contains('?') {
        p.text_muted
    } else if code.contains('A') {
        p.clean
    } else {
        p.dirty
    }
}

fn commit_header(ui: &mut Ui, p: &Palette, commit: &CommitLine) {
    ui.label(
        RichText::new(&commit.subject)
            .heading()
            .color(p.text_strong),
    );
    ui.add_space(2.0);
    let byline = format!("{} <{}>  ·  {}", commit.author, commit.email, commit.date);
    ui.label(RichText::new(byline).color(p.text_muted));
    ui.label(
        RichText::new(&commit.full)
            .monospace()
            .small()
            .color(p.text_muted),
    );
    if !commit.body.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new(&commit.body).color(p.text));
    }
}

fn commit_files(ui: &mut Ui, p: &Palette, commit: &CommitLine) {
    ui.add_space(8.0);
    section_header(ui, p, "FILES", commit.files.len());
    if commit.files.is_empty() {
        ui.label(RichText::new("No files").color(p.text_muted));
    }
    for file in &commit.files {
        ui.horizontal(|ui| {
            let status = RichText::new(&file.status).monospace();
            ui.label(status.color(change_color(p, &file.status)));
            ui.label(RichText::new(&file.path).color(p.text));
        });
    }
}

/// Subject, author, hash, files and the highlighted patch of one commit.
pub(super) fn commit_detail(
    ui: &mut Ui,
    p: &Palette,
    commit: &CommitLine,
    patch: Option<&PatchState>,
    path: &Path,
) {
    let hash = commit.hash.as_str();
    commit_header(ui, p, commit);
    commit_files(ui, p, commit);
    ui.add_space(8.0);
    ui.label(RichText::new("DIFF").small().color(p.sidebar_heading_fg));
    ui.add_space(2.0);
    match patch.filter(|item| item.path == path && item.hash == hash) {
        None => {
            super::layout::loading_dots(ui, p);
        }
        Some(item) => match &item.diff {
            Err(error) => {
                ui.label(RichText::new(&error.message).color(p.dirty));
            }
            Ok(diff) => diff_view::show(ui, p, diff, hash),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_buttons_follow_porcelain() {
        assert!(can_stage("??") && !can_unstage("??") && !can_discard("??"));
        assert!(can_stage(" M") && !can_unstage(" M") && can_discard(" M"));
        assert!(!can_stage("M ") && can_unstage("M ") && !can_discard("M "));
        assert!(can_stage("MM") && can_unstage("MM") && can_discard("MM"));
        assert!(!can_stage("A ") && can_unstage("A ") && !can_discard("A "));
        assert!(can_stage(" D") && !can_unstage(" D") && can_discard(" D"));
    }
}
