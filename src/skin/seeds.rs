//! Built-in skins. Merge Light and Merge Dark are the hand-tuned palettes of `theme.rs`; the others
//! are well-known editor schemes, each given as a few base colours from which every token of the
//! palette is derived, so a new scheme needs about twenty colours instead of fifty.

use eframe::egui::Color32;

use crate::theme::{hex, Palette, DARK, LIGHT};

/// The colours a scheme is made of.
#[derive(Clone, Copy, Debug)]
pub struct Seed {
    pub dark: bool,
    /// Tab bar, the darkest (dark) or most saturated (light) chrome.
    pub chrome: Color32,
    /// Toolbar and the active tab.
    pub toolbar: Color32,
    pub sidebar: Color32,
    /// Commit list.
    pub list: Color32,
    /// Summary, commit view and diff.
    pub pane: Color32,
    pub text: Color32,
    pub strong: Color32,
    pub muted: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub selection: Color32,
    pub green: Color32,
    pub red: Color32,
    pub orange: Color32,
    pub keyword: Color32,
    pub type_name: Color32,
    pub string: Color32,
    pub number: Color32,
    pub comment: Color32,
}

/// `a` moved towards `b` by `t` (0 = a, 1 = b).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let channel = |x: u8, y: u8| {
        (x as f32 + (y as f32 - x as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
    )
}

/// Black or white, whichever reads better on `fill`.
pub fn on(fill: Color32) -> Color32 {
    let luma = 0.299 * fill.r() as f32 + 0.587 * fill.g() as f32 + 0.114 * fill.b() as f32;
    if luma > 150.0 {
        hex(0x111111)
    } else {
        hex(0xffffff)
    }
}

impl Seed {
    /// Every palette token from the seed colours: window surfaces, controls, then content.
    pub fn palette(&self) -> Palette {
        let base = if self.dark { DARK } else { LIGHT };
        self.content(self.controls(self.surfaces(base)))
    }

    /// `base` moved towards `color` by the light or dark amount of this seed.
    fn tint(&self, base: Color32, color: Color32, light: f32, dark: f32) -> Color32 {
        mix(base, color, if self.dark { dark } else { light })
    }

    /// Backgrounds, text, rows and borders of the window.
    fn surfaces(&self, p: Palette) -> Palette {
        let s = self;
        Palette {
            bg: s.list,
            text: s.text,
            text_strong: s.strong,
            text_muted: s.muted,
            header_bg: s.chrome,
            header_fg: s.text,
            sidebar_bg: s.sidebar,
            sidebar_fg: s.text,
            sidebar_heading_fg: s.strong,
            row_hover_bg: s.tint(s.list, s.strong, 0.05, 0.07),
            row_selected_bg: s.selection,
            row_selected_fg: s.strong,
            row_disabled_fg: mix(s.text, s.list, 0.55),
            border: s.border,
            accent: s.accent,
            dirty: s.orange,
            clean: s.green,
            toolbar_bg: s.toolbar,
            detail_bg: s.pane,
            section_bg: s.tint(s.pane, s.text, 0.12, 0.12),
            pane_header_bg: s.tint(s.chrome, s.list, 0.6, 0.4),
            ..p
        }
    }

    /// Buttons, badges, inputs and the error bar.
    fn controls(&self, p: Palette) -> Palette {
        let s = self;
        Palette {
            badge_bg: s.tint(s.list, s.text, 0.14, 0.2),
            badge_fg: s.strong,
            button_bg: s.tint(s.toolbar, s.text, 0.12, 0.18),
            button_fg: s.strong,
            button_hover_bg: s.tint(s.toolbar, s.text, 0.22, 0.3),
            button_primary_bg: s.accent,
            button_primary_fg: on(s.accent),
            button_danger_bg: s.tint(s.red, s.list, 0.0, 0.35),
            button_danger_fg: on(s.tint(s.red, s.list, 0.0, 0.35)),
            button_disabled_opacity: 0.4,
            input_bg: s.tint(s.pane, hex(0x000000), 0.0, 0.3),
            error_bg: s.tint(s.orange, s.list, 0.55, 0.55),
            error_fg: s.strong,
            error_accent: s.orange,
            focus_ring: s.accent,
            badge_head_bg: s.accent,
            badge_head_fg: on(s.accent),
            ..p
        }
    }

    /// Diff rows, syntax colours and graph lanes.
    fn content(&self, p: Palette) -> Palette {
        let s = self;
        Palette {
            diff_file_bg: s.tint(s.pane, s.text, 0.07, 0.1),
            diff_file_fg: s.strong,
            diff_hunk_bg: s.tint(s.pane, s.accent, 0.08, 0.1),
            diff_hunk_fg: s.muted,
            diff_add_bg: s.tint(s.pane, s.green, 0.16, 0.18),
            diff_add_fg: s.green,
            diff_remove_bg: s.tint(s.pane, s.red, 0.14, 0.18),
            diff_remove_fg: s.red,
            syntax_keyword: s.keyword,
            syntax_type: s.type_name,
            syntax_string: s.string,
            syntax_number: s.number,
            syntax_comment: s.comment,
            graph: [s.keyword, s.accent, s.green, s.orange, s.red, s.type_name],
            ..p
        }
    }
}

/// Name, dark flag and palette of every built-in skin, in menu order.
pub fn builtin() -> Vec<(&'static str, bool, Palette)> {
    let mut skins = vec![("Merge Light", false, LIGHT), ("Merge Dark", true, DARK)];
    skins.extend(
        SEEDS
            .iter()
            .map(|(name, seed)| (*name, seed.dark, seed.palette())),
    );
    skins
}

const SEEDS: [(&str, Seed); 7] = [
    ("Mariana", MARIANA),
    ("One Dark", ONE_DARK),
    ("Monokai", MONOKAI),
    ("Nord", NORD),
    ("Solarized Dark", SOLARIZED_DARK),
    ("Solarized Light", SOLARIZED_LIGHT),
    ("GitHub Light", GITHUB_LIGHT),
];

const MARIANA: Seed = Seed {
    dark: true,
    chrome: hex(0x1e2328),
    toolbar: hex(0x3a434d),
    sidebar: hex(0x2b323a),
    list: hex(0x303841),
    pane: hex(0x343d46),
    text: hex(0xd8dee9),
    strong: hex(0xffffff),
    muted: hex(0xa6acb9),
    border: hex(0x1b1f24),
    accent: hex(0x6699cc),
    selection: hex(0x4f5b66),
    green: hex(0x99c794),
    red: hex(0xec5f66),
    orange: hex(0xf9ae58),
    keyword: hex(0xc695c6),
    type_name: hex(0x5fb4b4),
    string: hex(0x99c794),
    number: hex(0xf9ae58),
    comment: hex(0xa6acb9),
};

const ONE_DARK: Seed = Seed {
    dark: true,
    chrome: hex(0x1b1e23),
    toolbar: hex(0x333842),
    sidebar: hex(0x21252b),
    list: hex(0x282c34),
    pane: hex(0x2c313a),
    text: hex(0xabb2bf),
    strong: hex(0xe6e6e6),
    muted: hex(0x7f848e),
    border: hex(0x181a1f),
    accent: hex(0x61afef),
    selection: hex(0x3e4451),
    green: hex(0x98c379),
    red: hex(0xe06c75),
    orange: hex(0xd19a66),
    keyword: hex(0xc678dd),
    type_name: hex(0xe5c07b),
    string: hex(0x98c379),
    number: hex(0xd19a66),
    comment: hex(0x7f848e),
};

const MONOKAI: Seed = Seed {
    dark: true,
    chrome: hex(0x1a1b17),
    toolbar: hex(0x34352d),
    sidebar: hex(0x22231e),
    list: hex(0x272822),
    pane: hex(0x2d2e27),
    text: hex(0xf8f8f2),
    strong: hex(0xffffff),
    muted: hex(0x90908a),
    border: hex(0x141510),
    accent: hex(0x66d9ef),
    selection: hex(0x49483e),
    green: hex(0xa6e22e),
    red: hex(0xf92672),
    orange: hex(0xfd971f),
    keyword: hex(0xf92672),
    type_name: hex(0x66d9ef),
    string: hex(0xe6db74),
    number: hex(0xae81ff),
    comment: hex(0x75715e),
};

const NORD: Seed = Seed {
    dark: true,
    chrome: hex(0x242933),
    toolbar: hex(0x3b4252),
    sidebar: hex(0x2b303b),
    list: hex(0x2e3440),
    pane: hex(0x323946),
    text: hex(0xd8dee9),
    strong: hex(0xeceff4),
    muted: hex(0x7b88a1),
    border: hex(0x20242c),
    accent: hex(0x88c0d0),
    selection: hex(0x434c5e),
    green: hex(0xa3be8c),
    red: hex(0xbf616a),
    orange: hex(0xd08770),
    keyword: hex(0x81a1c1),
    type_name: hex(0x8fbcbb),
    string: hex(0xa3be8c),
    number: hex(0xb48ead),
    comment: hex(0x616e88),
};

const SOLARIZED_DARK: Seed = Seed {
    dark: true,
    chrome: hex(0x001e27),
    toolbar: hex(0x073642),
    sidebar: hex(0x002530),
    list: hex(0x002b36),
    pane: hex(0x00303c),
    text: hex(0x93a1a1),
    strong: hex(0xeee8d5),
    muted: hex(0x657b83),
    border: hex(0x001a21),
    accent: hex(0x268bd2),
    selection: hex(0x0a4a5a),
    green: hex(0x859900),
    red: hex(0xdc322f),
    orange: hex(0xcb4b16),
    keyword: hex(0x859900),
    type_name: hex(0xb58900),
    string: hex(0x2aa198),
    number: hex(0xd33682),
    comment: hex(0x586e75),
};

const SOLARIZED_LIGHT: Seed = Seed {
    dark: false,
    chrome: hex(0xd9d2bf),
    toolbar: hex(0xeee8d5),
    sidebar: hex(0xeee8d5),
    list: hex(0xfdf6e3),
    pane: hex(0xfffbef),
    text: hex(0x586e75),
    strong: hex(0x073642),
    muted: hex(0x93a1a1),
    border: hex(0xd3cbb7),
    accent: hex(0x268bd2),
    selection: hex(0xd6e4ea),
    green: hex(0x859900),
    red: hex(0xdc322f),
    orange: hex(0xcb4b16),
    keyword: hex(0x859900),
    type_name: hex(0xb58900),
    string: hex(0x2aa198),
    number: hex(0xd33682),
    comment: hex(0x93a1a1),
};

const GITHUB_LIGHT: Seed = Seed {
    dark: false,
    chrome: hex(0xdce1e6),
    toolbar: hex(0xf6f8fa),
    sidebar: hex(0xf6f8fa),
    list: hex(0xffffff),
    pane: hex(0xffffff),
    text: hex(0x24292f),
    strong: hex(0x1f2328),
    muted: hex(0x656d76),
    border: hex(0xd0d7de),
    accent: hex(0x0969da),
    selection: hex(0xddf4ff),
    green: hex(0x1a7f37),
    red: hex(0xcf222e),
    orange: hex(0xbc4c00),
    keyword: hex(0xcf222e),
    type_name: hex(0x953800),
    string: hex(0x0a3069),
    number: hex(0x0550ae),
    comment: hex(0x6e7781),
};

#[cfg(test)]
mod tests {
    use super::*;

    fn contrast(a: Color32, b: Color32) -> i32 {
        (0..3).map(|i| (a[i] as i32 - b[i] as i32).abs()).sum()
    }

    #[test]
    fn every_builtin_skin_keeps_text_and_tabs_readable() {
        for (name, _, p) in builtin() {
            assert!(contrast(p.text, p.bg) >= 150, "{name}: text on list");
            assert!(
                contrast(p.text_strong, p.detail_bg) >= 150,
                "{name}: text on pane"
            );
            assert!(
                contrast(p.toolbar_bg, p.header_bg) >= 30,
                "{name}: active tab vs tab strip"
            );
            assert!(
                contrast(p.diff_add_bg, p.diff_remove_bg) >= 20,
                "{name}: added vs removed"
            );
            assert!(
                contrast(p.button_primary_fg, p.button_primary_bg) >= 200,
                "{name}: primary button"
            );
        }
    }

    #[test]
    fn mix_and_contrast_text() {
        assert_eq!(mix(hex(0x000000), hex(0xffffff), 0.5), hex(0x808080));
        assert_eq!(on(hex(0xffffff)), hex(0x111111));
        assert_eq!(on(hex(0x123456)), hex(0xffffff));
    }
}
