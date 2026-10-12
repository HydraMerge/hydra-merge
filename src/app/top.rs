//! Repository tabs (folder icon, close button, ▾ list, +), the path row,
//! and the bottom status/error bar.

use super::*;

/// Width of the ▾ and + buttons at the right end of the tab bar.
const TAB_TOOLS: f32 = 56.0;
/// Folder icon, gaps and the close cross around the tab text.
const TAB_CHROME: f32 = 52.0;
const TAB_MIN_WIDTH: f32 = 80.0;
const TAB_MAX_WIDTH: f32 = 220.0;

/// What happened to one tab this frame.
struct TabClick {
    activate: bool,
    close: bool,
}

impl WorktreeBarApp {
    pub(super) fn tab_bar(&mut self, ui: &mut Ui, p: &Palette, repos: &[Repo]) -> Vec<Action> {
        let mut out = Vec::new();
        // Tabs start at the window edge and touch the toolbar.
        let frame = Frame::new().fill(p.header_bg).inner_margin(Margin {
            left: 0,
            right: 4,
            top: 0,
            bottom: 0,
        });
        Panel::top("tabs").frame(frame).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let tabs = (ui.available_width() - TAB_TOOLS).max(120.0);
                ui.allocate_ui_with_layout(
                    vec2(tabs, TAB_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Min),
                    |ui| out.extend(self.repo_tabs(ui, p, repos)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(2.0);
                    if icon_button(ui, p, Icon::Plus, "Open a repository by path").clicked() {
                        out.push(Action::OpenRepoDialog);
                    }
                    out.extend(self.repo_menu(ui, p, repos));
                });
            });
        });
        out
    }

    /// Every repository, including closed tabs; picking one reopens its tab.
    fn repo_menu(&self, ui: &mut Ui, p: &Palette, repos: &[Repo]) -> Option<Action> {
        let mut picked = None;
        let button = icon_button(ui, p, Icon::Chevron, "All repositories");
        egui::Popup::menu(&button).show(|ui| {
            ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                for repo in repos {
                    let active = self.active_root.as_ref() == Some(&repo.root);
                    if ui.selectable_label(active, &repo.name).clicked() {
                        picked = Some(Action::Activate(repo.root.clone()));
                        ui.close();
                    }
                }
            });
        });
        picked
    }

    pub(super) fn repo_tabs(&mut self, ui: &mut Ui, p: &Palette, repos: &[Repo]) -> Vec<Action> {
        let shown: Vec<&Repo> = repos
            .iter()
            .filter(|repo| !self.closed_tabs.contains(&repo.root))
            .collect();
        if shown.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new("No repositories").color(p.header_fg));
            return Vec::new();
        }
        // No scroll bar. A mouse wheel turns vertical steps into horizontal
        // scrolling; a trackpad swipes the strip directly.
        ui.style_mut().always_scroll_the_only_direction = true;
        let mut out = Vec::new();
        ScrollArea::horizontal()
            .id_salt("repo-tabs")
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for repo in &shown {
                        out.extend(self.one_tab(ui, p, repos, repo));
                    }
                });
            });
        self.reveal_tab = false;
        out
    }

    fn one_tab(&self, ui: &mut Ui, p: &Palette, repos: &[Repo], repo: &Repo) -> Option<Action> {
        let active = self.active_root.as_ref() == Some(&repo.root);
        let label = tab_label(repos, repo);
        let (response, click) = tab(ui, p, &label, active);
        let response = response.on_hover_text(repo.root.display().to_string());
        if active && self.reveal_tab {
            response.scroll_to_me(None);
        }
        if click.close {
            Some(Action::CloseTab(repo.root.clone()))
        } else {
            click.activate.then(|| Action::Activate(repo.root.clone()))
        }
    }

    pub(super) fn bottom_bar(&mut self, ui: &mut Ui, p: &Palette) -> Option<Action> {
        let busy = self.busy();
        let fill = if self.error.is_some() {
            p.error_bg
        } else {
            p.bg
        };
        let shown = Panel::bottom("bottom")
            .frame(panel_frame(fill))
            .show(ui, |ui| match &self.error {
                Some(error) => {
                    let terminal = self.selected.clone();
                    error_row(ui, p, error, terminal)
                }
                None => {
                    ui.horizontal(|ui| {
                        if busy || self.status_lane.is_some() {
                            ui.add(Spinner::new());
                        }
                        ui.label(RichText::new(&self.status).color(p.text_muted));
                        self.link_button(ui, p)
                    })
                    .inner
                }
            });
        if self.error.is_some() {
            paint_stripe(ui, p, shown.response.rect);
        }
        shown.inner
    }
}

/// Icons drawn with shapes, so they do not depend on glyphs in the fonts.
#[derive(Clone, Copy)]
pub(super) enum Icon {
    /// A window with its left column filled: show or hide Locations.
    Sidebar,
    More,
    Chevron,
    Plus,
    Back,
    Forward,
    /// `>_` of the command palette.
    Prompt,
    /// Settings of the app.
    Gear,
}

impl WorktreeBarApp {
    /// "Open Merge Request ↗" after a push that printed a link, with × to hide it.
    fn link_button(&self, ui: &mut Ui, p: &Palette) -> Option<Action> {
        let url = self.panes.link.as_ref()?;
        let kind = if url.contains("merge_request") {
            "Merge Request"
        } else if url.contains("/pull") {
            "Pull Request"
        } else {
            "Link"
        };
        // The host is shown so a link printed by an unexpected server is noticed.
        let host = url
            .trim_start_matches("https://")
            .split('/')
            .next()
            .unwrap_or("");
        let label = format!("Open {kind} on {host} ↗");
        let open = ui
            .add(Button::new(RichText::new(label).color(p.accent)).frame(false))
            .on_hover_text(url)
            .clicked()
            .then(|| Action::OpenLink(url.clone()));
        let hide = Button::new(RichText::new("×").color(p.text_muted)).frame(false);
        let dismiss = ui.add(hide).clicked().then_some(Action::DismissLink);
        open.or(dismiss)
    }
}

/// A frameless icon button of the tab bar and toolbar.
pub(super) fn icon_button(ui: &mut Ui, p: &Palette, icon: Icon, hint: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 22.0), Sense::click());
    paint_icon_button(ui, p, rect, icon, &response);
    response.on_hover_text(hint)
}

/// An icon button that can be turned off: drawn faded and not clickable.
pub(super) fn icon_button_enabled(
    ui: &mut Ui,
    p: &Palette,
    icon: Icon,
    hint: &str,
    enabled: bool,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| icon_button(ui, p, icon, hint))
        .inner
}

/// The same button at a fixed place, for layouts that position it by hand.
pub(super) fn icon_button_at(
    ui: &mut Ui,
    p: &Palette,
    rect: Rect,
    icon: Icon,
    hint: &str,
) -> egui::Response {
    let response = ui.interact(rect, Id::new(("icon", hint)), Sense::click());
    paint_icon_button(ui, p, rect, icon, &response);
    response.on_hover_text(hint)
}

fn paint_icon_button(ui: &Ui, p: &Palette, rect: Rect, icon: Icon, response: &egui::Response) {
    let fill = if response.hovered() {
        p.button_hover_bg
    } else {
        p.input_bg.gamma_multiply(0.0)
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    paint_icon(ui, p, rect, icon);
}

/// Draws `icon` centred in `rect`.
pub(super) fn paint_icon(ui: &Ui, p: &Palette, rect: Rect, icon: Icon) {
    let color = p.header_fg;
    let painter = ui.painter();
    let c = rect.center();
    let stroke = Stroke::new(1.3, color);
    match icon {
        Icon::Sidebar => {
            let frame = Rect::from_center_size(c, vec2(14.0, 11.0));
            painter.rect_stroke(frame, 1.5, stroke, egui::StrokeKind::Inside);
            let strip = Rect::from_min_size(frame.min, vec2(5.0, frame.height()));
            painter.rect_filled(strip, 1.0, color);
        }
        Icon::Gear => {
            for n in 0..8 {
                let angle = n as f32 * std::f32::consts::FRAC_PI_4;
                let dir = vec2(angle.cos(), angle.sin());
                painter.line_segment([c + dir * 4.5, c + dir * 7.0], Stroke::new(2.0, color));
            }
            painter.circle_stroke(c, 4.5, stroke);
            painter.circle_filled(c, 1.6, color);
        }
        Icon::More => {
            for dx in [-5.0, 0.0, 5.0] {
                painter.circle_filled(c + vec2(dx, 0.0), 1.6, color);
            }
        }
        Icon::Chevron => {
            let points = vec![
                c + vec2(-4.0, -2.0),
                c + vec2(4.0, -2.0),
                c + vec2(0.0, 3.0),
            ];
            painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
        }
        Icon::Back | Icon::Forward | Icon::Prompt => paint_nav_icon(painter, c, icon, color),
        Icon::Plus => {
            painter.hline((c.x - 5.0)..=(c.x + 5.0), c.y, stroke);
            painter.vline(c.x, (c.y - 5.0)..=(c.y + 5.0), stroke);
        }
    }
}

/// One tab: folder icon, name, close cross. The active tab blends into the toolbar below it.
fn tab(ui: &mut Ui, p: &Palette, label: &str, active: bool) -> (egui::Response, TabClick) {
    let shown = fit_label(label, 28);
    let font = FontId::proportional(12.5);
    // Inactive tabs use the quieter sidebar text, so the active name stands out.
    let color = if active { p.text_strong } else { p.sidebar_fg };
    let galley = ui.painter().layout_no_wrap(shown, font, color);
    let width = (galley.size().x + TAB_CHROME).clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH);
    let (rect, response) = ui.allocate_exact_size(vec2(width, TAB_HEIGHT), Sense::click());
    let painter = ui.painter();
    // Square tabs side by side; the active one takes the toolbar colour and joins it.
    if active {
        painter.rect_filled(rect, 0.0, p.toolbar_bg);
    } else if response.hovered() {
        painter.rect_filled(rect, 0.0, p.toolbar_bg.gamma_multiply(0.45));
    }
    let separator = p.header_fg.gamma_multiply(0.22);
    painter.vline(
        rect.right() - 0.5,
        rect.y_range(),
        Stroke::new(1.0, separator),
    );
    let icon = if active {
        p.header_fg
    } else {
        p.header_fg.gamma_multiply(0.6)
    };
    folder_icon(painter, pos2(rect.left() + 15.0, rect.center().y), icon);
    let text_at = pos2(rect.left() + 28.0, rect.center().y - galley.size().y / 2.0);
    let close_rect =
        Rect::from_center_size(pos2(rect.right() - 13.0, rect.center().y), vec2(16.0, 16.0));
    let text_clip = Rect::from_min_max(rect.min, pos2(close_rect.left() - 2.0, rect.bottom()));
    painter
        .with_clip_rect(text_clip)
        .galley(text_at, galley, color);
    let strong = active || response.hovered();
    let close = close_button(ui, p, close_rect, response.id, (strong, color));
    let click = TabClick {
        activate: response.clicked(),
        close: close.clicked() || response.middle_clicked(),
    };
    (response, click)
}

/// The × of a tab; `look` is (tab active or hovered, text colour).
fn close_button(
    ui: &Ui,
    p: &Palette,
    rect: Rect,
    tab: Id,
    look: (bool, Color32),
) -> egui::Response {
    let close = ui.interact(rect, tab.with("close"), Sense::click());
    if close.hovered() {
        ui.painter().rect_filled(rect, 3.0, p.border);
    }
    let (strong, color) = look;
    let cross = if strong || close.hovered() {
        color
    } else {
        p.text_muted
    };
    let font = FontId::proportional(13.0);
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, "×", font, cross);
    close
}

/// ◀ ▶ and the `>_` of the palette field.
fn paint_nav_icon(painter: &egui::Painter, c: Pos2, icon: Icon, color: Color32) {
    match icon {
        Icon::Back | Icon::Forward => {
            let dir = if matches!(icon, Icon::Back) {
                -1.0
            } else {
                1.0
            };
            // The tip points the way it goes: left for back, right for forward.
            let points = vec![
                c + vec2(-3.0 * dir, -4.5),
                c + vec2(-3.0 * dir, 4.5),
                c + vec2(3.5 * dir, 0.0),
            ];
            painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
        }
        _ => {
            let frame = Rect::from_center_size(c, vec2(14.0, 11.0));
            painter.rect_stroke(
                frame,
                1.5,
                Stroke::new(1.0, color),
                egui::StrokeKind::Inside,
            );
            let a = frame.left_center() + vec2(3.0, -2.5);
            painter.line_segment([a, a + vec2(2.5, 2.5)], Stroke::new(1.0, color));
            painter.line_segment(
                [a + vec2(2.5, 2.5), a + vec2(0.0, 5.0)],
                Stroke::new(1.0, color),
            );
        }
    }
}

/// A small folder outline: a tab on the top left and the body.
fn folder_icon(painter: &egui::Painter, center: Pos2, color: Color32) {
    let body = Rect::from_center_size(center + vec2(0.0, 1.0), vec2(13.0, 9.0));
    let flap = Rect::from_min_size(body.min - vec2(0.0, 2.0), vec2(6.0, 3.0));
    painter.rect_filled(flap, 1.0, color);
    painter.rect_filled(body, 1.5, color);
}

pub(super) fn fit_label(label: &str, max_chars: usize) -> String {
    if label.chars().count() <= max_chars {
        return label.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let head: String = label.chars().take(keep).collect();
    format!("{head}…")
}

/// The error, its details, and for a sign-in failure a button that opens the worktree in
/// Terminal so the user can sign in once.
pub(super) fn error_row(
    ui: &mut Ui,
    p: &Palette,
    error: &AppError,
    worktree: Option<PathBuf>,
) -> Option<Action> {
    ui.horizontal(|ui| {
        ui.add_space(STRIPE_WIDTH + 4.0);
        ui.label(RichText::new(&error.message).strong().color(p.error_fg));
        let sign_in = (error.code == ErrorCode::AuthRequired)
            .then_some(worktree)
            .flatten();
        let terminal = sign_in.and_then(|path| {
            ui.button("Open in Terminal")
                .clicked()
                .then_some(Action::OpenTerminal(path))
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let close = Button::new(RichText::new("×").color(p.error_fg)).frame(false);
            ui.add(close).clicked().then_some(Action::DismissError)
        })
        .inner
        .or(terminal)
    })
    .inner
    .or_else(|| {
        if let Some(detail) = &error.detail {
            CollapsingHeader::new(RichText::new("Details").color(p.error_fg))
                .id_salt("error_detail")
                .show(ui, |ui| {
                    ui.label(RichText::new(detail).monospace().color(p.error_fg))
                });
        }
        None
    })
}

pub(super) fn paint_stripe(ui: &Ui, p: &Palette, panel: egui::Rect) {
    let stripe = egui::Rect::from_min_max(
        panel.min,
        egui::pos2(panel.min.x + STRIPE_WIDTH, panel.max.y),
    );
    ui.painter().rect_filled(stripe, 0.0, p.error_accent);
}
