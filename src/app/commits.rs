//! The commit list: a working-copy row on top, then commits with the branch graph, ref badges,
//! author, date and file count.

use eframe::egui::epaint::CubicBezierShape;

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
const BADGE_PAD: f32 = 4.0;
const BADGE_HEIGHT: f32 = 15.0;

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
        line_text(
            ui,
            rect,
            (x, 0),
            &title,
            (text_font(), fg),
            rect.right() - 8.0,
        );
        let italic = italic_job("Commit Changes", p.text_muted);
        paint_job(ui, rect, x, 1, italic, rect.right() - 8.0);
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
        let right = rect.right() - 8.0;
        let count_left = file_count(ui, p, rect, right, commit.files.len());
        let fg = subject_color(p, selected, commit);
        line_text(
            ui,
            rect,
            (x, 0),
            &commit.subject,
            (text_font(), fg),
            count_left - 6.0,
        );
        let meta_left = if commit.refs.is_empty() {
            date_text(ui, p, rect, right, &commit.date)
        } else {
            ref_badges(ui, p, rect, right, &commit.refs, lane)
        };
        paint_job(
            ui,
            rect,
            x,
            1,
            italic_job(&commit.author, p.text_muted),
            meta_left - 6.0,
        );
        commit_clicks(response, commit)
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
    line_text(
        ui,
        rect,
        (x, 0),
        &title,
        (text_font(), p.accent),
        rect.right() - 8.0,
    );
    let hint = italic_job(
        "Messages of every branch, not only the latest 200 commits",
        p.text_muted,
    );
    paint_job(ui, rect, x, 1, hint, rect.right() - 8.0);
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
        rect.right() - 8.0,
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
fn commit_clicks(response: egui::Response, commit: &CommitLine) -> Option<Action> {
    let hover = format!("{}\n{} <{}>", commit.full, commit.author, commit.email);
    let response = response.on_hover_text(hover);
    let mut menu = None;
    response.context_menu(|ui| menu = commit_menu(ui, commit));
    let clicked = response
        .clicked()
        .then(|| Action::PickCommit(commit.hash.clone()));
    menu.or(clicked)
}

/// Right click on a commit: copy its hash or subject.
fn commit_menu(ui: &mut Ui, commit: &CommitLine) -> Option<Action> {
    let items = [
        ("Copy Commit Hash", commit.full.clone()),
        ("Copy Short Hash", commit.hash.clone()),
        ("Copy Subject", commit.subject.clone()),
    ];
    let mut picked = None;
    for (label, text) in items {
        if ui.button(label).clicked() {
            picked = Some(Action::Copy(text));
            ui.close();
        }
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
    ui.painter()
        .with_clip_rect(clip.intersect(ui.clip_rect()))
        .text(
            pos2(x, line_y(rect, line)),
            Align2::LEFT_CENTER,
            text,
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

/// Branch and tag badges, right-aligned on the second line. Returns their left edge.
fn ref_badges(ui: &Ui, p: &Palette, rect: Rect, right: f32, refs: &[RefLabel], lane: usize) -> f32 {
    let mut x = right;
    for label in refs.iter().rev() {
        let (fill, fg, bar) = badge_colors(p, label.kind, lane);
        let galley = ui
            .painter()
            .layout_no_wrap(label.name.clone(), meta_font(), fg);
        let bar_w = if bar.is_some() { 3.0 } else { 0.0 };
        let size = vec2(galley.size().x + 2.0 * BADGE_PAD + bar_w, BADGE_HEIGHT);
        let badge = Rect::from_min_size(pos2(x - size.x, line_y(rect, 1) - size.y / 2.0), size);
        if badge.left() < rect.center().x {
            break;
        }
        ui.painter().rect_filled(badge, 3.0, fill);
        if let Some(bar) = bar {
            let strip = Rect::from_min_size(badge.min, vec2(bar_w, size.y));
            ui.painter().rect_filled(strip, 1.0, bar);
        }
        let at = pos2(
            badge.left() + bar_w + BADGE_PAD,
            badge.center().y - galley.size().y / 2.0,
        );
        ui.painter().galley(at, galley, fg);
        x = badge.left() - 4.0;
    }
    x
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
