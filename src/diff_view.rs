//! Draws a parsed diff: line numbers, the `+`/`-` column and syntax colours.
//! Only rows inside the visible clip rectangle are laid out and painted.

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{
    pos2, vec2, Align2, Color32, FontId, Rect, RichText, ScrollArea, Sense, Stroke, TextStyle, Ui,
};

use crate::diff::{DiffRow, DiffView, RowKind};
use crate::highlight::TokenKind;
use crate::theme::Palette;

/// Vertical padding added to the monospace row height.
const ROW_PAD: f32 = 4.0;
/// Space on each side of a line-number column.
const GUTTER_PAD: f32 = 6.0;
/// Room after the longest line so it does not touch the edge.
const TRAILING_SPACE: f32 = 24.0;
const MIN_DIGITS: usize = 2;

/// Column positions of one diff, measured once per frame.
struct Columns {
    font: FontId,
    row_h: f32,
    old_right: f32,
    new_right: f32,
    sign_x: f32,
    code_x: f32,
    width: f32,
}

impl Columns {
    fn measure(ui: &Ui, view: &DiffView) -> Columns {
        let font = TextStyle::Monospace.resolve(ui.style());
        let (line_h, char_w) = ui
            .ctx()
            .fonts_mut(|fonts| (fonts.row_height(&font), fonts.glyph_width(&font, '0')));
        let digits = view.max_line_no.to_string().len().max(MIN_DIGITS);
        let number_w = digits as f32 * char_w + 2.0 * GUTTER_PAD;
        let sign_x = 2.0 * number_w + GUTTER_PAD;
        let code_x = sign_x + 2.0 * char_w;
        Columns {
            font,
            row_h: line_h + ROW_PAD,
            old_right: number_w - GUTTER_PAD,
            new_right: 2.0 * number_w - GUTTER_PAD,
            sign_x,
            code_x,
            width: code_x + view.max_chars as f32 * char_w + TRAILING_SPACE,
        }
    }
}

/// Draws `view` inside the current (vertically scrolling) area, with its own horizontal scroll.
/// `salt` keeps the scroll state of each diff apart when one pane shows several.
pub fn show(ui: &mut Ui, p: &Palette, view: &DiffView, salt: &str) {
    if view.rows.is_empty() {
        ui.label(RichText::new("No textual changes").color(p.text_muted));
        return;
    }
    let columns = Columns::measure(ui, view);
    ScrollArea::horizontal()
        .id_salt(("diff-horizontal", salt))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let width = columns.width.max(ui.available_width());
            let height = columns.row_h * view.rows.len() as f32;
            let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
            for index in visible_rows(rect, ui.clip_rect(), columns.row_h, view.rows.len()) {
                let top = rect.top() + index as f32 * columns.row_h;
                let row_rect =
                    Rect::from_min_size(pos2(rect.left(), top), vec2(width, columns.row_h));
                paint_row(ui, p, &columns, row_rect, &view.rows[index]);
            }
        });
}

/// Indices of rows that intersect `clip`.
fn visible_rows(rect: Rect, clip: Rect, row_h: f32, count: usize) -> std::ops::Range<usize> {
    if row_h <= 0.0 || count == 0 {
        return 0..0;
    }
    let first = ((clip.top() - rect.top()) / row_h).floor().max(0.0) as usize;
    let last = ((clip.bottom() - rect.top()) / row_h).ceil().max(0.0) as usize;
    first.min(count)..last.min(count)
}

fn paint_row(ui: &Ui, p: &Palette, columns: &Columns, rect: Rect, row: &DiffRow) {
    let painter = ui.painter();
    if let Some(fill) = row_fill(p, row.kind) {
        painter.rect_filled(rect, 0.0, fill);
    }
    let y = rect.center().y;
    match row.kind {
        RowKind::File => {
            painter.hline(rect.x_range(), rect.top(), Stroke::new(1.0, p.border));
            let at = pos2(rect.left() + GUTTER_PAD, y);
            painter.text(
                at,
                Align2::LEFT_CENTER,
                &row.text,
                columns.font.clone(),
                p.diff_file_fg,
            );
        }
        RowKind::Hunk => {
            let at = pos2(rect.left() + GUTTER_PAD, y);
            painter.text(
                at,
                Align2::LEFT_CENTER,
                &row.text,
                columns.font.clone(),
                p.diff_hunk_fg,
            );
        }
        _ => paint_code_row(ui, p, columns, rect, row),
    }
}

fn paint_code_row(ui: &Ui, p: &Palette, columns: &Columns, rect: Rect, row: &DiffRow) {
    let painter = ui.painter();
    let y = rect.center().y;
    let left = rect.left();
    for (number, right) in [
        (row.old_no, columns.old_right),
        (row.new_no, columns.new_right),
    ] {
        if let Some(number) = number {
            let at = pos2(left + right, y);
            let font = columns.font.clone();
            painter.text(
                at,
                Align2::RIGHT_CENTER,
                number.to_string(),
                font,
                p.text_muted,
            );
        }
    }
    if let Some((sign, color)) = sign(p, row.kind) {
        let at = pos2(left + columns.sign_x, y);
        painter.text(at, Align2::LEFT_CENTER, sign, columns.font.clone(), color);
    }
    let galley = painter.layout_job(code_job(p, &columns.font, row));
    let at = pos2(left + columns.code_x, y - galley.size().y / 2.0);
    painter.galley(at, galley, p.text);
}

fn row_fill(p: &Palette, kind: RowKind) -> Option<Color32> {
    match kind {
        RowKind::File => Some(p.diff_file_bg),
        RowKind::Hunk => Some(p.diff_hunk_bg),
        RowKind::Added => Some(p.diff_add_bg),
        RowKind::Removed => Some(p.diff_remove_bg),
        RowKind::Context | RowKind::Note => None,
    }
}

fn sign(p: &Palette, kind: RowKind) -> Option<(&'static str, Color32)> {
    match kind {
        RowKind::Added => Some(("+", p.diff_add_fg)),
        RowKind::Removed => Some(("-", p.diff_remove_fg)),
        _ => None,
    }
}

/// One text job per row: a section per token, or one plain section for notes and plain files.
/// Upright text only: comments and notes differ by colour, not by slant.
fn code_job(p: &Palette, font: &FontId, row: &DiffRow) -> LayoutJob {
    let format = |color: Color32| TextFormat {
        font_id: font.clone(),
        color,
        ..TextFormat::default()
    };
    let mut job = LayoutJob::default();
    if row.kind == RowKind::Note {
        job.append(&row.text, 0.0, format(p.text_muted));
        return job;
    }
    if row.spans.is_empty() {
        job.append(&row.text, 0.0, format(p.text));
        return job;
    }
    for span in &row.spans {
        let Some(text) = row.text.get(span.range.clone()) else {
            continue;
        };
        job.append(text, 0.0, format(token_color(p, span.kind)));
    }
    job
}

pub fn token_color(p: &Palette, kind: TokenKind) -> Color32 {
    match kind {
        TokenKind::Plain => p.text,
        TokenKind::Keyword => p.syntax_keyword,
        TokenKind::Type => p.syntax_type,
        TokenKind::String => p.syntax_string,
        TokenKind::Number => p.syntax_number,
        TokenKind::Comment => p.syntax_comment,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::parse_diff;
    use crate::theme::DARK;

    #[test]
    fn only_rows_in_the_clip_are_visible() {
        let rect = Rect::from_min_size(pos2(0.0, 100.0), vec2(500.0, 1000.0));
        let clip = Rect::from_min_max(pos2(0.0, 150.0), pos2(500.0, 250.0));
        assert_eq!(visible_rows(rect, clip, 10.0, 100), 5..15);
        let below = Rect::from_min_max(pos2(0.0, 2000.0), pos2(500.0, 2100.0));
        assert_eq!(visible_rows(rect, below, 10.0, 100), 100..100);
        assert_eq!(visible_rows(rect, clip, 0.0, 100), 0..0);
    }

    #[test]
    fn code_job_uses_token_colours() {
        let view = parse_diff(
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n+fn x() {} // c\n",
        );
        let row = &view.rows[2];
        let job = code_job(&DARK, &FontId::monospace(12.0), row);
        assert_eq!(job.text, row.text);
        let colours: Vec<Color32> = job.sections.iter().map(|s| s.format.color).collect();
        assert!(colours.contains(&DARK.syntax_keyword));
        assert!(colours.contains(&DARK.syntax_comment));
        assert!(
            job.sections.iter().all(|s| !s.format.italics),
            "no italic text"
        );
    }
}
