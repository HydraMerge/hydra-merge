//! Palettes (tokens from HydraMerge themes), egui visuals and the persisted theme choice.

use std::fs;
use std::path::{Path, PathBuf};

use eframe::egui::epaint::text::{
    FontData, FontInsert, FontPriority, InsertFontFamily, VariationCoords,
};
use eframe::egui::style::WidgetVisuals;
use eframe::egui::{
    Color32, Context, FontFamily, FontId, Stroke, TextStyle, Theme, ThemePreference, Visuals,
};

use crate::model::{AppError, AppResult};

const THEME_FILENAME: &str = "theme.txt";
const AUTOSCAN_FILENAME: &str = "autoscan.txt";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeChoice {
    Dark,
    Light,
}

impl ThemeChoice {
    pub fn from_egui(theme: Theme) -> ThemeChoice {
        match theme {
            Theme::Dark => ThemeChoice::Dark,
            Theme::Light => ThemeChoice::Light,
        }
    }

    pub fn to_preference(self) -> ThemePreference {
        match self {
            ThemeChoice::Dark => ThemePreference::Dark,
            ThemeChoice::Light => ThemePreference::Light,
        }
    }

    pub fn opposite(self) -> ThemeChoice {
        match self {
            ThemeChoice::Dark => ThemeChoice::Light,
            ThemeChoice::Light => ThemeChoice::Dark,
        }
    }

    fn file_text(self) -> &'static str {
        match self {
            ThemeChoice::Dark => "dark\n",
            ThemeChoice::Light => "light\n",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub bg: Color32,
    pub text: Color32,
    pub text_strong: Color32,
    pub text_muted: Color32,
    pub header_bg: Color32,
    pub header_fg: Color32,
    pub sidebar_bg: Color32,
    pub sidebar_fg: Color32,
    pub sidebar_heading_fg: Color32,
    pub row_hover_bg: Color32,
    pub row_selected_bg: Color32,
    pub row_selected_fg: Color32,
    pub row_disabled_fg: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub dirty: Color32,
    pub clean: Color32,
    pub badge_bg: Color32,
    pub badge_fg: Color32,
    pub button_bg: Color32,
    pub button_fg: Color32,
    pub button_hover_bg: Color32,
    pub button_primary_bg: Color32,
    pub button_primary_fg: Color32,
    pub button_danger_bg: Color32,
    pub button_danger_fg: Color32,
    pub button_disabled_opacity: f32,
    pub input_bg: Color32,
    pub error_bg: Color32,
    pub error_fg: Color32,
    pub error_accent: Color32,
    pub focus_ring: Color32,
    /// Diff view: file header, hunk header, added and removed lines.
    pub diff_file_bg: Color32,
    pub diff_file_fg: Color32,
    pub diff_hunk_bg: Color32,
    pub diff_hunk_fg: Color32,
    pub diff_add_bg: Color32,
    pub diff_add_fg: Color32,
    pub diff_remove_bg: Color32,
    pub diff_remove_fg: Color32,
    /// Syntax colours in the diff (Mariana in dark, One Light in light).
    pub syntax_keyword: Color32,
    pub syntax_type: Color32,
    pub syntax_string: Color32,
    pub syntax_number: Color32,
    pub syntax_comment: Color32,
    /// Toolbar under the tabs; the active tab blends into it.
    pub toolbar_bg: Color32,
    /// Badge of the branch checked out in the shown worktree.
    pub badge_head_bg: Color32,
    pub badge_head_fg: Color32,
    /// Lane colours of the commit graph, used in turn.
    pub graph: [Color32; 6],
    /// Right pane (Summary, commit) background; the commit list uses `bg`.
    pub detail_bg: Color32,
    /// Full-width bars of the Summary sections ("Unstaged Files 2").
    pub section_bg: Color32,
    /// Strip with the pane titles ("Commits", "Summary"); the active title takes the pane colour.
    pub pane_header_bg: Color32,
}

pub const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const LIGHT: Palette = Palette {
    bg: hex(0xebedef),
    text: hex(0x404040),
    text_strong: hex(0x2e3338),
    text_muted: hex(0x8a939c),
    header_bg: hex(0xadb6bd),
    header_fg: hex(0x3c4246),
    sidebar_bg: hex(0xdddfe1),
    sidebar_fg: hex(0x505254),
    sidebar_heading_fg: hex(0x3c4246),
    row_hover_bg: hex(0xe0e4e7),
    row_selected_bg: hex(0xd2e1ed),
    row_selected_fg: hex(0x1e2328),
    row_disabled_fg: hex(0xb0b0b0),
    border: hex(0xc3c9cd),
    accent: hex(0x3b8ed0),
    dirty: hex(0xe68619),
    clean: hex(0x3d9933),
    badge_bg: hex(0xd3d8db),
    badge_fg: hex(0x3a3f44),
    button_bg: hex(0xccd3d6),
    button_fg: hex(0x333333),
    button_hover_bg: hex(0xbfc7cb),
    button_primary_bg: hex(0x3b8ed0),
    button_primary_fg: hex(0xffffff),
    button_danger_bg: hex(0xc75050),
    button_danger_fg: hex(0xffffff),
    button_disabled_opacity: 0.4,
    input_bg: hex(0xfbfbfb),
    error_bg: hex(0xffdd80),
    error_fg: hex(0x000000),
    error_accent: hex(0xe68619),
    focus_ring: hex(0x3b8ed0),
    diff_file_bg: hex(0xeef1f4),
    diff_file_fg: hex(0x1a1a1a),
    diff_hunk_bg: hex(0xe6eef7),
    diff_hunk_fg: hex(0x5a6b7d),
    diff_add_bg: hex(0xe3f4dd),
    diff_add_fg: hex(0x3d9933),
    diff_remove_bg: hex(0xfbe4e4),
    diff_remove_fg: hex(0xcc3333),
    syntax_keyword: hex(0xa626a4),
    syntax_type: hex(0xc18401),
    syntax_string: hex(0x50a14f),
    syntax_number: hex(0x986801),
    syntax_comment: hex(0x9a9ba3),
    toolbar_bg: hex(0xc9cfd3),
    badge_head_bg: hex(0x3b8ed0),
    badge_head_fg: hex(0xffffff),
    graph: [
        hex(0x8e5ccf),
        hex(0x3b8ed0),
        hex(0x3d9933),
        hex(0xe68619),
        hex(0xd0455c),
        hex(0x17a2a2),
    ],
    detail_bg: hex(0xfbfbfb),
    section_bg: hex(0xd0d6d8),
    pane_header_bg: hex(0xc9cfd3),
};

pub const DARK: Palette = Palette {
    bg: hex(0x2c3035),
    text: hex(0xaaaaaa),
    text_strong: hex(0xdddddd),
    text_muted: hex(0x737f8c),
    header_bg: hex(0x16181b),
    header_fg: hex(0xdddddd),
    sidebar_bg: hex(0x25292d),
    sidebar_fg: hex(0xaaaaaa),
    sidebar_heading_fg: hex(0xdddddd),
    row_hover_bg: hex(0x32383e),
    row_selected_bg: hex(0x435970),
    row_selected_fg: hex(0xdddddd),
    row_disabled_fg: hex(0x505962),
    border: hex(0x17191c),
    accent: hex(0x81b2e4),
    dirty: hex(0xeb9e47),
    clean: hex(0x5cd65c),
    badge_bg: hex(0x505962),
    badge_fg: hex(0xdddddd),
    button_bg: hex(0x66696c),
    button_fg: hex(0xdddddd),
    button_hover_bg: hex(0x737f8c),
    button_primary_bg: hex(0x4c89c7),
    button_primary_fg: hex(0xffffff),
    button_danger_bg: hex(0x704343),
    button_danger_fg: hex(0xdddddd),
    button_disabled_opacity: 0.4,
    input_bg: hex(0x17191c),
    error_bg: hex(0x704343),
    error_fg: hex(0xdddddd),
    error_accent: hex(0xeb9e47),
    focus_ring: hex(0xaaaaaa),
    diff_file_bg: hex(0x353b42),
    diff_file_fg: hex(0xdddddd),
    diff_hunk_bg: hex(0x313840),
    diff_hunk_fg: hex(0x8a9aab),
    diff_add_bg: hex(0x2d4034),
    diff_add_fg: hex(0x5cd65c),
    diff_remove_bg: hex(0x4a3236),
    diff_remove_fg: hex(0xec5f66),
    syntax_keyword: hex(0xc695c6),
    syntax_type: hex(0x5fb4b4),
    syntax_string: hex(0x99c794),
    syntax_number: hex(0xf9ae58),
    syntax_comment: hex(0x7d8996),
    toolbar_bg: hex(0x343a40),
    badge_head_bg: hex(0x4c89c7),
    badge_head_fg: hex(0xffffff),
    graph: [
        hex(0xb48ef0),
        hex(0x81b2e4),
        hex(0x5cd65c),
        hex(0xeb9e47),
        hex(0xec5f66),
        hex(0x5fb4b4),
    ],
    detail_bg: hex(0x292d32),
    section_bg: hex(0x353b42),
    pane_header_bg: hex(0x23272b),
};

pub fn palette(choice: ThemeChoice) -> &'static Palette {
    match choice {
        ThemeChoice::Dark => &DARK,
        ThemeChoice::Light => &LIGHT,
    }
}

fn widget(base: WidgetVisuals, fill: Color32, stroke: Color32, fg: Color32) -> WidgetVisuals {
    WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: Stroke {
            color: stroke,
            ..base.bg_stroke
        },
        fg_stroke: Stroke {
            color: fg,
            ..base.fg_stroke
        },
        ..base
    }
}

/// Visuals built from the palette tokens (§8.3.1), starting from egui's own dark or light set.
/// Widget states from the palette; `open` looks like `hovered`.
fn widgets(p: &Palette, base: &Visuals) -> eframe::egui::style::Widgets {
    let w = &base.widgets;
    let hovered = widget(w.hovered, p.button_hover_bg, p.focus_ring, p.text_strong);
    eframe::egui::style::Widgets {
        noninteractive: widget(w.noninteractive, p.bg, p.border, p.text),
        inactive: widget(w.inactive, p.button_bg, p.border, p.button_fg),
        hovered,
        active: widget(w.active, p.row_selected_bg, p.accent, p.row_selected_fg),
        open: hovered,
    }
}

pub fn visuals(choice: ThemeChoice) -> Visuals {
    visuals_for(palette(choice), choice)
}

/// egui visuals from any palette (built-in or a skin), on egui's dark or light base.
pub fn visuals_for(p: &Palette, choice: ThemeChoice) -> Visuals {
    let base = match choice {
        ThemeChoice::Dark => Visuals::dark(),
        ThemeChoice::Light => Visuals::light(),
    };
    let widgets = widgets(p, &base);
    Visuals {
        dark_mode: choice == ThemeChoice::Dark,
        override_text_color: None,
        panel_fill: p.bg,
        window_fill: p.bg,
        window_stroke: Stroke {
            color: p.border,
            ..base.window_stroke
        },
        faint_bg_color: p.row_hover_bg,
        extreme_bg_color: p.input_bg,
        code_bg_color: p.sidebar_bg,
        hyperlink_color: p.accent,
        warn_fg_color: p.dirty,
        error_fg_color: p.error_accent,
        selection: eframe::egui::style::Selection {
            bg_fill: p.row_selected_bg,
            stroke: Stroke {
                color: p.row_selected_fg,
                ..base.selection.stroke
            },
        },
        text_cursor: eframe::egui::style::TextCursorStyle {
            stroke: Stroke {
                color: p.accent,
                ..base.text_cursor.stroke
            },
            ..base.text_cursor
        },
        disabled_alpha: p.button_disabled_opacity,
        widgets,
        ..base
    }
}

const SF_UI: &str = "/System/Library/Fonts/SFNS.ttf";
const SF_MONO: &[&str] = &[
    "/Library/Fonts/SF-Mono-Regular.otf",
    "/System/Library/Fonts/SFNSMono.ttf",
];

/// San Francisco for the interface, SF Mono for hashes. egui's built-in face stays as fallback.
pub fn apply_fonts(ctx: &Context) {
    if let Ok(bytes) = fs::read(SF_UI) {
        let mut data = FontData::from_owned(bytes);
        // Slightly above Regular so 13 pt stays crisp on a dark ground. Optical size is UI, not display.
        data.tweak.coords = VariationCoords::new([(b"wght", 450.0), (b"opsz", 17.0)]);
        data.tweak.hinting = Some(false);
        ctx.add_font(FontInsert::new(
            "SanFrancisco",
            data,
            vec![InsertFontFamily {
                family: FontFamily::Proportional,
                priority: FontPriority::Highest,
            }],
        ));
    }
    if let Some(bytes) = SF_MONO.iter().find_map(|path| fs::read(path).ok()) {
        let mut data = FontData::from_owned(bytes);
        data.tweak.hinting = Some(false);
        ctx.add_font(FontInsert::new(
            "SFMono",
            data,
            vec![InsertFontFamily {
                family: FontFamily::Monospace,
                priority: FontPriority::Highest,
            }],
        ));
    }
    let prop = |size| FontId::new(size, FontFamily::Proportional);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, prop(11.0)),
            (TextStyle::Body, prop(12.5)),
            (TextStyle::Button, prop(12.5)),
            (TextStyle::Heading, prop(16.0)),
            (
                TextStyle::Monospace,
                FontId::new(11.5, FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.button_padding = eframe::egui::vec2(8.0, 3.0);
        style.spacing.item_spacing = eframe::egui::vec2(6.0, 4.0);
    });
}

/// Installs both visuals, fonts, the Dark fallback and the stored (or system) preference.
pub fn install(ctx: &Context, stored: Option<ThemeChoice>) {
    ctx.set_visuals_of(Theme::Dark, visuals(ThemeChoice::Dark));
    ctx.set_visuals_of(Theme::Light, visuals(ThemeChoice::Light));
    apply_fonts(ctx);
    ctx.options_mut(|options| options.fallback_theme = Theme::Dark);
    let preference = stored.map_or(ThemePreference::System, ThemeChoice::to_preference);
    ctx.set_theme(preference);
}

pub fn config_dir(home: &Path) -> PathBuf {
    crate::os::config_dir(home)
}

pub fn theme_file(dir: &Path) -> PathBuf {
    dir.join(THEME_FILENAME)
}

pub fn parse_theme(s: &str) -> Option<ThemeChoice> {
    match s.trim() {
        "dark" => Some(ThemeChoice::Dark),
        "light" => Some(ThemeChoice::Light),
        _ => None,
    }
}

pub fn load_theme_choice(dir: &Path) -> Option<ThemeChoice> {
    fs::read_to_string(theme_file(dir))
        .ok()
        .and_then(|text| parse_theme(&text))
}

pub fn save_theme_choice(dir: &Path, choice: ThemeChoice) -> AppResult<()> {
    fs::create_dir_all(dir)
        .map_err(|e| AppError::io(&format!("cannot create {}", dir.display()), &e))?;
    let file = theme_file(dir);
    fs::write(&file, choice.file_text())
        .map_err(|e| AppError::io(&format!("cannot write {}", file.display()), &e))
}

pub fn autoscan_file(dir: &Path) -> PathBuf {
    dir.join(AUTOSCAN_FILENAME)
}

/// Missing or any value other than `on` means auto-scan stays off.
pub fn load_autoscan(dir: &Path) -> bool {
    fs::read_to_string(autoscan_file(dir))
        .ok()
        .is_some_and(|text| text.trim() == "on")
}

pub fn save_autoscan(dir: &Path, enabled: bool) -> AppResult<()> {
    fs::create_dir_all(dir)
        .map_err(|e| AppError::io(&format!("cannot create {}", dir.display()), &e))?;
    let file = autoscan_file(dir);
    let text = if enabled { "on\n" } else { "off\n" };
    fs::write(&file, text)
        .map_err(|e| AppError::io(&format!("cannot write {}", file.display()), &e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn san_francisco_has_weight_axis() {
        let bytes = fs::read(SF_UI).expect("SFNS.ttf");
        let data = FontData::from_owned(bytes);
        let axes: Vec<String> = data
            .variation_axes()
            .into_iter()
            .map(|axis| format!("{:?}", axis.tag))
            .collect();
        assert!(
            axes.iter().any(|tag| tag.contains("wght")),
            "axes: {axes:?}"
        );
    }

    #[test]
    fn both_themes_construct() {
        for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
            let v = visuals(choice);
            let p = palette(choice);
            assert_eq!(v.dark_mode, choice == ThemeChoice::Dark);
            assert_eq!(v.panel_fill, p.bg);
            assert_eq!(v.selection.bg_fill, p.row_selected_bg);
            assert_eq!(v.extreme_bg_color, p.input_bg);
            assert_eq!(v.widgets.open.bg_fill, p.button_hover_bg);
            assert_eq!(v.disabled_alpha, p.button_disabled_opacity);
        }
    }

    #[rustfmt::skip]
    fn tokens(p: &Palette) -> Vec<(&'static str, Color32)> {
        vec![
            ("bg", p.bg), ("text", p.text), ("text_strong", p.text_strong), ("text_muted", p.text_muted),
            ("header_bg", p.header_bg), ("header_fg", p.header_fg), ("sidebar_bg", p.sidebar_bg),
            ("sidebar_fg", p.sidebar_fg), ("sidebar_heading_fg", p.sidebar_heading_fg),
            ("row_hover_bg", p.row_hover_bg), ("row_selected_bg", p.row_selected_bg),
            ("row_selected_fg", p.row_selected_fg), ("row_disabled_fg", p.row_disabled_fg),
            ("border", p.border), ("accent", p.accent), ("dirty", p.dirty), ("clean", p.clean),
            ("badge_bg", p.badge_bg), ("badge_fg", p.badge_fg), ("button_bg", p.button_bg),
            ("button_fg", p.button_fg), ("button_hover_bg", p.button_hover_bg),
            ("button_primary_bg", p.button_primary_bg), ("button_primary_fg", p.button_primary_fg),
            ("button_danger_bg", p.button_danger_bg), ("button_danger_fg", p.button_danger_fg),
            ("input_bg", p.input_bg), ("error_bg", p.error_bg), ("error_fg", p.error_fg),
            ("error_accent", p.error_accent), ("focus_ring", p.focus_ring),
        ]
    }

    // (token, light, dark) copied from the spec table.
    #[rustfmt::skip]
    const SPEC: [(&str, u32, u32); 31] = [
        ("bg", 0xebedef, 0x2c3035), ("text", 0x404040, 0xaaaaaa), ("text_strong", 0x2e3338, 0xdddddd),
        ("text_muted", 0x8a939c, 0x737f8c), ("header_bg", 0xadb6bd, 0x16181b),
        ("header_fg", 0x3c4246, 0xdddddd), ("sidebar_bg", 0xdddfe1, 0x25292d),
        ("sidebar_fg", 0x505254, 0xaaaaaa), ("sidebar_heading_fg", 0x3c4246, 0xdddddd),
        ("row_hover_bg", 0xe0e4e7, 0x32383e), ("row_selected_bg", 0xd2e1ed, 0x435970),
        ("row_selected_fg", 0x1e2328, 0xdddddd), ("row_disabled_fg", 0xb0b0b0, 0x505962),
        ("border", 0xc3c9cd, 0x17191c), ("accent", 0x3b8ed0, 0x81b2e4), ("dirty", 0xe68619, 0xeb9e47),
        ("clean", 0x3d9933, 0x5cd65c), ("badge_bg", 0xd3d8db, 0x505962), ("badge_fg", 0x3a3f44, 0xdddddd),
        ("button_bg", 0xccd3d6, 0x66696c), ("button_fg", 0x333333, 0xdddddd),
        ("button_hover_bg", 0xbfc7cb, 0x737f8c), ("button_primary_bg", 0x3b8ed0, 0x4c89c7),
        ("button_primary_fg", 0xffffff, 0xffffff), ("button_danger_bg", 0xc75050, 0x704343),
        ("button_danger_fg", 0xffffff, 0xdddddd), ("input_bg", 0xfbfbfb, 0x17191c),
        ("error_bg", 0xffdd80, 0x704343), ("error_fg", 0x000000, 0xdddddd),
        ("error_accent", 0xe68619, 0xeb9e47), ("focus_ring", 0x3b8ed0, 0xaaaaaa),
    ];

    #[test]
    fn palettes_match_spec_hex() {
        let light = tokens(&LIGHT);
        let dark = tokens(&DARK);
        assert_eq!(light.len(), SPEC.len());
        for (i, (name, l, d)) in SPEC.iter().enumerate() {
            assert_eq!(light[i], (*name, hex(*l)), "light {name}");
            assert_eq!(dark[i], (*name, hex(*d)), "dark {name}");
        }
        assert_eq!(LIGHT.button_disabled_opacity, 0.4);
        assert_eq!(DARK.button_disabled_opacity, 0.4);
    }

    /// Sum of channel differences; the active tab needs a visible step from the tab strip.
    fn contrast(a: Color32, b: Color32) -> i32 {
        (0..3).map(|i| (a[i] as i32 - b[i] as i32).abs()).sum()
    }

    #[test]
    fn active_tab_stands_out_from_the_tab_strip() {
        for p in [&LIGHT, &DARK] {
            assert!(
                contrast(p.toolbar_bg, p.header_bg) >= 45,
                "tab strip vs active tab"
            );
            assert!(
                contrast(p.pane_header_bg, p.bg) >= 15,
                "pane strip vs active pane title"
            );
        }
    }

    #[test]
    fn diff_backgrounds_differ_from_panel() {
        for p in [&LIGHT, &DARK] {
            for fill in [
                p.diff_file_bg,
                p.diff_hunk_bg,
                p.diff_add_bg,
                p.diff_remove_bg,
            ] {
                assert_ne!(fill, p.bg);
            }
            assert_ne!(p.diff_add_bg, p.diff_remove_bg);
            assert_ne!(p.syntax_keyword, p.text);
            assert_ne!(p.syntax_string, p.text);
            assert_ne!(p.syntax_comment, p.text);
        }
    }

    #[test]
    fn theme_file_roundtrip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("nested").join("cfg");
        assert_eq!(load_theme_choice(&dir), None);
        save_theme_choice(&dir, ThemeChoice::Light).expect("save");
        let text = fs::read_to_string(theme_file(&dir)).expect("read");
        assert!(text.contains("light"));
        assert_eq!(load_theme_choice(&dir), Some(ThemeChoice::Light));
        save_theme_choice(&dir, ThemeChoice::Dark).expect("save");
        assert_eq!(load_theme_choice(&dir), Some(ThemeChoice::Dark));
        fs::write(theme_file(&dir), "purple\n").expect("write");
        assert_eq!(load_theme_choice(&dir), None);
    }

    #[test]
    fn parse_theme_trims_and_rejects_garbage() {
        assert_eq!(parse_theme(" dark \n"), Some(ThemeChoice::Dark));
        assert_eq!(parse_theme("Light"), None);
        assert_eq!(parse_theme(""), None);
    }

    #[test]
    fn autoscan_defaults_off() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("cfg");
        assert!(!load_autoscan(&dir));
        save_autoscan(&dir, true).expect("save");
        assert!(load_autoscan(&dir));
        save_autoscan(&dir, false).expect("save");
        assert!(!load_autoscan(&dir));
        fs::write(autoscan_file(&dir), "yes\n").expect("write");
        assert!(!load_autoscan(&dir));
    }

    #[test]
    fn config_dir_path() {
        let dir = config_dir(Path::new("/h"));
        assert!(dir.ends_with("hydra-merge"));
        #[cfg(target_os = "macos")]
        assert_eq!(
            dir,
            PathBuf::from("/h/Library/Application Support/hydra-merge")
        );
    }
}
