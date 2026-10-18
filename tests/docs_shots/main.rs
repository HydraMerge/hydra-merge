//! Renders HydraMerge on a made-up company and writes the PNGs for the documentation.
//! Needs a GPU (Metal on macOS), so it is ignored by default:
//!
//! ```text
//! cargo test --test docs_shots -- --ignored
//! ```
//!
//! The pictures go to `target/docs-shots/`; set `DOCS_SHOTS_OUT=docs/screenshots` to refresh the
//! ones the README and the guide show. The company lives in `/tmp/elena` while the test runs, a
//! short path that keeps temporary directories out of the pictures; it is removed afterwards.
#![cfg(unix)]

mod demo;
mod hero;
mod others;
mod scenes;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui;
use egui_kittest::Harness;
use hydra_merge::app::WorktreeBarApp;
use hydra_merge::model::Config;

use demo::{Git, ME};

const SIZE: [f32; 2] = [1400.0, 820.0];
/// Where the made-up home directory is. `/tmp` is a link to `/private/tmp` on macOS.
const BASE: &str = "/tmp/elena";

pub type Window = Harness<'static, WorktreeBarApp>;

pub struct World {
    base: PathBuf,
    config: Config,
    settings: PathBuf,
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn out_dir() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = match std::env::var_os("DOCS_SHOTS_OUT") {
        Some(rel) => manifest.join(rel),
        None => manifest.join("target/docs-shots"),
    };
    fs::create_dir_all(&dir).expect("output dir");
    dir
}

/// The company on disk: repositories under `~/repos`, and the git config of the person using
/// the app. `HOME` points at it, so the window shows `~/repos` the way a real one would.
fn world() -> World {
    let _ = fs::remove_dir_all(BASE);
    fs::create_dir_all(BASE).expect("base");
    let base = fs::canonicalize(BASE).expect("canonical base");
    let (work, seeds) = (base.join("repos"), base.join(".seeds"));
    fs::create_dir_all(&work).expect("repos");
    let git = Git::new(&base);
    let root = hero::build(&git, &work, &seeds);
    others::build(&git, &work, &seeds);
    hero::make_busy(&git, &root);
    let gitconfig = base.join(".gitconfig");
    let identity = format!("[user]\n\tname = {}\n\temail = {}\n", ME.name, ME.email);
    fs::write(&gitconfig, identity).expect("gitconfig");
    std::env::set_var("HOME", &base);
    let pair = |k: &str, v: &Path| (k.into(), v.as_os_str().to_os_string());
    let config = Config {
        roots: vec![fs::canonicalize(&work).expect("repos")],
        smerge: base.join("no-such-editor"),
        git_env: vec![
            pair("GIT_CONFIG_GLOBAL", &gitconfig),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            pair("HOME", &base),
        ],
        trusted: Default::default(),
    };
    let settings = base.join(".settings");
    fs::create_dir_all(&settings).expect("settings");
    fs::write(settings.join("autoscan.txt"), "on\n").expect("autoscan");
    World {
        base,
        config,
        settings,
    }
}

/// A fresh window in the `theme` ("dark" or "light"), scanned and settled.
pub fn open(world: &World, theme: &str) -> Window {
    // Every window starts as a first start: the last one's tabs are not restored.
    let _ = fs::remove_file(world.settings.join("session.txt"));
    fs::write(world.settings.join("theme.txt"), format!("{theme}\n")).expect("theme");
    let egui_theme = if theme == "light" {
        egui::Theme::Light
    } else {
        egui::Theme::Dark
    };
    let (config, dir) = (world.config.clone(), world.settings.clone());
    let mut window = Harness::builder()
        .with_size(SIZE)
        .with_theme(egui_theme)
        .wgpu()
        .build_eframe(move |cc| WorktreeBarApp::with_settings_dir(cc, config, Some(dir)));
    settle(&mut window, 150);
    window
}

/// Lets background work (git runs on threads) finish and the window draw it.
pub fn settle(window: &mut Window, frames: usize) {
    for _ in 0..frames {
        window.step();
        std::thread::sleep(Duration::from_millis(15));
    }
}

pub fn shot(window: &mut Window, name: &str) {
    let image = window.render().expect("render");
    image
        .save(out_dir().join(format!("{name}.png")))
        .expect("save png");
}

#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn docs_screenshots() {
    let world = world();
    scenes::light(&world);
    scenes::dark(&world);
}
