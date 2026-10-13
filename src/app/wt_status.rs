//! The state of a worktree as one icon in front of its name, so the list answers "can I work
//! here" without opening each one. Only the dot of a dirty worktree existed before: a clean one,
//! one still being read, a locked one and one off the `<root>/<branch>` layout all looked alike.

use eframe::egui::{pos2, vec2, Painter, Pos2, Rect, Stroke};

use crate::model::Worktree;
use crate::theme::Palette;

/// What the icon says, most urgent first: a locked worktree cannot be removed whatever its
/// files, and changes matter more than where the folder sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WtState {
    Locked,
    Dirty,
    /// The folder is not at `<repo_root>/<branch>`.
    OffLayout,
    Clean,
    /// `git status` has not answered yet (the list is shown before it).
    Unknown,
}

pub(super) fn state_of(wt: &Worktree, bare_layout: bool) -> WtState {
    if wt.locked {
        WtState::Locked
    } else if wt.dirty == Some(true) {
        WtState::Dirty
    } else if bare_layout && !wt.layout_correct {
        WtState::OffLayout
    } else if wt.dirty == Some(false) {
        WtState::Clean
    } else {
        WtState::Unknown
    }
}

/// The tooltip line under the path.
pub(super) fn describe(state: WtState) -> &'static str {
    match state {
        WtState::Locked => "Locked: git will not remove or move it",
        WtState::Dirty => "Has uncommitted changes",
        WtState::OffLayout => {
            "Not at <repository>/<branch>: the folder name differs from the branch"
        }
        WtState::Clean => "Clean",
        WtState::Unknown => "Reading status…",
    }
}

/// Room the icon takes before the name.
pub(super) const ICON_WIDTH: f32 = 14.0;

/// Paints the icon centred at `at`.
pub(super) fn paint(painter: &Painter, p: &Palette, at: Pos2, state: WtState) {
    match state {
        WtState::Clean => {
            painter.circle_filled(at, 3.5, p.clean);
        }
        WtState::Dirty => {
            painter.circle_filled(at, 3.5, p.dirty);
        }
        WtState::Unknown => {
            painter.circle_stroke(at, 3.0, Stroke::new(1.0, p.text_muted));
        }
        WtState::OffLayout => {
            let top = at + vec2(0.0, -4.5);
            let (left, right) = (at + vec2(-4.5, 3.5), at + vec2(4.5, 3.5));
            let stroke = Stroke::new(1.2, p.dirty);
            painter.line_segment([top, left], stroke);
            painter.line_segment([left, right], stroke);
            painter.line_segment([right, top], stroke);
        }
        WtState::Locked => {
            let body = Rect::from_center_size(at + vec2(0.0, 1.5), vec2(8.0, 6.0));
            painter.rect_filled(body, 1.0, p.text_muted);
            let stroke = Stroke::new(1.2, p.text_muted);
            let (l, r) = (body.left() + 1.5, body.right() - 1.5);
            painter.line_segment([pos2(l, body.top()), pos2(l, body.top() - 2.5)], stroke);
            painter.line_segment([pos2(r, body.top()), pos2(r, body.top() - 2.5)], stroke);
            painter.line_segment(
                [pos2(l, body.top() - 2.5), pos2(r, body.top() - 2.5)],
                stroke,
            );
        }
    }
}

/// Fill of the icon, for tests and for anything that wants to match it.
#[cfg(test)]
fn colour(p: &Palette, state: WtState) -> eframe::egui::Color32 {
    match state {
        WtState::Clean => p.clean,
        WtState::Dirty | WtState::OffLayout => p.dirty,
        WtState::Unknown | WtState::Locked => p.text_muted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn wt(dirty: Option<bool>, locked: bool, layout_correct: bool) -> Worktree {
        Worktree {
            path: PathBuf::from("/r/master"),
            branch: Some("master".into()),
            head: Some("abcdef0".into()),
            bare: false,
            detached: false,
            locked,
            prunable: false,
            dirty,
            layout_path: None,
            layout_correct,
        }
    }

    #[test]
    fn states_in_order_of_urgency() {
        assert_eq!(state_of(&wt(Some(true), true, true), true), WtState::Locked);
        assert_eq!(
            state_of(&wt(Some(true), false, false), true),
            WtState::Dirty
        );
        assert_eq!(
            state_of(&wt(Some(false), false, false), true),
            WtState::OffLayout
        );
        assert_eq!(
            state_of(&wt(Some(false), false, true), true),
            WtState::Clean
        );
        assert_eq!(state_of(&wt(None, false, true), true), WtState::Unknown);
    }

    /// A plain clone has no `<root>/<branch>` layout, so it is never flagged for it.
    #[test]
    fn plain_clone_is_not_off_layout() {
        assert_eq!(
            state_of(&wt(Some(false), false, false), false),
            WtState::Clean
        );
    }

    #[test]
    fn clean_and_dirty_look_different() {
        let p = &crate::theme::LIGHT;
        assert_ne!(colour(p, WtState::Clean), colour(p, WtState::Dirty));
        assert_ne!(colour(p, WtState::Clean), colour(p, WtState::Unknown));
    }
}
