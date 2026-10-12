//! Skins: named palettes. Built-in ones come from `seeds.rs`; custom ones are text files in
//! `<config dir>/skins/*.skin`:
//!
//! ```text
//! # comments start with '#'
//! base = Merge Dark          # any skin name, or "dark" / "light"; tokens not set below come from it
//! dark = true                # optional: which egui base and which system mode it belongs to
//! accent = #61afef           # any palette token, as #rrggbb
//! graph = #c678dd, #61afef, #98c379, #d19a66, #e06c75, #e5c07b
//! button_disabled_opacity = 0.4
//! ```

pub mod seeds;

use std::fs;
use std::path::{Path, PathBuf};

use eframe::egui::Color32;

use crate::model::{AppError, AppResult};
use crate::theme::{hex, Palette, DARK, LIGHT};

/// The default: Merge Light or Merge Dark, following the system and the theme switch.
pub const AUTO: &str = "Auto";
const SKINS_DIRNAME: &str = "skins";
const SKIN_EXTENSION: &str = "skin";
const CHOICE_FILENAME: &str = "skin.txt";

/// One selectable skin.
#[derive(Clone, Debug, PartialEq)]
pub struct Skin {
    pub name: String,
    pub dark: bool,
    pub palette: Palette,
    /// The `.skin` file of a custom skin.
    pub file: Option<PathBuf>,
}

macro_rules! colour_tokens {
    ($($field:ident),* $(,)?) => {
        /// Names of the colour tokens, as written in `.skin` files.
        pub const TOKENS: &[&str] = &[$(stringify!($field)),*];

        fn get_colour(p: &Palette, name: &str) -> Option<Color32> {
            match name {
                $(stringify!($field) => Some(p.$field),)*
                _ => None,
            }
        }

        fn set_colour(p: &mut Palette, name: &str, colour: Color32) -> bool {
            match name {
                $(stringify!($field) => { p.$field = colour; true })*
                _ => false,
            }
        }
    };
}

colour_tokens!(
    bg,
    text,
    text_strong,
    text_muted,
    header_bg,
    header_fg,
    sidebar_bg,
    sidebar_fg,
    sidebar_heading_fg,
    row_hover_bg,
    row_selected_bg,
    row_selected_fg,
    row_disabled_fg,
    border,
    accent,
    dirty,
    clean,
    badge_bg,
    badge_fg,
    button_bg,
    button_fg,
    button_hover_bg,
    button_primary_bg,
    button_primary_fg,
    button_danger_bg,
    button_danger_fg,
    input_bg,
    error_bg,
    error_fg,
    error_accent,
    focus_ring,
    diff_file_bg,
    diff_file_fg,
    diff_hunk_bg,
    diff_hunk_fg,
    diff_add_bg,
    diff_add_fg,
    diff_remove_bg,
    diff_remove_fg,
    syntax_keyword,
    syntax_type,
    syntax_string,
    syntax_number,
    syntax_comment,
    toolbar_bg,
    badge_head_bg,
    badge_head_fg,
    detail_bg,
    section_bg,
    pane_header_bg,
);

/// `#rrggbb` (or `rrggbb`).
pub fn parse_colour(text: &str) -> Option<Color32> {
    let digits = text.trim().trim_start_matches('#');
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(digits, 16).ok().map(hex)
}

fn colour_text(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// Built-in skins, with Merge Light and Merge Dark first.
pub fn builtin_skins() -> Vec<Skin> {
    seeds::builtin()
        .into_iter()
        .map(|(name, dark, palette)| Skin {
            name: name.to_string(),
            dark,
            palette,
            file: None,
        })
        .collect()
}

/// The skin a custom file starts from: `dark` / `light` or the name of a known skin.
fn base_skin(name: &str, known: &[Skin]) -> Option<(bool, Palette)> {
    match name.to_ascii_lowercase().as_str() {
        "dark" => Some((true, DARK)),
        "light" => Some((false, LIGHT)),
        lower => known
            .iter()
            .find(|s| s.name.to_ascii_lowercase() == lower)
            .map(|s| (s.dark, s.palette.clone())),
    }
}

/// A custom skin from its text; `known` are the skins `base =` may name. Lines that cannot be
/// used come back as warnings ("line 4: unknown token 'acent'"), the rest still applies.
pub fn parse_skin(name: &str, text: &str, known: &[Skin]) -> (Skin, Vec<String>) {
    let lines: Vec<(usize, &str, &str)> = text
        .lines()
        .enumerate()
        .filter_map(|(n, line)| {
            let line = strip_comment(line);
            let (key, value) = line.split_once('=')?;
            Some((n + 1, key.trim(), value.trim()))
        })
        .collect();
    let mut warnings = Vec::new();
    let base = lines.iter().find(|(_, key, _)| *key == "base");
    let (mut dark, mut palette) = match base {
        Some((n, _, value)) => base_skin(value, known).unwrap_or_else(|| {
            warnings.push(format!(
                "line {n}: unknown base skin '{value}', using Merge Dark"
            ));
            (true, DARK)
        }),
        None => (true, DARK),
    };
    for (n, key, value) in lines.iter().filter(|(_, key, _)| *key != "base") {
        if let Err(problem) = apply_line(&mut palette, &mut dark, key, value) {
            warnings.push(format!("line {n}: {problem}"));
        }
    }
    let skin = Skin {
        name: name.to_string(),
        dark,
        palette,
        file: None,
    };
    (skin, warnings)
}

/// The line without its comment: `#` at the start, or `#` after a space that does not begin a
/// colour (`accent = #ff0000  # red` keeps `#ff0000`).
fn strip_comment(line: &str) -> &str {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return "";
    }
    let bytes = trimmed.as_bytes();
    let cut = (1..bytes.len()).find(|&i| {
        bytes[i] == b'#'
            && bytes[i - 1].is_ascii_whitespace()
            && !bytes.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
    });
    cut.map_or(trimmed, |i| trimmed[..i].trim_end())
}

/// One `key = value` line applied to the palette.
fn apply_line(p: &mut Palette, dark: &mut bool, key: &str, value: &str) -> Result<(), String> {
    match key {
        "dark" => {
            *dark = match value {
                "true" | "yes" | "on" => true,
                "false" | "no" | "off" => false,
                _ => return Err(format!("dark must be true or false, not '{value}'")),
            };
        }
        "button_disabled_opacity" => {
            let opacity: f32 = value
                .parse()
                .map_err(|_| format!("not a number: '{value}'"))?;
            p.button_disabled_opacity = opacity.clamp(0.1, 1.0);
        }
        "graph" => {
            let colours: Vec<Color32> = value.split(',').filter_map(parse_colour).collect();
            let graph: [Color32; 6] = colours
                .try_into()
                .map_err(|_| "graph needs six colours, separated by commas".to_string())?;
            p.graph = graph;
        }
        _ => {
            let colour =
                parse_colour(value).ok_or_else(|| format!("'{value}' is not a #rrggbb colour"))?;
            if !set_colour(p, key, colour) {
                return Err(format!("unknown token '{key}'"));
            }
        }
    }
    Ok(())
}

/// A complete `.skin` file for `skin`: every token, ready to edit.
pub fn format_skin(skin: &Skin) -> String {
    let p = &skin.palette;
    let mut text = format!(
        "# HydraMerge skin \"{}\". Edit a colour and save: the window updates at once.\n\
         # Tokens left out come from `base`.\nbase = {}\ndark = {}\n\n",
        skin.name,
        if skin.dark { "dark" } else { "light" },
        skin.dark
    );
    for name in TOKENS {
        if let Some(colour) = get_colour(p, name) {
            text.push_str(&format!("{name} = {}\n", colour_text(colour)));
        }
    }
    let graph: Vec<String> = p.graph.iter().map(|c| colour_text(*c)).collect();
    text.push_str(&format!("graph = {}\n", graph.join(", ")));
    text.push_str(&format!(
        "button_disabled_opacity = {}\n",
        p.button_disabled_opacity
    ));
    text
}

pub fn skins_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(SKINS_DIRNAME)
}

/// Every `*.skin` in the skins folder, by file name, each with the warnings of its text.
pub fn load_custom_skins(config_dir: &Path, builtins: &[Skin]) -> Vec<(Skin, Vec<String>)> {
    let Ok(entries) = fs::read_dir(skins_dir(config_dir)) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == SKIN_EXTENSION))
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|file| {
            let text = fs::read_to_string(&file).ok()?;
            let stem = file.file_stem()?.to_string_lossy().into_owned();
            let taken = builtins.iter().any(|s| s.name.eq_ignore_ascii_case(&stem)) || stem == AUTO;
            let name = if taken {
                format!("{stem} (custom)")
            } else {
                stem
            };
            let (skin, warnings) = parse_skin(&name, &text, builtins);
            Some((
                Skin {
                    file: Some(file),
                    ..skin
                },
                warnings,
            ))
        })
        .collect()
}

/// Writes `skin` as a new file `<name>.skin` (with " 2", " 3"… when taken) and returns its path.
pub fn create_skin_file(config_dir: &Path, skin: &Skin, name: &str) -> AppResult<PathBuf> {
    let dir = skins_dir(config_dir);
    fs::create_dir_all(&dir)
        .map_err(|e| AppError::io(&format!("cannot create {}", dir.display()), &e))?;
    let base: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let file = (1..)
        .map(|n| match n {
            1 => dir.join(format!("{base}.{SKIN_EXTENSION}")),
            n => dir.join(format!("{base} {n}.{SKIN_EXTENSION}")),
        })
        .find(|path| !path.exists())
        .unwrap_or_else(|| dir.join(format!("{base}.{SKIN_EXTENSION}")));
    let named = Skin {
        name: base.clone(),
        ..skin.clone()
    };
    fs::write(&file, format_skin(&named))
        .map_err(|e| AppError::io(&format!("cannot write {}", file.display()), &e))?;
    Ok(file)
}

/// The chosen skin's name; `Auto` when none is stored.
pub fn load_skin_choice(config_dir: &Path) -> String {
    fs::read_to_string(config_dir.join(CHOICE_FILENAME))
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|name| !name.is_empty() && !name.contains('\n'))
        .unwrap_or_else(|| AUTO.to_string())
}

pub fn save_skin_choice(config_dir: &Path, name: &str) -> AppResult<()> {
    fs::create_dir_all(config_dir)
        .map_err(|e| AppError::io(&format!("cannot create {}", config_dir.display()), &e))?;
    let file = config_dir.join(CHOICE_FILENAME);
    fs::write(&file, format!("{name}\n"))
        .map_err(|e| AppError::io(&format!("cannot write {}", file.display()), &e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_overrides_tokens_of_its_base() {
        let known = builtin_skins();
        let text = "# my skin\nbase = Nord\naccent = #ff0000   # red\ngraph = #000001,#000002,#000003,#000004,#000005,#000006\n";
        let (skin, warnings) = parse_skin("Mine", text, &known);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(skin.dark);
        assert_eq!(skin.palette.accent, hex(0xff0000));
        let nord = known.iter().find(|s| s.name == "Nord").expect("nord");
        assert_eq!(
            skin.palette.bg, nord.palette.bg,
            "the rest comes from the base"
        );
        assert_eq!(skin.palette.graph[5], hex(0x000006));
    }

    #[test]
    fn bad_lines_become_warnings_and_the_rest_applies() {
        let (skin, warnings) = parse_skin(
            "x",
            "base = nope\nacent = #fff000\nbg = blue\ndark = maybe\ntext = #010203\n",
            &[],
        );
        assert_eq!(warnings.len(), 4, "{warnings:?}");
        assert!(warnings[0].starts_with("line 1: unknown base"));
        assert_eq!(skin.palette.text, hex(0x010203));
        assert_eq!(skin.palette.bg, DARK.bg);
    }

    #[test]
    fn a_written_skin_reads_back_the_same() {
        for skin in builtin_skins() {
            let (back, warnings) = parse_skin(&skin.name, &format_skin(&skin), &[]);
            assert!(warnings.is_empty(), "{}: {warnings:?}", skin.name);
            assert_eq!(back.palette, skin.palette, "{}", skin.name);
            assert_eq!(back.dark, skin.dark, "{}", skin.name);
        }
    }

    #[test]
    fn custom_files_load_and_new_files_get_free_names() {
        let tmp = tempfile::tempdir().expect("tmp");
        let dir = tmp.path();
        let known = builtin_skins();
        assert!(load_custom_skins(dir, &known).is_empty());
        let first = create_skin_file(dir, &known[3], "Night/Owl").expect("create");
        let second = create_skin_file(dir, &known[3], "Night/Owl").expect("create again");
        assert_eq!(
            first.file_name().and_then(|n| n.to_str()),
            Some("Night_Owl.skin")
        );
        assert_eq!(
            second.file_name().and_then(|n| n.to_str()),
            Some("Night_Owl 2.skin")
        );
        fs::write(skins_dir(dir).join("Nord.skin"), "base = light\n").expect("write");
        let loaded = load_custom_skins(dir, &known);
        let names: Vec<&str> = loaded.iter().map(|(s, _)| s.name.as_str()).collect();
        assert_eq!(names, vec!["Night_Owl 2", "Night_Owl", "Nord (custom)"]);
        assert!(!loaded[2].0.dark);
    }

    #[test]
    fn choice_round_trip() {
        let tmp = tempfile::tempdir().expect("tmp");
        assert_eq!(load_skin_choice(tmp.path()), AUTO);
        save_skin_choice(tmp.path(), "Monokai").expect("save");
        assert_eq!(load_skin_choice(tmp.path()), "Monokai");
    }
}
