//! Where the pages are: a column of pages, centred, at a zoom. Pure arithmetic on page sizes, so the
//! window only draws what it says is visible.

use std::ops::Range;

use egui::{Rect, Vec2, pos2, vec2};
use wordcraft_pdfdoc::PageSize;

/// Screen points per PDF point at 100%: a PDF point is 1/72 inch and 100% shows 96 to the inch.
pub const POINT_TO_UI: f32 = 96.0 / 72.0;
pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 8.0;
/// Space around the column of pages, and between two pages.
pub const MARGIN: f32 = 16.0;
pub const GAP: f32 = 12.0;

/// How big the pages are shown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Zoom {
    /// The widest page as wide as the window.
    FitWidth,
    /// A factor: 1.0 is 100%.
    Factor(f32),
}

/// The pages laid out for one zoom and window width.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// Screen points per PDF point.
    pub scale: f32,
    /// The size of the whole column, margins included.
    pub content: Vec2,
    sizes: Vec<Vec2>,
    tops: Vec<f32>,
    bottoms: Vec<f32>,
}

/// Lay out `sizes` at `zoom` for a window `available_width` wide.
pub fn layout(sizes: &[PageSize], zoom: Zoom, available_width: f32) -> Layout {
    let widest = sizes.iter().map(|s| s.width).fold(1.0_f32, f32::max);
    let factor = match zoom {
        Zoom::Factor(f) if f.is_finite() => f,
        Zoom::FitWidth if available_width.is_finite() && available_width > 2.0 * MARGIN => (available_width - 2.0 * MARGIN) / (widest * POINT_TO_UI),
        _ => 1.0,
    };
    let scale = factor.clamp(MIN_ZOOM, MAX_ZOOM) * POINT_TO_UI;
    let sizes: Vec<Vec2> = sizes.iter().map(|s| vec2(s.width * scale, s.height * scale)).collect();
    let mut tops = Vec::with_capacity(sizes.len());
    let mut bottoms = Vec::with_capacity(sizes.len());
    let mut y = MARGIN;
    for s in &sizes {
        tops.push(y);
        y += s.y;
        bottoms.push(y);
        y += GAP;
    }
    let width = sizes.iter().map(|s| s.x).fold(0.0_f32, f32::max);
    let content = if sizes.is_empty() { Vec2::ZERO } else { vec2(width + 2.0 * MARGIN, y - GAP + MARGIN) };
    Layout { scale, content, sizes, tops, bottoms }
}

impl Layout {
    /// The zoom as a factor (1.0 is 100%).
    pub fn factor(&self) -> f32 {
        self.scale / POINT_TO_UI
    }

    /// Where page `index` starts, from the top of the column.
    pub fn top(&self, index: usize) -> Option<f32> {
        self.tops.get(index).copied()
    }

    /// The pages that show between `from` and `to` (positions in the column).
    pub fn visible(&self, from: f32, to: f32) -> Range<usize> {
        let first = self.bottoms.partition_point(|b| *b < from);
        let last = self.tops.partition_point(|t| *t <= to);
        first..last.max(first)
    }

    /// The page the reader is on for a window showing `view_top` to `view_top + view_height`: the
    /// one at a third of the way down, so a page counts once it fills the upper part of the window.
    pub fn page_at(&self, view_top: f32, view_height: f32) -> usize {
        let y = view_top + view_height.max(0.0) / 3.0;
        self.tops.partition_point(|t| *t <= y).saturating_sub(1).min(self.sizes.len().saturating_sub(1))
    }

    /// The rectangle of page `index` in a column drawn `column_width` wide, from the column's
    /// top left corner. Pages are centred.
    pub fn page_rect(&self, index: usize, column_width: f32) -> Option<Rect> {
        let size = self.sizes.get(index).copied()?;
        let top = self.tops.get(index).copied()?;
        let x = ((column_width - size.x) / 2.0).max(MARGIN);
        Some(Rect::from_min_size(pos2(x, top), size))
    }
}

/// The zoom steps the buttons move between, as factors.
pub const STEPS: &[f32] = &[0.1, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0];

/// The next step above (`up`) or below `factor`.
pub fn step(factor: f32, up: bool) -> f32 {
    let eps = 1e-3;
    let next = if up { STEPS.iter().copied().find(|s| *s > factor + eps) } else { STEPS.iter().rev().copied().find(|s| *s < factor - eps) };
    next.unwrap_or(factor.clamp(MIN_ZOOM, MAX_ZOOM))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn letter(n: usize) -> Vec<PageSize> {
        vec![PageSize { width: 612.0, height: 792.0 }; n]
    }

    #[test]
    fn pages_are_stacked_with_gaps_and_margins() {
        let l = layout(&letter(3), Zoom::Factor(1.0), 1000.0);
        let (w, h) = (612.0 * POINT_TO_UI, 792.0 * POINT_TO_UI);
        assert_eq!(l.sizes.len(), 3);
        assert_eq!(l.top(0), Some(MARGIN));
        assert_eq!(l.top(1), Some(MARGIN + h + GAP));
        assert_eq!(l.top(3), None);
        assert!((l.content.x - (w + 2.0 * MARGIN)).abs() < 1e-3);
        assert!((l.content.y - (3.0 * h + 2.0 * GAP + 2.0 * MARGIN)).abs() < 1e-2);
    }

    #[test]
    fn fit_width_fills_the_window_and_zoom_is_held_to_its_limits() {
        let l = layout(&letter(1), Zoom::FitWidth, 1000.0);
        assert!((612.0 * l.scale - (1000.0 - 2.0 * MARGIN)).abs() < 1e-2);
        assert!(layout(&letter(1), Zoom::Factor(1000.0), 800.0).factor() <= MAX_ZOOM + 1e-4);
        assert!(layout(&letter(1), Zoom::Factor(0.0), 800.0).factor() >= MIN_ZOOM - 1e-4);
        // Not numbers: 100%.
        for z in [Zoom::Factor(f32::NAN), Zoom::Factor(f32::INFINITY)] {
            assert!((layout(&letter(1), z, 800.0).factor() - 1.0).abs() < 1e-4, "{z:?}");
        }
        for w in [0.0, -5.0, f32::NAN, 10.0] {
            assert!((layout(&letter(1), Zoom::FitWidth, w).factor() - 1.0).abs() < 1e-4, "{w}");
        }
    }

    #[test]
    fn the_visible_pages_are_the_ones_the_window_shows() {
        let l = layout(&letter(10), Zoom::Factor(1.0), 1000.0);
        let (h, step) = (792.0 * POINT_TO_UI, 792.0 * POINT_TO_UI + GAP);
        assert_eq!(l.visible(0.0, 100.0), 0..1);
        assert_eq!(l.visible(MARGIN + h - 1.0, MARGIN + h + 1.0), 0..1, "the gap is no page");
        assert_eq!(l.visible(MARGIN + h + GAP, MARGIN + h + GAP + 1.0), 1..2);
        // From just below page 1 to the top of page 4: pages 2 and 3.
        assert_eq!(l.visible(2.0 * step + 20.0, 4.0 * step), 2..4);
        assert_eq!(l.visible(1.0e9, 2.0e9), 10..10);
        assert_eq!(l.visible(-100.0, -50.0), 0..0);
        assert_eq!(layout(&[], Zoom::FitWidth, 800.0).visible(0.0, 100.0), 0..0);
    }

    #[test]
    fn the_current_page_is_the_one_in_the_upper_part_of_the_window() {
        let l = layout(&letter(5), Zoom::Factor(1.0), 1000.0);
        let step = 792.0 * POINT_TO_UI + GAP;
        assert_eq!(l.page_at(0.0, 600.0), 0);
        assert_eq!(l.page_at(step - 0.4 * 600.0, 600.0), 0, "a page counts when it fills the upper third");
        assert_eq!(l.page_at(step - 0.2 * 600.0, 600.0), 1);
        assert_eq!(l.page_at(1.0e9, 600.0), 4, "past the end: the last page");
        assert_eq!(layout(&[], Zoom::FitWidth, 800.0).page_at(0.0, 600.0), 0);
    }

    #[test]
    fn pages_are_centred_in_a_wider_column_and_kept_inside_a_narrower_one() {
        let l = layout(&letter(1), Zoom::Factor(1.0), 1000.0);
        let w = 612.0 * POINT_TO_UI;
        let wide = l.page_rect(0, 2000.0).unwrap();
        assert!((wide.center().x - 1000.0).abs() < 1e-2);
        let narrow = l.page_rect(0, 100.0).unwrap();
        assert_eq!(narrow.min.x, MARGIN);
        assert!((narrow.width() - w).abs() < 1e-3);
        assert_eq!(l.page_rect(1, 1000.0), None);
    }

    #[test]
    fn zoom_steps_go_up_and_down_and_stop_at_the_ends() {
        assert_eq!(step(1.0, true), 1.25);
        assert_eq!(step(1.0, false), 0.75);
        assert_eq!(step(0.9, true), 1.0, "from between steps to the next");
        assert_eq!(step(0.9, false), 0.75);
        assert_eq!(step(MAX_ZOOM, true), MAX_ZOOM);
        assert_eq!(step(MIN_ZOOM, false), MIN_ZOOM);
    }

    #[test]
    fn many_pages_lay_out_at_once() {
        let l = layout(&letter(100_000), Zoom::FitWidth, 1200.0);
        assert_eq!(l.sizes.len(), 100_000);
        assert!(l.content.y.is_finite());
        assert_eq!(l.visible(l.content.y - 10.0, l.content.y), 99_999..100_000);
    }
}
