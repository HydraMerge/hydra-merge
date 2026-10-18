//! The pictures: what to click, type and press to reach each screen. Coordinates are for the
//! 1400 x 820 window; the first click in a fresh window lands on the layout without tabs, which
//! sits 14 points higher than the one with a repository open.

use eframe::egui::{self, pos2, Key, Modifiers};
use egui_kittest::kittest::Queryable;

use super::{open, settle, shot, Window, World};

/// Frames to wait after an input: enough for the git threads to answer.
const AFTER_INPUT: usize = 45;

fn pointer(window: &mut Window, pos: egui::Pos2, button: egui::PointerButton) {
    let press = |pressed| egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    };
    window
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(pos));
    window.step();
    window.input_mut().events.push(press(true));
    window.step();
    window.input_mut().events.push(press(false));
    settle(window, AFTER_INPUT);
}

fn click(window: &mut Window, x: f32, y: f32) {
    pointer(window, pos2(x, y), egui::PointerButton::Primary);
}

fn right_click(window: &mut Window, x: f32, y: f32) {
    pointer(window, pos2(x, y), egui::PointerButton::Secondary);
}

fn press(window: &mut Window, key: Key, modifiers: Modifiers) {
    for pressed in [true, false] {
        window.input_mut().events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        });
        window.step();
    }
    settle(window, AFTER_INPUT);
}

fn type_text(window: &mut Window, text: &str) {
    window
        .input_mut()
        .events
        .push(egui::Event::Text(text.to_string()));
    settle(window, AFTER_INPUT);
}

fn escape(window: &mut Window) {
    press(window, Key::Escape, Modifiers::NONE);
}

/// Opens the command palette (⌘⇧P) and types `text`.
fn palette(window: &mut Window, text: &str) {
    press(window, Key::P, Modifiers::COMMAND | Modifiers::SHIFT);
    type_text(window, text);
}

/// Types into the palette and runs the first match.
fn run(window: &mut Window, text: &str) {
    palette(window, text);
    press(window, Key::Enter, Modifiers::NONE);
}

/// Rows of the Repositories column: its first row is at y = 108 while no tab is open, 122 after.
const FIRST_ROW: f32 = 122.0;
const ROW: f32 = 25.0;

fn repo_row(index: usize) -> f32 {
    FIRST_ROW + ROW * index as f32
}

// Positions in the sorted list of repositories.
const WEB_PORTAL: usize = 2;
const TERRAFORM: usize = 8;
const AUTH_SERVICE: usize = 9;
const PAYMENTS_API: usize = 11;
const PARTNER_SDK: usize = 13;

/// Four tabs, `payments-api` last: its first worktree has staged, unstaged and new files.
fn workday(window: &mut Window) {
    click(window, 58.0, 108.0 + ROW * WEB_PORTAL as f32);
    for index in [AUTH_SERVICE, TERRAFORM, PAYMENTS_API] {
        click(window, 58.0, repo_row(index));
    }
}

// Rows of the Commits column (y) once `payments-api` is open.
const COMMIT_RETRY_WEBHOOK: f32 = 414.0;

/// The pictures every theme gets: the window at work, the Summary, the palette.
fn main_views(window: &mut Window, theme: &str) {
    workday(window);
    click(window, 1077.0, 137.0);
    type_text(window, "feat: sign refund webhooks with HMAC-SHA256");
    shot(window, &format!("summary-{theme}"));
    click(window, 560.0, COMMIT_RETRY_WEBHOOK);
    shot(window, &format!("commit-{theme}"));
    palette(window, "worktree");
    shot(window, &format!("palette-{theme}"));
    escape(window);
}

pub fn light(world: &World) {
    let mut window = open(world, "light");
    main_views(&mut window, "light");
}

pub fn dark(world: &World) {
    let mut window = open(world, "dark");
    main_views(&mut window, "dark");
    finding_things(&mut window);
    worktrees(&mut window);
    branches(&mut window);
    settings(&mut window);
    skins(&mut window);
    trust(&mut window);
    repositories(&mut window);
}

/// Filter the history, compare two branches, look at a stash.
fn finding_things(window: &mut Window) {
    press(window, Key::F, Modifiers::COMMAND);
    type_text(window, "webhook");
    shot(window, "filter-dark");
    escape(window);
    run(window, "compare with: main");
    shot(window, "compare-dark");
    click(window, 1362.0, 120.0);
    click(window, 265.0, 681.0);
    shot(window, "stash-dark");
}

/// A clean worktree, the ⋯ menu and the question before a worktree is removed.
fn worktrees(window: &mut Window) {
    click(window, 265.0, 177.0);
    shot(window, "worktree-dark");
    click(window, 889.0, 54.0);
    shot(window, "worktree-menu-dark");
    escape(window);
    run(window, "remove worktree");
    shot(window, "confirm-remove-dark");
    escape(window);
}

/// A branch that exists only on the remote, and a free local branch: normal and sparse worktrees.
fn branches(window: &mut Window) {
    click(window, 253.0, 479.0);
    click(window, 300.0, 504.0);
    shot(window, "remote-branch-dark");
    click(window, 275.0, 278.0);
    right_click(window, 290.0, 303.0);
    shot(window, "branch-menu-dark");
    click(window, 375.0, 341.0);
    // cmd/, internal/ and migrations/ of the list of folders.
    for folder_y in [393.0, 437.0, 459.0] {
        click(window, 497.0, folder_y);
    }
    shot(window, "sparse-dialog-dark");
    click(window, 881.0, 514.0);
    settle(window, 90);
    // The new worktree is the first of the list; its card names the folders it holds.
    click(window, 265.0, 152.0);
    shot(window, "sparse-worktree-dark");
}

/// The gear menu and the date format list inside it.
fn settings(window: &mut Window) {
    click(window, 1380.0, 54.0);
    shot(window, "settings-dark");
    click(window, 1253.0, 148.0);
    window.get_by_label_contains("Date Format").click();
    settle(window, AFTER_INPUT);
    shot(window, "date-format-dark");
    escape(window);
    escape(window);
}

/// Three of the built-in skins on the same screen.
fn skins(window: &mut Window) {
    click(window, 560.0, COMMIT_RETRY_WEBHOOK);
    for (query, name) in [
        ("skin: nord", "skin-nord"),
        ("skin: solarized light", "skin-solarized-light"),
        ("skin: monokai", "skin-monokai"),
    ] {
        run(window, query);
        shot(window, name);
    }
    run(window, "skin: auto");
}

/// A repository that came as a folder: the question, then the repository after the answer.
fn trust(window: &mut Window) {
    click(window, 58.0, repo_row(PARTNER_SDK));
    shot(window, "trust-card-dark");
    window.get_by_label("Trust this repository").click();
    settle(window, AFTER_INPUT);
    shot(window, "trusted-dark");
}

/// Narrow the list of repositories, and the window that opens one by path.
fn repositories(window: &mut Window) {
    press(window, Key::L, Modifiers::COMMAND);
    type_text(window, "api");
    // The filter field takes the first row; `payments-api` is the second of the narrowed list.
    click(window, 58.0, repo_row(2));
    shot(window, "repositories-filter-dark");
    click(window, 1382.0, 18.0);
    shot(window, "open-repository-dark");
    escape(window);
}
