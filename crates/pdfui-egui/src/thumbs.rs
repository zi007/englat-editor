//! The thumbnail strip: a narrow column of small pages beside the document. Clicking one goes to
//! that page. Pictures are drawn lazily (one per frame) into their own small cache.

use std::collections::HashSet;

use egui::{Align2, Color32, Rect, Sense, TextureHandle, TextureOptions, pos2, vec2};
use wordcraft_pdfdoc::{PageSize, PdfDoc};
use wordcraft_pdfview::render_page;

use crate::cache::Cache;

/// Width of a thumbnail, in screen points.
pub const THUMB_WIDTH: f32 = 112.0;
/// Space around and between thumbnails.
const PAD: f32 = 8.0;
/// Room under a thumbnail for its page number.
const LABEL: f32 = 18.0;
/// Pages above and below the window drawn ahead, in screen points.
const AHEAD: f32 = 100.0;

/// Thumbnail state.
#[derive(Default)]
pub struct Thumbs {
    pub open: bool,
    pictures: Cache<TextureHandle>,
    failed: HashSet<usize>,
}

/// Height of one thumbnail's picture for a page.
fn picture_height(size: PageSize) -> f32 {
    if size.width > 0.0 && size.height.is_finite() { (THUMB_WIDTH * size.height / size.width).clamp(8.0, THUMB_WIDTH * 20.0) } else { THUMB_WIDTH }
}

/// Top of each row and the total height.
fn rows(sizes: &[PageSize]) -> (Vec<f32>, f32) {
    let mut tops = Vec::with_capacity(sizes.len());
    let mut y = PAD;
    for s in sizes {
        tops.push(y);
        y += picture_height(*s) + LABEL + PAD;
    }
    (tops, y)
}

impl Thumbs {
    pub fn clear(&mut self) {
        self.pictures.clear();
        self.failed.clear();
    }

    pub fn held(&self) -> usize {
        self.pictures.len()
    }

    /// Draw the strip; returns the page clicked, if any.
    pub fn show(&mut self, ui: &mut egui::Ui, doc: &PdfDoc, sizes: &[PageSize], current: usize) -> Option<usize> {
        let (tops, total) = rows(sizes);
        let ctx = ui.ctx().clone();
        let ppp = ctx.pixels_per_point();
        let width_px = ((THUMB_WIDTH * ppp).round() as u32).clamp(16, 1024);
        let mut clicked = None;
        let mut more = false;
        egui::ScrollArea::vertical().id_salt("pdf_thumbs").auto_shrink([false, false]).show_viewport(ui, |ui, viewport| {
            let (rect, _) = ui.allocate_exact_size(vec2(THUMB_WIDTH + 2.0 * PAD, total), Sense::hover());
            let painter = ui.painter_at(ui.clip_rect());
            let first = tops.partition_point(|t| *t < viewport.min.y - AHEAD).saturating_sub(1);
            let mut budget = 1;
            for (index, top) in tops.iter().enumerate().skip(first) {
                if *top > viewport.max.y + AHEAD {
                    break;
                }
                let h = picture_height(sizes[index]);
                let page = Rect::from_min_size(rect.min + vec2(PAD, *top), vec2(THUMB_WIDTH, h));
                let cell = Rect::from_min_size(page.min, vec2(THUMB_WIDTH, h + LABEL));
                if current == index {
                    painter.rect_filled(cell.expand(4.0), 3.0, Color32::from_rgb(70, 110, 190));
                }
                painter.rect_filled(page, 0.0, Color32::WHITE);
                if self.pictures.exact(index, width_px).is_none() && !self.failed.contains(&index) && cell.intersects(ui.clip_rect()) {
                    if budget == 0 {
                        more = true;
                    } else {
                        budget -= 1;
                        match render_page(doc, index, width_px as f32) {
                            Ok(img) => {
                                let size = [img.width as usize, img.height as usize];
                                let image = egui::ColorImage::from_rgba_unmultiplied(size, &img.rgba);
                                let handle = ctx.load_texture(format!("pdf-thumb-{index}"), image, TextureOptions::LINEAR);
                                self.pictures.insert(index, width_px, handle, u64::from(img.width) * u64::from(img.height));
                            }
                            Err(e) => {
                                log::warn!("thumbnail {}: {e}", index + 1);
                                self.failed.insert(index);
                            }
                        }
                    }
                }
                if let Some(t) = self.pictures.exact(index, width_px) {
                    painter.image(t.id(), page, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                }
                painter.text(
                    pos2(page.center().x, page.max.y + LABEL / 2.0),
                    Align2::CENTER_CENTER,
                    (index + 1).to_string(),
                    egui::FontId::proportional(12.0),
                    Color32::WHITE,
                );
                let response = ui.interact(cell, ui.id().with(("thumb", index)), Sense::click());
                if response.clicked() {
                    clicked = Some(index);
                }
            }
        });
        if more {
            ctx.request_repaint();
        }
        clicked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_stack_without_overlap_and_survive_hostile_sizes() {
        let sizes = [
            PageSize { width: 612.0, height: 792.0 },
            PageSize { width: 0.0, height: 0.0 },
            PageSize { width: 1.0, height: 14400.0 },
            PageSize { width: f32::NAN, height: f32::INFINITY },
        ];
        let (tops, total) = rows(&sizes);
        assert_eq!(tops.len(), 4);
        assert!(tops.windows(2).all(|w| w[1] > w[0]));
        assert!(total.is_finite() && total > *tops.last().unwrap_or(&0.0));
        assert!(sizes.iter().all(|s| picture_height(*s).is_finite()));
    }
}
