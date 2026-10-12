//! The filter of the commit list: a search icon in the "Commits" header (or ⌘F) opens a field;
//! loaded commits are filtered as you type, and "Search all history" asks git for the rest.

use super::layout::paint_magnifier;
use super::*;

const FILTER_HEIGHT: f32 = 30.0;

/// Every word of `query` (any case) appears in the subject, author, e-mail, hash or a ref name.
pub(super) fn commit_matches(commit: &CommitLine, query: &str) -> bool {
    let refs: Vec<&str> = commit.refs.iter().map(|r| r.name.as_str()).collect();
    let haystack = [
        commit.subject.as_str(),
        commit.author.as_str(),
        commit.email.as_str(),
        commit.full.as_str(),
        &refs.join(" "),
    ]
    .join("\n")
    .to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|word| haystack.contains(word))
}

impl WorktreeBarApp {
    /// The search icon at the right end of the "Commits" header.
    pub(super) fn history_search_toggle(&mut self, ui: &mut Ui, p: &Palette, head: Rect) {
        let icon =
            Rect::from_center_size(pos2(head.right() - 14.0, head.center().y), vec2(18.0, 18.0));
        let response = ui
            .interact(icon, Id::new("history-search-toggle"), Sense::click())
            .on_hover_text(format!("Filter commits ({})", crate::os::shortcut_find()));
        let open = self.panes.history_search_open || !self.panes.history_query.is_empty();
        let color = if response.hovered() || open {
            p.text_strong
        } else {
            p.text_muted
        };
        paint_magnifier(ui.painter(), icon.center(), color);
        if response.clicked() {
            if open {
                self.close_history_filter();
            } else {
                self.open_history_filter();
            }
        }
    }

    pub(super) fn open_history_filter(&mut self) {
        self.panes.history_search_open = true;
        self.panes.history_focus = true;
    }

    fn close_history_filter(&mut self) {
        self.panes.history_search_open = false;
        self.panes.history_query.clear();
        self.panes.searched = None;
    }

    /// The filter field above the list while it is open; returns the rest of `body`.
    pub(super) fn history_filter_row(&mut self, ui: &mut Ui, p: &Palette, body: Rect) -> Rect {
        if !self.panes.history_search_open && self.panes.history_query.is_empty() {
            return body;
        }
        let row = Rect::from_min_size(body.min, vec2(body.width(), FILTER_HEIGHT));
        ui.painter().rect_filled(row, 0.0, p.pane_header_bg);
        let inner = row.shrink2(vec2(8.0, 4.0));
        let mut close = false;
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            ui.visuals_mut().extreme_bg_color = p.input_bg;
            let edit = ui.add(
                TextEdit::singleline(&mut self.panes.history_query)
                    .id_salt("history-filter")
                    .hint_text("Filter: message, author, hash, branch")
                    .desired_width(f32::INFINITY),
            );
            if self.panes.history_focus {
                edit.request_focus();
                self.panes.history_focus = false;
            }
            // egui drops the field's focus on Escape before this runs, so either state counts.
            let typing = edit.has_focus() || edit.lost_focus();
            close = typing && ui.input(|i| i.key_pressed(Key::Escape));
        });
        if close {
            self.close_history_filter();
        }
        Rect::from_min_max(pos2(body.left(), row.bottom()), body.max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{RefKind, RefLabel};

    fn commit() -> CommitLine {
        CommitLine {
            full: "9dc2f1d64babebaafcad6f36535a8d43df432037".to_string(),
            hash: "9dc2f1d".to_string(),
            author: "Ruslan Khakimov".to_string(),
            email: "r@example.com".to_string(),
            date: "Thu, 8 Oct, 23:39".to_string(),
            parents: Vec::new(),
            refs: vec![RefLabel {
                name: "origin/fix/redis-oom".to_string(),
                kind: RefKind::Remote,
            }],
            subject: "INT3-355 harden redis-cluster against OOM".to_string(),
            body: String::new(),
            files: Vec::new(),
        }
    }

    #[test]
    fn filter_matches_every_word_anywhere() {
        let c = commit();
        assert!(commit_matches(&c, "int3-355"));
        assert!(commit_matches(&c, "khakimov oom"));
        assert!(commit_matches(&c, "9dc2f1d6"));
        assert!(commit_matches(&c, "redis-oom"), "ref names count");
        assert!(!commit_matches(&c, "oom postgres"));
        assert!(commit_matches(&c, "   "), "blank matches all");
    }
}
