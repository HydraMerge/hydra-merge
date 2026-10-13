//! Renders the window offscreen on temporary repositories and writes PNGs to `target/ui-shots/`.
//! Needs a GPU (Metal on macOS), so it is ignored by default:
//! `cargo test --test ui_shots -- --ignored`.

#![cfg(unix)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{fake_smerge, git, make_bare_layout, test_config, tmp};
use eframe::egui;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use hydra_merge::app::WorktreeBarApp;

const SIZE: [f32; 2] = [1400.0, 820.0];

fn shots_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/ui-shots");
    fs::create_dir_all(&dir).expect("shots dir");
    dir
}

fn git_str(config: &hydra_merge::model::Config, args: &[&str]) {
    let os: Vec<&std::ffi::OsStr> = args.iter().map(std::ffi::OsStr::new).collect();
    git(config, &os);
}

/// A repository with a few commits, a branch ahead of master and a remote-tracking branch.
fn demo(tmp: &Path, config: &hydra_merge::model::Config) -> PathBuf {
    let root = make_bare_layout(
        tmp,
        config,
        "grp/demo",
        &["master", "test"],
        &["master", "test"],
    );
    let wt = root.join("test");
    let wt_s = wt.to_str().expect("utf8");
    fs::create_dir_all(wt.join("src")).expect("mkdir");
    fs::write(
        wt.join("src/main.rs"),
        "use std::path::Path;\n\n/* entry */\nfn main() {\n    let p = Path::new(\"/tmp\"); // привет\n    println!(\"{}\", p.display());\n}\n",
    )
    .expect("write");
    git_str(config, &["-C", wt_s, "add", "."]);
    git_str(config, &["-C", wt_s, "commit", "-m", "feat: add main"]);
    fs::write(
        wt.join("site.yml"),
        "- name: install nginx\n  become: yes\n",
    )
    .expect("write");
    git_str(config, &["-C", wt_s, "add", "."]);
    git_str(config, &["-C", wt_s, "commit", "-m", "chore: ansible site"]);
    let bare = format!("--git-dir={}", root.join(".bare").display());
    // master moves on by itself, then test merges it: two lanes and a merge dot.
    let master = root.join("master");
    let master_s = master.to_str().expect("utf8");
    fs::write(master.join("CHANGELOG.md"), "# 1.1\n").expect("write");
    git_str(config, &["-C", master_s, "add", "CHANGELOG.md"]);
    git_str(
        config,
        &["-C", master_s, "commit", "-m", "docs: changelog for 1.1"],
    );
    git_str(config, &["-C", wt_s, "merge", "--no-edit", "master"]);
    fs::write(
        wt.join("site.yml"),
        "- name: install nginx\n  become: true\n",
    )
    .expect("write");
    git_str(config, &["-C", wt_s, "commit", "-am", "fix: become true"]);
    git_str(config, &[&bare, "tag", "v1.0", "master~1"]);
    for i in 1..=12 {
        make_bare_layout(
            tmp,
            config,
            &format!("grp/service-{i:02}"),
            &["master"],
            &["master"],
        );
    }
    git_str(
        config,
        &[&bare, "update-ref", "refs/remotes/origin/mr/162", "master"],
    );
    git_str(
        config,
        &[&bare, "update-ref", "refs/remotes/origin/test", "test"],
    );
    root
}

fn settle(harness: &mut Harness<'_, WorktreeBarApp>) {
    for _ in 0..150 {
        harness.step();
        std::thread::sleep(Duration::from_millis(15));
    }
}

/// Moves the pointer to `pos` and clicks there.
fn click(harness: &mut Harness<'_, WorktreeBarApp>, pos: egui::Pos2) {
    let press = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(pos));
    harness.step();
    harness.input_mut().events.push(press(true));
    harness.step();
    harness.input_mut().events.push(press(false));
    settle(harness);
}

/// A mouse wheel turned over `pos`: `delta` in lines, negative y scrolls down.
fn wheel(harness: &mut Harness<'_, WorktreeBarApp>, pos: egui::Pos2, delta: egui::Vec2) {
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(pos));
    harness.step();
    harness.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta,
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    settle(harness);
}

/// Presses and releases `key` with `modifiers`.
fn key(harness: &mut Harness<'_, WorktreeBarApp>, key: egui::Key, modifiers: egui::Modifiers) {
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        });
        harness.step();
    }
    settle(harness);
}

fn type_text(harness: &mut Harness<'_, WorktreeBarApp>, text: &str) {
    harness
        .input_mut()
        .events
        .push(egui::Event::Text(text.to_string()));
    settle(harness);
}

/// Opens the palette, types `text` and runs the first match.
fn run_palette(harness: &mut Harness<'_, WorktreeBarApp>, text: &str) {
    let shift_command = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
    key(harness, egui::Key::P, shift_command);
    type_text(harness, text);
    key(harness, egui::Key::Enter, egui::Modifiers::NONE);
}

/// Opens the palette and types `text`; nothing is run.
fn run_palette_only(harness: &mut Harness<'_, WorktreeBarApp>, text: &str) {
    let shift_command = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
    key(harness, egui::Key::P, shift_command);
    type_text(harness, text);
}

fn shot(harness: &mut Harness<'_, WorktreeBarApp>, name: &str) {
    let image = harness.render().expect("render");
    image
        .save(shots_dir().join(format!("{name}.png")))
        .expect("save png");
}

/// Staged and unstaged edits in `master`, so the Summary pane has files and a diff.
fn make_dirty(root: &Path, config: &hydra_merge::model::Config) {
    let wt = root.join("master");
    let wt_s = wt.to_str().expect("utf8");
    fs::write(wt.join("README"), "seed\nmore text\n").expect("write");
    fs::write(wt.join("app.py"), "def main():\n    return 42  # answer\n").expect("write");
    git_str(config, &["-C", wt_s, "add", "app.py"]);
    fs::write(wt.join("notes.txt"), "draft\n").expect("write");
}

#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn window_screenshots() {
    let t = tmp();
    let config = test_config(&t.path);
    fake_smerge(&t.path);
    let root = demo(&t.path, &config);
    make_dirty(&root, &config);
    let settings = t.path.join("settings");
    fs::create_dir_all(&settings).expect("settings");
    fs::write(settings.join("autoscan.txt"), "on\n").expect("autoscan");
    fs::create_dir_all(settings.join("skins")).expect("skins dir");
    fs::write(
        settings.join("skins/Ocean.skin"),
        "# a custom skin\nbase = Nord\naccent = #ff8800\nbadge_head_bg = #ff8800\n",
    )
    .expect("skin");
    for (theme, egui_theme) in [("light", egui::Theme::Light), ("dark", egui::Theme::Dark)] {
        fs::write(settings.join("theme.txt"), format!("{theme}\n")).expect("theme");
        let (cfg, dir) = (config.clone(), settings.clone());
        let mut harness = Harness::builder()
            .with_size(SIZE)
            .with_theme(egui_theme)
            .wgpu()
            .build_eframe(move |cc| WorktreeBarApp::with_settings_dir(cc, cfg, Some(dir)));
        settle(&mut harness);
        shot(&mut harness, &format!("start-{theme}"));
        click(&mut harness, egui::pos2(40.0, 108.0));
        shot(&mut harness, &format!("summary-{theme}"));
        click(&mut harness, egui::pos2(1385.0, 18.0));
        type_text(&mut harness, "/etc");
        key(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
        shot(&mut harness, &format!("open-repo-{theme}"));
        key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
        click(&mut harness, egui::pos2(889.0, 54.0));
        shot(&mut harness, &format!("menu-worktree-{theme}"));
        key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
        click(&mut harness, egui::pos2(1382.0, 54.0));
        shot(&mut harness, &format!("menu-settings-{theme}"));
        key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
        wheel(&mut harness, egui::pos2(600.0, 15.0), egui::vec2(0.0, -6.0));
        shot(&mut harness, &format!("tabs-wheel-{theme}"));
        click(&mut harness, egui::pos2(592.0, 155.0));
        shot(&mut harness, &format!("commit-{theme}"));
        key(
            &mut harness,
            egui::Key::OpenBracket,
            egui::Modifiers::COMMAND,
        );
        shot(&mut harness, &format!("nav-back-{theme}"));
        click(&mut harness, egui::pos2(640.0, 54.0));
        harness
            .input_mut()
            .events
            .push(egui::Event::Text("pu".to_string()));
        settle(&mut harness);
        shot(&mut harness, &format!("palette-{theme}"));
        key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
        let shift_command = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        key(&mut harness, egui::Key::P, shift_command);
        type_text(&mut harness, "compare with: test");
        key(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
        shot(&mut harness, &format!("compare-{theme}"));
        click(&mut harness, egui::pos2(592.0, 118.0));
        key(&mut harness, egui::Key::F, egui::Modifiers::COMMAND);
        type_text(&mut harness, "chore");
        shot(&mut harness, &format!("filter-{theme}"));
        key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
        click(&mut harness, egui::pos2(592.0, 118.0));
        click(&mut harness, egui::pos2(877.0, 394.0));
        shot(&mut harness, &format!("selected-{theme}"));
        for skin in ["Monokai", "Ocean", "Solarized Light"] {
            run_palette(&mut harness, &format!("skin: {}", skin.to_lowercase()));
            shot(
                &mut harness,
                &format!("skin-{}-{theme}", skin.replace(' ', "-").to_lowercase()),
            );
        }
        click(&mut harness, egui::pos2(1382.0, 54.0));
        harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(egui::pos2(1300.0, 210.0)));
        settle(&mut harness);
        shot(&mut harness, &format!("skin-menu-{theme}"));
        key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
        run_palette(&mut harness, "skin: auto");
        click(&mut harness, egui::pos2(1382.0, 54.0));
        harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(egui::pos2(1300.0, 211.0)));
        settle(&mut harness);
        shot(&mut harness, &format!("open-with-{theme}"));
        key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
        // The clean worktree `test`: the Summary shows its card instead of a commit form.
        click(&mut harness, egui::pos2(262.0, 177.0));
        shot(&mut harness, &format!("clean-{theme}"));
        key(&mut harness, egui::Key::L, egui::Modifiers::COMMAND);
        type_text(&mut harness, "service-1");
        shot(&mut harness, &format!("repo-filter-{theme}"));
        key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    }
}

/// A repository whose own config names a clean filter; every run of the filter is recorded in
/// `<tmp>/clean.marker`. The tracked file is edited afterwards, so reading the repository would
/// run the filter.
fn risky(tmp: &Path, config: &hydra_merge::model::Config) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let root = make_bare_layout(tmp, config, "grp/aaa-risky", &["master"], &["master"]);
    let wt = root.join("master");
    let wt_s = wt.to_str().expect("utf8");
    fs::write(wt.join(".gitattributes"), "*.dat filter=f\n").expect("attributes");
    fs::write(wt.join("a.dat"), "one\n").expect("write");
    git_str(config, &["-C", wt_s, "add", "."]);
    git_str(config, &["-C", wt_s, "commit", "-m", "add data"]);
    let script = tmp.join("clean.sh");
    let marker = tmp.join("clean.marker");
    fs::write(
        &script,
        format!("#!/bin/sh\necho ran >> '{}'\ncat\n", marker.display()),
    )
    .expect("script");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
    let bare = format!("--git-dir={}", root.join(".bare").display());
    git_str(
        config,
        &[
            &bare,
            "config",
            "filter.f.clean",
            script.to_str().expect("utf8"),
        ],
    );
    fs::write(wt.join("a.dat"), "two\n").expect("edit");
    root
}

#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn trust_card_screenshots() {
    let t = tmp();
    let config = test_config(&t.path);
    fake_smerge(&t.path);
    risky(&t.path, &config);
    make_bare_layout(&t.path, &config, "grp/plain", &["master"], &["master"]);
    let marker = t.path.join("clean.marker");
    let settings = t.path.join("settings");
    fs::create_dir_all(&settings).expect("settings");
    fs::write(settings.join("autoscan.txt"), "on\n").expect("autoscan");
    for (theme, egui_theme) in [("light", egui::Theme::Light), ("dark", egui::Theme::Dark)] {
        fs::write(settings.join("theme.txt"), format!("{theme}\n")).expect("theme");
        let _ = fs::remove_file(&marker);
        let (cfg, dir) = (config.clone(), settings.clone());
        let mut harness = Harness::builder()
            .with_size(SIZE)
            .with_theme(egui_theme)
            .wgpu()
            .build_eframe(move |cc| WorktreeBarApp::with_settings_dir(cc, cfg, Some(dir)));
        settle(&mut harness);
        shot(&mut harness, &format!("trust-card-{theme}"));
        assert!(
            harness
                .query_by_label("This repository is not trusted")
                .is_some(),
            "the card is not shown"
        );
        assert!(
            !marker.exists(),
            "the clean filter ran before the user trusted"
        );
        for git_button in ["↑", "↓", "↻"] {
            assert!(
                harness
                    .get_by_label(git_button)
                    .accesskit_node()
                    .is_disabled(),
                "{git_button} is enabled in an untrusted repository"
            );
        }
        assert!(
            !harness.get_by_label("Open").accesskit_node().is_disabled(),
            "Open only starts the editor and stays enabled"
        );
        assert!(
            harness.query_by_label("BRANCHES (0)").is_none(),
            "a count of 0 branches is shown for a list that was not read"
        );
        assert!(harness.query_by_label("BRANCHES").is_some());
        run_palette_only(&mut harness, "fetch");
        shot(&mut harness, &format!("trust-palette-{theme}"));
        key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);

        harness.get_by_label("Trust this repository").click();
        settle(&mut harness);
        shot(&mut harness, &format!("trust-after-{theme}"));
        assert!(
            harness
                .query_by_label("This repository is not trusted")
                .is_none(),
            "the card stays after Trust"
        );
        assert!(
            marker.exists(),
            "after Trust the repository is read as usual"
        );
        assert!(
            !harness.get_by_label("↻").accesskit_node().is_disabled(),
            "Fetch stays disabled after Trust"
        );
        assert!(harness.query_by_label("BRANCHES (1)").is_some());

        run_palette(&mut harness, "stop trusting");
        shot(&mut harness, &format!("trust-revoked-{theme}"));
        assert!(
            harness
                .query_by_label("This repository is not trusted")
                .is_some(),
            "the card does not come back after Stop Trusting"
        );
    }
}
