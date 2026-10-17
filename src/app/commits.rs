//! The commit list: a working-copy row on top, then commits with the branch graph. Line one is
//! the subject, then the ref badges and the file count; line two is the author and the date.

use eframe::egui::epaint::CubicBezierShape;

use super::elide::{fit, Elide};
use super::history::{can_stage, can_unstage};
use super::history_filter::commit_matches;
use super::*;
use crate::git::{RefKind, RefLabel};

/// Two text lines per commit.
const COMMIT_ROW: f32 = 38.0;
const LANE_WIDTH: f32 = 12.0;
const GRAPH_LEFT: f32 = 8.0;
const DOT_RADIUS: f32 = 4.0;
const TEXT_SIZE: f32 = 12.0;
const META_SIZE: f32 = 11.0;
const BADGE_PAD: f32 = 5.0;
const BADGE_HEIGHT: f32 = 16.0;
const BADGE_RADIUS: u8 = 3;
const BADGE_GAP: f32 = 4.0;
const BADGE_BAR: f32 = 3.0;
/// A longer branch name loses its middle, so one ref cannot push the subject out of the row.
const BADGE_TEXT_MAX: f32 = 160.0;
/// Badges never take more than this share of the first line; the subject keeps the rest.
const BADGE_SHARE: f32 = 0.5;
/// Space between neighbours on a line (subject, badges, count; author, date).
const GAP: f32 = 8.0;
/// Space between the row's content and its right edge (or the scroll bar).
const ROW_INSET: f32 = 8.0;

impl WorktreeBarApp {
    /// Rows of the commit list inside `rect`. Only visible rows are drawn.
    pub(super) fn commit_list(
        &self,
        ui: &mut Ui,
        p: &Palette,
        rect: Rect,
        data: &ViewData,
    ) -> Option<Action> {
        let rows = self.list_rows(data);
        let mut picked = None;
        let column = egui::Layout::top_down(egui::Align::Min);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect).layout(column), |ui| {
            ui.set_clip_rect(rect.intersect(ui.clip_rect()));
            ui.spacing_mut().item_spacing.y = 0.0;
            ScrollArea::vertical()
                .id_salt("commit-list")
                .auto_shrink([false, false])
                .show_rows(ui, COMMIT_ROW, rows.len(), |ui, range| {
                    for row in &rows[range] {
                        let action = match row {
                            Row::Working => self.working_row(ui, p, data),
                            Row::Commit(commit, graph) => {
                                self.commit_row(ui, p, commit, *graph, data.graph_width)
                            }
                            Row::SearchAll(query) => search_all_row(ui, p, query),
                            Row::Note(text) => note_row(ui, p, text),
                        };
                        picked = picked.take().or(action);
                    }
                });
        });
        picked
    }

    /// All commits with the graph; with a filter, the matching ones (no graph, since the lanes
    /// no longer join up) and a row to search all history; after that search, its results.
    fn list_rows<'a>(&'a self, data: &'a ViewData) -> Vec<Row<'a>> {
        let query = self.panes.history_query.trim();
        let commits = &data.view.commits;
        if query.is_empty() {
            let graph = data.graph.iter().map(Some).chain(std::iter::repeat(None));
            let listed = commits.iter().zip(graph).map(|(c, g)| Row::Commit(c, g));
            return std::iter::once(Row::Working).chain(listed).collect();
        }
        match self.search_hits() {
            Some(Ok(found)) if found.is_empty() => {
                vec![Row::Note("No commits match in all history".to_string())]
            }
            Some(Ok(found)) => found.iter().map(|c| Row::Commit(c, None)).collect(),
            Some(Err(error)) => vec![Row::Note(error.message.clone())],
            None => commits
                .iter()
                .filter(|c| commit_matches(c, query))
                .map(|c| Row::Commit(c, None))
                .chain(std::iter::once(Row::SearchAll(query.to_string())))
                .collect(),
        }
    }

    /// "No changes" or "N changes", with "Commit Changes" under it.
    fn working_row(&self, ui: &mut Ui, p: &Palette, data: &ViewData) -> Option<Action> {
        let selected = self.open_commit.is_none() && !self.has_location_pick();
        let (rect, response) = row_rect(ui, p, selected);
        let count = data.view.files.len();
        let title = working_title(&data.view.files);
        let dot = pos2(lane_x(rect, 0), rect.center().y);
        let square = Rect::from_center_size(dot, vec2(DOT_RADIUS * 2.0, DOT_RADIUS * 2.0));
        let color = if count == 0 { p.text_muted } else { p.accent };
        ui.painter().rect_filled(square, 1.5, color);
        let x = lane_x(rect, data.graph_width) + 2.0;
        let fg = if selected {
            p.row_selected_fg
        } else {
            p.text_strong
        };
        let right = row_right(ui, rect);
        line_text(ui, rect, (x, 0), &title, (text_font(), fg), right);
        let italic = italic_job("Commit Changes", p.text_muted);
        paint_job(ui, rect, x, 1, italic, right);
        response.clicked().then_some(Action::PickWorkingCopy)
    }

    fn commit_row(
        &self,
        ui: &mut Ui,
        p: &Palette,
        commit: &CommitLine,
        graph: Option<&GraphRow>,
        graph_width: usize,
    ) -> Option<Action> {
        let selected =
            self.open_commit.as_deref() == Some(commit.hash.as_str()) && !self.has_location_pick();
        let (rect, response) = row_rect(ui, p, selected);
        let x = match graph {
            Some(graph) => {
                paint_graph(ui, p, rect, graph, commit.parents.len() > 1);
                lane_x(rect, graph_width) + 2.0
            }
            None => rect.left() + 12.0,
        };
        let lane = graph.map_or(0, |g| g.lane);
        let right = row_right(ui, rect);
        let count_left = file_count(ui, p, rect, right, commit.files.len());
        let badges_right = if commit.files.is_empty() {
            right
        } else {
            count_left - GAP
        };
        let floor = badges_right - (badges_right - x).max(0.0) * BADGE_SHARE;
        let badges_left = ref_badges(ui, p, rect, (floor, badges_right), &commit.refs, lane);
        let subject_right = if commit.refs.is_empty() {
            badges_right
        } else {
            badges_left - GAP
        };
        let fg = subject_color(p, selected, commit);
        line_text(
            ui,
            rect,
            (x, 0),
            &commit.subject,
            (text_font(), fg),
            subject_right,
        );
        let date = commit.date_text(self.date_format);
        let date_left = date_text(ui, p, rect, right, &date);
        paint_job(
            ui,
            rect,
            x,
            1,
            italic_job(&commit.author, p.text_muted),
            date_left - GAP,
        );
        commit_clicks(response, self.selected.clone(), commit)
    }
}

/// One line of the commit list.
enum Row<'a> {
    Working,
    Commit(&'a CommitLine, Option<&'a GraphRow>),
    /// Offers `git log --grep` over all branches for the filter text.
    SearchAll(String),
    Note(String),
}

fn search_all_row(ui: &mut Ui, p: &Palette, query: &str) -> Option<Action> {
    let (rect, response) = row_rect(ui, p, false);
    let x = rect.left() + 12.0;
    let title = format!("Search all history for “{query}”");
    let right = row_right(ui, rect);
    line_text(ui, rect, (x, 0), &title, (text_font(), p.accent), right);
    let hint = italic_job(
        "Messages of every branch, not only the latest 200 commits",
        p.text_muted,
    );
    paint_job(ui, rect, x, 1, hint, right);
    response
        .clicked()
        .then(|| Action::SearchHistory(query.to_string()))
}

fn note_row(ui: &mut Ui, p: &Palette, text: &str) -> Option<Action> {
    let (rect, _) = row_rect(ui, p, false);
    let x = rect.left() + 12.0;
    line_text(
        ui,
        rect,
        (x, 0),
        text,
        (text_font(), p.text_muted),
        row_right(ui, rect),
    );
    None
}

/// The checked-out branch's tip stands out.
fn subject_color(p: &Palette, selected: bool, commit: &CommitLine) -> Color32 {
    let head = commit.refs.iter().any(|r| r.kind == RefKind::Head);
    match (selected, head) {
        (true, _) => p.row_selected_fg,
        (false, true) => p.text_strong,
        (false, false) => p.text,
    }
}

/// "No changes", "3 unstaged files", "1 staged, 2 unstaged files" —,
/// untracked files count as unstaged.
fn working_title(files: &[FileChange]) -> String {
    let staged = files.iter().filter(|f| can_unstage(&f.code)).count();
    let unstaged = files.iter().filter(|f| can_stage(&f.code)).count();
    let noun = |n: usize| if n == 1 { "file" } else { "files" };
    match (staged, unstaged) {
        (0, 0) => "No changes".to_string(),
        (0, u) => format!("{u} unstaged {}", noun(u)),
        (s, 0) => format!("{s} staged {}", noun(s)),
        (s, u) => format!("{s} staged, {u} unstaged {}", noun(u)),
    }
}

/// Hover shows the full hash and author; a click opens the commit, a right click its menu.
fn commit_clicks(
    response: egui::Response,
    worktree: Option<PathBuf>,
    commit: &CommitLine,
) -> Option<Action> {
    let hover = format!("{}\n{} <{}>", commit.full, commit.author, commit.email);
    let response = response.on_hover_text(hover);
    let mut menu = None;
    response.context_menu(|ui| menu = commit_menu(ui, worktree.clone(), commit));
    let clicked = response
        .clicked()
        .then(|| Action::PickCommit(commit.hash.clone()));
    menu.or(clicked)
}

/// Right click on a commit: copy it, or cherry-pick / revert it into the open worktree.
fn commit_menu(ui: &mut Ui, worktree: Option<PathBuf>, commit: &CommitLine) -> Option<Action> {
    let mut picked = None;
    let copies = [
        ("Copy Commit Hash", commit.full.clone()),
        ("Copy Short Hash", commit.hash.clone()),
        ("Copy Subject", commit.subject.clone()),
    ];
    for (label, text) in copies {
        if ui.button(label).clicked() {
            picked = Some(Action::Copy(text));
            ui.close();
        }
    }
    let Some(path) = worktree else {
        return picked;
    };
    ui.separator();
    let hash = commit.full.clone();
    let short = commit.hash.clone();
    if ui.button("Cherry-pick into this worktree").clicked() {
        picked = Some(Action::Ask(Confirm {
            title: "Cherry-pick this commit?",
            subject: short.clone(),
            note: Some("Applies the commit onto the branch checked out here."),
            button: "Cherry-pick",
            then: Box::new(Action::CherryPick {
                path: path.clone(),
                hash: hash.clone(),
            }),
        }));
        ui.close();
    }
    if ui.button("Revert this commit").clicked() {
        picked = Some(Action::Ask(Confirm {
            title: "Revert this commit?",
            subject: short,
            note: Some("Adds a new commit that undoes this one."),
            button: "Revert",
            then: Box::new(Action::RevertCommit { path, hash }),
        }));
        ui.close();
    }
    picked
}

fn text_font() -> FontId {
    FontId::proportional(TEXT_SIZE)
}

fn meta_font() -> FontId {
    FontId::proportional(META_SIZE)
}

/// Allocates one list row and paints its selection or hover background.
fn row_rect(ui: &mut Ui, p: &Palette, selected: bool) -> (Rect, egui::Response) {
    let size = vec2(ui.available_width(), COMMIT_ROW);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let fill = if selected {
        Some(p.row_selected_bg)
    } else if response.hovered() {
        Some(p.row_hover_bg)
    } else {
        None
    };
    if let Some(fill) = fill {
        ui.painter().rect_filled(rect, 0.0, fill);
    }
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(0.5, p.row_hover_bg),
    );
    (rect, response)
}

/// Right edge for row content. egui's scroll bar floats over the content instead of taking
/// width (`ScrollStyle::floating`, `floating_allocated_width` 0), so without this gap the date,
/// the badges and the file count end up under the bar.
fn row_right(ui: &Ui, rect: Rect) -> f32 {
    let scroll = &ui.spacing().scroll;
    let bar = if scroll.floating {
        scroll.bar_width + scroll.bar_outer_margin
    } else {
        0.0
    };
    rect.right() - ROW_INSET - bar
}

fn lane_x(rect: Rect, lane: usize) -> f32 {
    rect.left() + GRAPH_LEFT + lane as f32 * LANE_WIDTH + LANE_WIDTH / 2.0
}

/// Baseline-free placement: line 0 is the upper half of the row, line 1 the lower half.
fn line_y(rect: Rect, line: usize) -> f32 {
    rect.top() + if line == 0 { 12.0 } else { 27.0 }
}

/// `at` is (x, line); `style` is (font, colour); text is clipped at `right`.
fn line_text(
    ui: &Ui,
    rect: Rect,
    at: (f32, usize),
    text: &str,
    style: (FontId, Color32),
    right: f32,
) {
    let ((x, line), (font, color)) = (at, style);
    let clip = Rect::from_min_max(pos2(x, rect.top()), pos2(right.max(x), rect.bottom()));
    let shown = fit(ui, text, &font, (right - x).max(0.0), Elide::End);
    ui.painter()
        .with_clip_rect(clip.intersect(ui.clip_rect()))
        .text(
            pos2(x, line_y(rect, line)),
            Align2::LEFT_CENTER,
            shown,
            font,
            color,
        );
}

fn italic_job(text: &str, color: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let format = egui::text::TextFormat {
        font_id: meta_font(),
        color,
        italics: true,
        ..Default::default()
    };
    job.append(text, 0.0, format);
    job
}

fn paint_job(ui: &Ui, rect: Rect, x: f32, line: usize, job: egui::text::LayoutJob, right: f32) {
    let galley = ui.painter().layout_job(job);
    let at = pos2(x, line_y(rect, line) - galley.size().y / 2.0);
    let clip = Rect::from_min_max(pos2(x, rect.top()), pos2(right.max(x), rect.bottom()));
    ui.painter()
        .with_clip_rect(clip.intersect(ui.clip_rect()))
        .galley(at, galley, Color32::PLACEHOLDER);
}

/// The number of changed files in a small box at the right of the first line.
fn file_count(ui: &Ui, p: &Palette, rect: Rect, right: f32, count: usize) -> f32 {
    if count == 0 {
        return right;
    }
    let galley = ui
        .painter()
        .layout_no_wrap(count.to_string(), meta_font(), p.text_muted);
    let size = vec2(galley.size().x + 2.0 * BADGE_PAD, BADGE_HEIGHT);
    let badge = Rect::from_min_size(pos2(right - size.x, line_y(rect, 0) - size.y / 2.0), size);
    ui.painter().rect_filled(badge, 3.0, p.badge_bg);
    let at = badge.center() - galley.size() / 2.0;
    ui.painter().galley(at, galley, p.text_muted);
    badge.left()
}

fn date_text(ui: &Ui, p: &Palette, rect: Rect, right: f32, date: &str) -> f32 {
    let galley = ui
        .painter()
        .layout_no_wrap(date.to_string(), meta_font(), p.text_muted);
    let at = pos2(
        right - galley.size().x,
        line_y(rect, 1) - galley.size().y / 2.0,
    );
    let left = at.x;
    ui.painter().galley(at, galley, p.text_muted);
    left
}

/// One measured ref badge.
struct Badge {
    galley: std::sync::Arc<egui::Galley>,
    fill: Color32,
    fg: Color32,
    bar: Option<Color32>,
}

impl Badge {
    fn width(&self) -> f32 {
        let bar = if self.bar.is_some() { BADGE_BAR } else { 0.0 };
        self.galley.size().x + 2.0 * BADGE_PAD + bar
    }
}

fn measure_badge(ui: &Ui, text: String, colors: (Color32, Color32, Option<Color32>)) -> Badge {
    let (fill, fg, bar) = colors;
    let shown = fit(
        ui,
        &text,
        &meta_font(),
        BADGE_TEXT_MAX,
        Elide::for_name(&text),
    );
    let galley = ui.painter().layout_no_wrap(shown, meta_font(), fg);
    Badge {
        galley,
        fill,
        fg,
        bar,
    }
}

/// How many of the badges (in order) fit in `room`, leaving space for a "+N" badge when some
/// do not. `more(n)` is the width of the "+n" badge.
fn badges_that_fit(widths: &[f32], room: f32, more: impl Fn(usize) -> f32) -> usize {
    let row = |k: usize| widths[..k].iter().sum::<f32>() + BADGE_GAP * k.saturating_sub(1) as f32;
    if row(widths.len()) <= room {
        return widths.len();
    }
    (0..widths.len())
        .rev()
        .find(|&k| {
            let extra = more(widths.len() - k) + if k > 0 { BADGE_GAP } else { 0.0 };
            row(k) + extra <= room
        })
        .unwrap_or(0)
}

/// Branch and tag badges on the first line, right-aligned between `span.0` and `span.1`.
/// Refs that do not fit are counted in a trailing "+N" badge. Returns the left edge.
fn ref_badges(
    ui: &Ui,
    p: &Palette,
    rect: Rect,
    span: (f32, f32),
    refs: &[RefLabel],
    lane: usize,
) -> f32 {
    let (floor, right) = span;
    let badges: Vec<Badge> = refs
        .iter()
        .map(|label| measure_badge(ui, label.name.clone(), badge_colors(p, label.kind, lane)))
        .collect();
    let widths: Vec<f32> = badges.iter().map(Badge::width).collect();
    let more = |n: usize| measure_badge(ui, format!("+{n}"), more_colors(p));
    let shown = badges_that_fit(&widths, right - floor, |n| more(n).width());
    let hidden = badges.len() - shown;
    let mut row: Vec<Badge> = badges.into_iter().take(shown).collect();
    if hidden > 0 {
        row.push(more(hidden));
    }
    let total =
        row.iter().map(Badge::width).sum::<f32>() + BADGE_GAP * row.len().saturating_sub(1) as f32;
    let left = right - total;
    let mut x = left;
    for badge in row {
        x = paint_badge(ui, rect, x, badge) + BADGE_GAP;
    }
    left
}

/// Paints `badge` with its left edge at `x` on the first line; returns its right edge.
fn paint_badge(ui: &Ui, rect: Rect, x: f32, badge: Badge) -> f32 {
    let size = vec2(badge.width(), BADGE_HEIGHT);
    let rect = Rect::from_min_size(pos2(x, line_y(rect, 0) - size.y / 2.0), size);
    let painter = ui.painter();
    painter.rect_filled(rect, BADGE_RADIUS, badge.fill);
    let mut text_left = rect.left() + BADGE_PAD;
    if let Some(bar) = badge.bar {
        let strip = Rect::from_min_size(rect.min, vec2(BADGE_BAR, size.y));
        let left_side = egui::CornerRadius {
            nw: BADGE_RADIUS,
            sw: BADGE_RADIUS,
            ne: 0,
            se: 0,
        };
        painter.rect_filled(strip, left_side, bar);
        text_left += BADGE_BAR;
    }
    let at = pos2(text_left, rect.center().y - badge.galley.size().y / 2.0);
    painter.galley(at, badge.galley, badge.fg);
    rect.right()
}

/// The "+N" badge for refs that did not fit.
fn more_colors(p: &Palette) -> (Color32, Color32, Option<Color32>) {
    (p.badge_bg, p.text_muted, None)
}

/// (fill, text, side bar): the checked-out branch is solid, other local branches carry the
/// lane colour, remotes are plain, tags use the warning colour.
fn badge_colors(p: &Palette, kind: RefKind, lane: usize) -> (Color32, Color32, Option<Color32>) {
    match kind {
        RefKind::Head => (p.badge_head_bg, p.badge_head_fg, None),
        RefKind::Local => (p.badge_bg, p.text_strong, Some(lane_color(p, lane))),
        RefKind::Remote => (p.badge_bg, p.text_muted, None),
        RefKind::Tag => (p.badge_bg, p.dirty, None),
    }
}

fn lane_color(p: &Palette, lane: usize) -> Color32 {
    p.graph[lane % p.graph.len()]
}

/// Lines through, into and out of the row, then the commit's dot.
fn paint_graph(ui: &Ui, p: &Palette, rect: Rect, row: &GraphRow, merge: bool) {
    let painter = ui.painter();
    let (top, mid, bottom) = (rect.top(), rect.center().y, rect.bottom());
    let x = lane_x(rect, row.lane);
    for lane in &row.through {
        let stroke = Stroke::new(1.5, lane_color(p, *lane));
        painter.vline(lane_x(rect, *lane), top..=bottom, stroke);
    }
    for lane in &row.from_above {
        edge(
            painter,
            pos2(lane_x(rect, *lane), top),
            pos2(x, mid),
            lane_color(p, *lane),
        );
    }
    for lane in &row.to_below {
        edge(
            painter,
            pos2(x, mid),
            pos2(lane_x(rect, *lane), bottom),
            lane_color(p, *lane),
        );
    }
    let color = lane_color(p, row.lane);
    if merge {
        painter.circle(pos2(x, mid), DOT_RADIUS, p.bg, Stroke::new(2.0, color));
    } else {
        painter.circle_filled(pos2(x, mid), DOT_RADIUS, color);
    }
}

/// A straight line, or an S-curve when the lane changes.
fn edge(painter: &egui::Painter, from: Pos2, to: Pos2, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    if (from.x - to.x).abs() < 0.5 {
        painter.line_segment([from, to], stroke);
        return;
    }
    let bend = (to.y - from.y) / 2.0;
    let points = [
        from,
        pos2(from.x, from.y + bend),
        pos2(to.x, to.y - bend),
        to,
    ];
    painter.add(CubicBezierShape::from_points_stroke(
        points,
        false,
        Color32::TRANSPARENT,
        stroke,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(code: &str) -> FileChange {
        FileChange {
            code: code.to_string(),
            path: "f".to_string(),
            orig_path: None,
        }
    }

    #[test]
    fn badges_never_pass_their_room() {
        let more = |_: usize| 10.0;
        assert_eq!(badges_that_fit(&[30.0, 30.0], 64.0, more), 2);
        assert_eq!(badges_that_fit(&[30.0, 30.0], 63.0, more), 1);
        assert_eq!(badges_that_fit(&[30.0, 30.0, 30.0], 50.0, more), 1);
        assert_eq!(badges_that_fit(&[30.0], 20.0, more), 0);
        assert_eq!(badges_that_fit(&[], 0.0, more), 0);
    }

    #[test]
    fn working_title_counts_files() {
        assert_eq!(working_title(&[]), "No changes");
        assert_eq!(
            working_title(&[change("??"), change(" M")]),
            "2 unstaged files"
        );
        assert_eq!(working_title(&[change("A ")]), "1 staged file");
        assert_eq!(
            working_title(&[change("MM"), change("??")]),
            "1 staged, 2 unstaged files"
        );
    }
}
