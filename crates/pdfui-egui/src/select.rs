//! Selecting, copying and searching the text of the pages.
//!
//! The text of a page comes from `wordcraft_pdfview::text` and is kept for the pages near the
//! window ([`TextStore`]); a search reads the whole document a few pages per frame without
//! keeping the text ([`Search`]). Boxes are in PDF points from the page's top left, so they are
//! scaled like the page picture.

use std::collections::HashMap;
use std::sync::Arc;

use egui::{Color32, CursorIcon, Event, Painter, Pos2, Rect, Response, vec2};
use wordcraft_pdfdoc::PdfDoc;
use wordcraft_pdfview::text::{PageText, page_text};

use crate::layout::Layout;

/// Pages whose text is held; past this the store starts again (a window shows a few pages).
const MAX_CACHED_PAGES: usize = 48;
/// Pages a search reads in one frame.
const SCAN_PER_FRAME: usize = 3;
/// Most matches a search keeps.
pub const MAX_HITS: usize = 10_000;
/// Space left above a match when the window scrolls to it, in screen points.
pub const HIT_MARGIN: f32 = 80.0;

const SELECTION: Color32 = Color32::from_rgba_premultiplied(21, 42, 81, 90);
const HIT: Color32 = Color32::from_rgba_premultiplied(110, 90, 0, 110);
const CURRENT_HIT: Color32 = Color32::from_rgba_premultiplied(150, 70, 0, 150);

/// The text of the pages near the window.
#[derive(Default)]
pub struct TextStore {
    pages: HashMap<usize, Option<Arc<PageText>>>,
}

impl TextStore {
    pub fn clear(&mut self) {
        self.pages.clear();
    }

    /// The text of page `index`; `None` if it can't be read (remembered, so it isn't retried).
    pub fn get(&mut self, doc: &PdfDoc, index: usize) -> Option<Arc<PageText>> {
        if let Some(text) = self.pages.get(&index) {
            return text.clone();
        }
        if self.pages.len() >= MAX_CACHED_PAGES {
            self.pages.clear();
        }
        let text = page_text(doc, index).ok().map(Arc::new);
        self.pages.insert(index, text.clone());
        text
    }

    pub fn held(&self) -> usize {
        self.pages.len()
    }
}

/// A run of selected characters on one page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub page: usize,
    pub anchor: usize,
    pub head: usize,
}

/// One match of a search.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub page: usize,
    pub first: usize,
    pub last: usize,
    /// Top of the match, in points from the page's top.
    pub top: f32,
}

/// A search through the document, a few pages per frame.
#[derive(Default)]
pub struct Search {
    pub open: bool,
    pub query: String,
    /// The query the matches below are for.
    applied: String,
    hits: Vec<Hit>,
    scanned: usize,
    current: Option<usize>,
    /// The window should scroll to the current match.
    pub jump: bool,
    /// Give the box the keyboard focus on the next frame.
    pub focus: bool,
}

impl Search {
    /// Forget the matches (another document, or the same one asked again).
    pub fn reset(&mut self) {
        self.applied.clear();
        self.hits.clear();
        self.scanned = 0;
        self.current = None;
        self.jump = false;
    }

    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }

    /// Pages read so far.
    pub fn scanned(&self) -> usize {
        self.scanned
    }

    /// Number of the current match (from 0).
    pub fn current(&self) -> Option<usize> {
        self.current
    }

    /// The matches on page `index`.
    pub fn on_page(&self, index: usize) -> &[Hit] {
        let from = self.hits.partition_point(|h| h.page < index);
        let to = self.hits.partition_point(|h| h.page <= index);
        self.hits.get(from..to).unwrap_or(&[])
    }

    /// Read a few more pages if the query changed or the document isn't done; returns whether
    /// more is left to read.
    pub fn update(&mut self, doc: &PdfDoc) -> bool {
        if self.query != self.applied {
            self.reset();
            self.applied = self.query.clone();
            self.jump = true;
        }
        if self.applied.is_empty() {
            return false;
        }
        let pages = doc.page_count();
        for _ in 0..SCAN_PER_FRAME {
            if self.scanned >= pages || self.hits.len() >= MAX_HITS {
                break;
            }
            let index = self.scanned;
            self.scanned += 1;
            let Ok(text) = page_text(doc, index) else { continue };
            for (first, last) in text.find(&self.applied) {
                if self.hits.len() >= MAX_HITS {
                    break;
                }
                let top = text.chars.get(first).map_or(0.0, |c| c.rect[1]);
                self.hits.push(Hit { page: index, first, last, top });
            }
        }
        if self.current.is_none() && !self.hits.is_empty() {
            self.current = Some(0);
        }
        self.scanned < pages && self.hits.len() < MAX_HITS
    }

    /// Move to the next (or previous) match, wrapping round; the match to scroll to.
    pub fn step(&mut self, forward: bool) -> Option<Hit> {
        let n = self.hits.len();
        if n == 0 {
            return None;
        }
        let next = match self.current {
            None => 0,
            Some(c) if forward => (c + 1) % n,
            Some(c) => (c + n - 1) % n,
        };
        self.current = Some(next);
        self.hits.get(next).copied()
    }

    /// The current match, if any.
    pub fn current_hit(&self) -> Option<Hit> {
        self.current.and_then(|c| self.hits.get(c).copied())
    }
}

/// Everything about text in the window.
#[derive(Default)]
pub struct TextState {
    pub store: TextStore,
    pub selection: Option<Selection>,
    pub search: Search,
}

impl TextState {
    /// Start again for another document.
    pub fn clear(&mut self) {
        self.store.clear();
        self.selection = None;
        self.search.reset();
    }

    /// The selected text, if anything is selected.
    pub fn selected_text(&mut self, doc: &PdfDoc) -> Option<String> {
        let sel = self.selection?;
        let text = self.store.get(doc, sel.page)?;
        let s = text.text_between(sel.anchor, sel.head);
        (!s.is_empty()).then_some(s)
    }
}

fn to_screen(page: Rect, scale: f32, b: [f32; 4]) -> Rect {
    Rect::from_min_max(page.min + vec2(b[0], b[1]) * scale, page.min + vec2(b[2], b[3]) * scale)
}

/// Paint the selection and the search matches over page `index`, drawn in `page` on the screen.
pub fn paint_highlights(painter: &Painter, page: Rect, scale: f32, index: usize, text: &PageText, state: &TextState) {
    let current = state.search.current_hit();
    for hit in state.search.on_page(index) {
        let color = if current.is_some_and(|c| c.page == hit.page && c.first == hit.first) { CURRENT_HIT } else { HIT };
        for b in text.boxes_between(hit.first, hit.last) {
            painter.rect_filled(to_screen(page, scale, b), 1.0, color);
        }
    }
    if let Some(sel) = state.selection.filter(|s| s.page == index) {
        for b in text.boxes_between(sel.anchor, sel.head) {
            painter.rect_filled(to_screen(page, scale, b), 0.0, SELECTION);
        }
    }
}

/// The page under `pos` among the visible ones, with its screen rectangle.
fn page_under(pages: &[(usize, Rect)], pos: Pos2) -> Option<(usize, Rect)> {
    pages.iter().copied().find(|(_, r)| r.contains(pos))
}

/// Mouse selection over the column: press and drag across the text of a page; a click clears it.
/// `pages` are the pages on screen with their rectangles. Touch screens keep dragging for
/// scrolling, so nothing is selected by touch.
pub fn interact(ui: &egui::Ui, response: &Response, pages: &[(usize, Rect)], scale: f32, doc: &PdfDoc, state: &mut TextState) {
    let Some(pos) = response.interact_pointer_pos().or_else(|| response.hover_pos()) else { return };
    let over = page_under(pages, pos);
    if let Some((index, _)) = over
        && state.store.get(doc, index).is_some_and(|t| !t.chars.is_empty())
    {
        ui.ctx().set_cursor_icon(CursorIcon::Text);
    }
    let at = |state: &mut TextState, index: usize, page: Rect, pos: Pos2| -> Option<usize> {
        let text = state.store.get(doc, index)?;
        let local = (pos - page.min) / scale.max(1e-6);
        text.index_at(local.x, local.y)
    };
    if response.drag_started() {
        // A drag is only noticed after the pointer has moved a little, so the selection starts
        // where the button went down, not where the pointer is by now.
        let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(pos);
        state.selection = page_under(pages, origin).and_then(|(index, page)| {
            let anchor = at(state, index, page, origin)?;
            let head = at(state, index, page, pos).unwrap_or(anchor);
            Some(Selection { page: index, anchor, head })
        });
    } else if response.dragged()
        && let Some(sel) = state.selection
        && let Some(&(index, page)) = pages.iter().find(|(i, _)| *i == sel.page)
        && let Some(head) = at(state, index, page, pos)
    {
        state.selection = Some(Selection { head, ..sel });
    } else if response.clicked() {
        state.selection = None;
    }
}

/// Whether a copy was asked for this frame (Ctrl+C or the platform's copy).
pub fn copy_requested(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.events.iter().any(|e| matches!(e, Event::Copy)))
}

/// The page rectangles for the pages on screen, in the column whose top left is `column`.
pub fn visible_pages(lay: &Layout, column: Rect, from: f32, to: f32) -> Vec<(usize, Rect)> {
    lay.visible(from, to).filter_map(|i| lay.page_rect(i, column.width()).map(|r| (i, r.translate(column.min.to_vec2())))).collect()
}
