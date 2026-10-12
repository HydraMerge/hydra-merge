//! Back / forward history of what the window shows, like the ◀ ▶ buttons of HydraMerge.
//! Pure state; the window records a place after each user action that changes it.

use std::path::PathBuf;

/// Most places kept behind the current one.
const LIMIT: usize = 100;

/// What the window shows: repository tab, worktree, commit or picked location.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Place {
    pub root: Option<PathBuf>,
    pub selected: Option<PathBuf>,
    pub commit: Option<String>,
    pub branch: String,
    pub remote: Option<(String, String)>,
    pub tag: Option<(String, String)>,
    pub stash: Option<String>,
}

#[derive(Debug, Default)]
pub(super) struct NavHistory {
    back: Vec<Place>,
    forward: Vec<Place>,
}

impl NavHistory {
    /// The window moved from `before` to `now` on its own: `before` goes on the back stack and
    /// the forward stack is dropped, as in a browser.
    pub fn record(&mut self, before: Place, now: &Place) {
        if &before == now {
            return;
        }
        self.back.push(before);
        if self.back.len() > LIMIT {
            self.back.remove(0);
        }
        self.forward.clear();
    }

    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// The place to go back to; `current` becomes the first forward step.
    pub fn back(&mut self, current: Place) -> Option<Place> {
        let place = self.back.pop()?;
        self.forward.push(current);
        Some(place)
    }

    pub fn forward(&mut self, current: Place) -> Option<Place> {
        let place = self.forward.pop()?;
        self.back.push(current);
        Some(place)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(commit: &str) -> Place {
        Place {
            commit: Some(commit.to_string()),
            ..Place::default()
        }
    }

    #[test]
    fn back_and_forward_walk_the_history() {
        let mut nav = NavHistory::default();
        nav.record(at("a"), &at("b"));
        nav.record(at("b"), &at("c"));
        assert!(nav.can_back() && !nav.can_forward());
        assert_eq!(nav.back(at("c")), Some(at("b")));
        assert_eq!(nav.back(at("b")), Some(at("a")));
        assert_eq!(nav.back(at("a")), None);
        assert_eq!(nav.forward(at("a")), Some(at("b")));
        assert_eq!(nav.forward(at("b")), Some(at("c")));
        assert_eq!(nav.forward(at("c")), None);
    }

    #[test]
    fn a_new_place_drops_the_forward_steps() {
        let mut nav = NavHistory::default();
        nav.record(at("a"), &at("b"));
        assert_eq!(nav.back(at("b")), Some(at("a")));
        nav.record(at("a"), &at("x"));
        assert!(!nav.can_forward());
        assert_eq!(nav.back(at("x")), Some(at("a")));
    }

    #[test]
    fn unchanged_place_is_not_recorded_and_history_is_capped() {
        let mut nav = NavHistory::default();
        nav.record(at("a"), &at("a"));
        assert!(!nav.can_back());
        for i in 0..(LIMIT + 10) {
            nav.record(at(&i.to_string()), &at("next"));
        }
        assert_eq!(nav.back.len(), LIMIT);
        assert_eq!(nav.back.first(), Some(&at("10")));
    }
}
