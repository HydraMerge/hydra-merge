//! Summary of a worktree with nothing to commit. A commit form there can only be refused, so
//! the pane shows what the worktree is and the commands one opens a clean worktree for.

use super::wt_status::{self, WtState};
use super::*;

/// (label, hint, action) of the card's buttons, left to right.
fn card_actions(wt: &Worktree, editor: &str, sparse: bool) -> Vec<(String, String, Action)> {
    let path = wt.path.clone();
    let mut actions = vec![
        (
            format!("Open in {editor}"),
            "⚙ → Open With picks the app".to_string(),
            Action::Open(path.clone()),
        ),
        (
            "Open in Terminal".to_string(),
            String::new(),
            Action::OpenTerminal(path.clone()),
        ),
        (
            crate::os::reveal_in_file_manager().to_string(),
            String::new(),
            Action::RevealInFinder(path.clone()),
        ),
        (
            "Copy Path".to_string(),
            String::new(),
            Action::Copy(path.display().to_string()),
        ),
    ];
    if sparse {
        actions.push((
            "Show All Files…".to_string(),
            "Turns the sparse worktree into a full one".to_string(),
            show_all_confirm(path),
        ));
    }
    actions
}

/// Not destructive, but it can write a large tree to disk, so it is confirmed.
fn show_all_confirm(path: PathBuf) -> Action {
    Action::Ask(Confirm {
        title: "Show all files?",
        subject: tilde_path(&path),
        note: Some("Writes the rest of the branch to disk. Nothing is deleted."),
        button: "Show All Files",
        then: Box::new(Action::ShowAllFiles(path)),
    })
}

/// (label, value) lines under the title.
fn card_facts(wt: &Worktree, sparse: Option<&[String]>) -> Vec<(&'static str, String)> {
    let mut facts = vec![("Path", tilde_path(&wt.path))];
    if let Some(folders) = sparse {
        let names: Vec<String> = folders.iter().map(|f| format!("{f}/")).collect();
        facts.push(("Sparse", names.join(", ")));
    }
    if let Some(head) = &wt.head {
        facts.push(("HEAD", head.clone()));
    }
    if wt.detached {
        facts.push(("Branch", "detached HEAD".to_string()));
    }
    if let (false, Some(expected)) = (wt.layout_correct, &wt.layout_path) {
        facts.push(("Expected at", tilde_path(expected)));
    }
    facts
}

impl WorktreeBarApp {
    pub(super) fn clean_card(
        &self,
        ui: &mut Ui,
        p: &Palette,
        repo: &Repo,
        wt: &Worktree,
        sparse: Option<&[String]>,
    ) -> Vec<Action> {
        let state = wt_status::state_of(wt, repo.layout == Layout::Bare);
        ui.horizontal(|ui| {
            let (rect, _) =
                ui.allocate_exact_size(vec2(wt_status::ICON_WIDTH, 20.0), Sense::hover());
            wt_status::paint(ui.painter(), p, rect.center(), state);
            ui.label(
                RichText::new(worktree_name(wt))
                    .heading()
                    .color(p.text_strong),
            );
        });
        let line = match state {
            WtState::Clean | WtState::Unknown => "Nothing to commit, working tree clean",
            other => wt_status::describe(other),
        };
        ui.label(RichText::new(line).color(p.text_muted));
        ui.add_space(10.0);
        egui::Grid::new("clean-card-facts")
            .num_columns(2)
            .spacing(vec2(12.0, 4.0))
            .show(ui, |ui| {
                for (label, value) in card_facts(wt, sparse) {
                    ui.label(RichText::new(label).color(p.text_muted));
                    ui.label(RichText::new(value).monospace().color(p.text));
                    ui.end_row();
                }
            });
        ui.add_space(14.0);
        let busy = self.busy();
        let mut out = Vec::new();
        ui.horizontal_wrapped(|ui| {
            for (n, (label, hint, action)) in
                card_actions(wt, self.editors.current.name(), sparse.is_some())
                    .into_iter()
                    .enumerate()
            {
                // The editor is what a clean worktree is opened for: the one primary button.
                let (fill, fg) = if n == 0 {
                    (p.button_primary_bg, p.button_primary_fg)
                } else {
                    (p.button_bg, p.button_fg)
                };
                let button = Button::new(RichText::new(label).color(fg))
                    .fill(fill)
                    .min_size(vec2(0.0, 24.0));
                let mut response = ui.add_enabled(!busy, button);
                if !hint.is_empty() {
                    response = response.on_hover_text(hint);
                }
                if response.clicked() {
                    out.push(action);
                }
            }
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wt(layout_correct: bool) -> Worktree {
        Worktree {
            path: PathBuf::from("/r/fix/dns"),
            branch: Some("fix/dns".into()),
            head: Some("abcdef0".into()),
            bare: false,
            detached: false,
            locked: false,
            prunable: false,
            dirty: Some(false),
            layout_path: Some(PathBuf::from("/r/fix/dns-x")),
            layout_correct,
        }
    }

    #[test]
    fn editor_comes_first_and_every_action_targets_the_worktree() {
        let actions = card_actions(&wt(true), "Sublime Text", false);
        assert_eq!(actions[0].0, "Open in Sublime Text");
        assert!(matches!(&actions[0].2, Action::Open(p) if p == Path::new("/r/fix/dns")));
        assert!(matches!(&actions[3].2, Action::Copy(s) if s == "/r/fix/dns"));
    }

    #[test]
    fn sparse_worktree_lists_its_folders_and_offers_show_all() {
        let folders = ["app".to_string(), "lib".to_string()];
        let facts = card_facts(&wt(true), Some(&folders));
        assert!(facts.contains(&("Sparse", "app/, lib/".to_string())));
        let full = card_actions(&wt(true), "Zed", false);
        assert!(full.iter().all(|(label, _, _)| label != "Show All Files…"));
        let sparse = card_actions(&wt(true), "Zed", true);
        let last = sparse.last().expect("action");
        assert_eq!(last.0, "Show All Files…");
        assert!(matches!(&last.2, Action::Ask(c)
            if matches!(c.then.as_ref(), Action::ShowAllFiles(p) if p == Path::new("/r/fix/dns"))));
    }

    #[test]
    fn expected_path_only_when_off_layout() {
        let has = |w: &Worktree| card_facts(w, None).iter().any(|(l, _)| *l == "Expected at");
        assert!(!has(&wt(true)));
        assert!(has(&wt(false)));
    }
}
