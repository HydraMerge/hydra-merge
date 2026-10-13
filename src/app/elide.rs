//! Text that does not fit its row is shortened with `…`, so a cut name is never mistaken for a
//! whole one. Branch-like names (with `/`) lose their middle: `fix/dns-resolvers-options` and
//! `fix/dns-resolvers-static-options` differ at the end, which a plain right cut would hide.

use eframe::egui::{Color32, FontId, Ui};

const ELLIPSIS: char = '…';

/// Where the characters are taken out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Elide {
    End,
    Middle,
}

impl Elide {
    /// Middle for names with `/` (branches, worktrees, paths), End for everything else.
    pub(super) fn for_name(name: &str) -> Elide {
        if name.contains('/') {
            Elide::Middle
        } else {
            Elide::End
        }
    }
}

/// `text` shortened until `width` of it is at most `max`; unchanged when it already fits.
/// `width` measures a candidate string, so the function needs no fonts and is tested alone.
pub(super) fn elide(text: &str, max: f32, mode: Elide, width: impl Fn(&str) -> f32) -> String {
    if width(text) <= max {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    // The longest kept count that fits: kept characters only grow the width, so bisect.
    let (mut fits, mut too_long) = (0usize, chars.len());
    while too_long - fits > 1 {
        let mid = (fits + too_long) / 2;
        if width(&cut(&chars, mid, mode)) <= max {
            fits = mid;
        } else {
            too_long = mid;
        }
    }
    cut(&chars, fits, mode)
}

/// `keep` characters of `chars` around one `…`.
fn cut(chars: &[char], keep: usize, mode: Elide) -> String {
    let (head, tail) = match mode {
        Elide::End => (keep, 0),
        // The tail tells similar branch names apart, so it gets the extra character.
        Elide::Middle => (keep / 2, keep - keep / 2),
    };
    let mut out: String = chars[..head].iter().collect();
    out.push(ELLIPSIS);
    out.extend(&chars[chars.len() - tail..]);
    out
}

/// `text` fitted to `max` points in `font`, measured with the painter's fonts.
pub(super) fn fit(ui: &Ui, text: &str, font: &FontId, max: f32, mode: Elide) -> String {
    let painter = ui.painter();
    elide(text, max, mode, |candidate| {
        painter
            .layout_no_wrap(candidate.to_string(), font.clone(), Color32::PLACEHOLDER)
            .size()
            .x
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character is one point wide.
    fn chars(s: &str) -> f32 {
        s.chars().count() as f32
    }

    #[test]
    fn text_that_fits_is_kept_as_it_is() {
        assert_eq!(elide("master", 6.0, Elide::End, chars), "master");
        assert_eq!(elide("", 0.0, Elide::Middle, chars), "");
    }

    #[test]
    fn end_mode_cuts_the_right_side() {
        assert_eq!(
            elide("ssl-pinning-trustlist", 8.0, Elide::End, chars),
            "ssl-pin…"
        );
    }

    #[test]
    fn middle_mode_keeps_the_distinguishing_tail() {
        let a = elide("fix/dns-resolvers-options", 12.0, Elide::Middle, chars);
        let b = elide(
            "fix/dns-resolvers-static-options",
            12.0,
            Elide::Middle,
            chars,
        );
        assert_eq!(a, "fix/d…ptions");
        assert_eq!(chars(&a), 12.0);
        assert!(b.starts_with("fix/d") && b.ends_with("ptions") && b.contains('…'));
    }

    #[test]
    fn a_shortened_text_never_exceeds_the_limit() {
        for max in 1..30 {
            let out = elide(
                "feature/SRE-3594-click-support-s3",
                max as f32,
                Elide::Middle,
                chars,
            );
            assert!(chars(&out) <= max as f32, "{out} > {max}");
            assert!(out.contains('…'));
        }
    }

    #[test]
    fn no_room_leaves_only_the_ellipsis() {
        assert_eq!(elide("master", 1.0, Elide::End, chars), "…");
        assert_eq!(elide("master", 0.0, Elide::End, chars), "…");
    }

    #[test]
    fn cyrillic_is_cut_on_characters_not_bytes() {
        assert_eq!(elide("правила контура", 5.0, Elide::End, chars), "прав…");
    }

    #[test]
    fn mode_follows_the_slash() {
        assert_eq!(Elide::for_name("fix/x"), Elide::Middle);
        assert_eq!(Elide::for_name("backend"), Elide::End);
    }
}
