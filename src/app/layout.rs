//! Pane geometry shared by the main area: header strips, scrolling columns and the splitter.

use super::*;

/// Height of the strip with pane titles ("Commits", "Summary").
pub(super) const PANE_HEADER: f32 = 26.0;
/// Share of the width the commit list takes before the user drags the splitter.
const HISTORY_SHARE: f32 = 0.34;
const HISTORY_MIN_WIDTH: f32 = 200.0;
/// The right pane keeps at least this much room.
const DETAIL_MIN_WIDTH: f32 = 320.0;
pub(super) const SPLITTER_WIDTH: f32 = 5.0;

/// Width of the commit list inside `full`, kept between its minimum and the detail minimum.
pub(super) fn history_width(wanted: Option<f32>, full: f32) -> f32 {
    let wanted = wanted.unwrap_or(full * HISTORY_SHARE);
    let max = (full - SPLITTER_WIDTH - DETAIL_MIN_WIDTH).max(HISTORY_MIN_WIDTH);
    wanted.clamp(HISTORY_MIN_WIDTH, max).min(full * 0.9)
}

/// A strip of pane titles; the active one looks like a raised tab.
/// `tab` is (width of each title, fill of the active one: the pane's own background).
pub(super) fn pane_header(
    ui: &Ui,
    p: &Palette,
    rect: Rect,
    titles: &[(&str, bool)],
    tab: (f32, Color32),
) {
    let (tab_width, active_fill) = tab;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, p.pane_header_bg);
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, p.border),
    );
    let width = tab_width.min(rect.width());
    for (i, (title, active)) in titles.iter().enumerate() {
        let tab = Rect::from_min_size(
            pos2(rect.left() + i as f32 * width, rect.top()),
            vec2(width, rect.height()),
        );
        if *active {
            painter.rect_filled(tab, 0.0, active_fill);
        }
        let color = if *active { p.text_strong } else { p.text_muted };
        let font = FontId::proportional(12.0);
        painter.text(tab.center(), Align2::CENTER_CENTER, title, font, color);
    }
}

/// Three gray squares that pulse in turn while loading.
pub(super) fn loading_dots(ui: &mut Ui, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    let time = ui.input(|i| i.time) as f32;
    for i in 0..3 {
        let phase = ((time * 3.0 - i as f32 * 0.6).sin() + 1.0) / 2.0;
        let center = pos2(rect.center().x + (i as f32 - 1.0) * 9.0, rect.center().y);
        let color = p.text_muted.gamma_multiply(0.35 + 0.65 * phase);
        ui.painter()
            .rect_filled(Rect::from_center_size(center, vec2(6.0, 6.0)), 1.0, color);
    }
    ui.ctx().request_repaint();
}

/// The search icon of Locations and the commit list.
pub(super) fn paint_magnifier(painter: &egui::Painter, center: Pos2, color: Color32) {
    let stroke = Stroke::new(1.3, color);
    painter.circle_stroke(center - vec2(1.5, 1.5), 4.0, stroke);
    let handle = center + vec2(1.5, 1.5);
    painter.line_segment([handle, handle + vec2(3.5, 3.5)], stroke);
}

/// A clipped, vertically scrolling column at `rect` with an inner margin.
pub(super) fn in_column<R>(
    ui: &mut Ui,
    rect: Rect,
    salt: &str,
    margin: f32,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let column = egui::Layout::top_down(egui::Align::Min);
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect).layout(column), |ui| {
        ui.set_clip_rect(rect.intersect(ui.clip_rect()));
        ScrollArea::vertical()
            .id_salt(salt)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Frame::new()
                    .inner_margin(Margin::same(margin as i8))
                    .show(ui, add)
                    .inner
            })
            .inner
    })
    .inner
}

/// Paints the divider; returns the new left width while it is dragged.
pub(super) fn splitter(ui: &mut Ui, p: &Palette, rect: Rect, width: f32) -> Option<f32> {
    let response = ui
        .interact(rect, Id::new("history-splitter"), Sense::drag())
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    let active = response.hovered() || response.dragged();
    let color = if active { p.accent } else { p.border };
    ui.painter()
        .vline(rect.center().x, rect.y_range(), Stroke::new(1.0, color));
    response.dragged().then(|| width + response.drag_delta().x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_width_stays_in_bounds() {
        assert_eq!(history_width(Some(300.0), 1000.0), 300.0);
        assert_eq!(history_width(Some(10.0), 1000.0), HISTORY_MIN_WIDTH);
        let widest = 1000.0 - SPLITTER_WIDTH - DETAIL_MIN_WIDTH;
        assert_eq!(history_width(Some(5000.0), 1000.0), widest);
        assert_eq!(history_width(None, 1000.0), 340.0);
        // A narrow window still leaves room for the detail pane.
        assert!(history_width(Some(5000.0), 300.0) < 300.0);
    }
}
