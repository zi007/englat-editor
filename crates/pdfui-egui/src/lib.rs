//! The PDF viewer's front end: a toolbar, the pages in a scrolling column, zoom, and a password
//! prompt. [`PdfApp`] is what the web shell (and tests) drive; it opens files from bytes, so it does
//! not care where they come from (a file picker, a drop, the sample).
//!
//! Pages are drawn when they scroll into view, a couple per frame, and pictures are kept in a
//! small cache (see [`cache`]); a long document never holds more than a fixed amount of memory.
//! Drawing a page happens on the interface thread for now, so a very heavy page can make a frame
//! slow; moving it off the thread is later work (`docs/pdf-editor.md`).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod cache;
mod layout;
mod select;
mod thumbs;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use cache::Cache;
use egui::{Align2, Color32, Id, Key, Modifiers, Rect, Sense, TextureHandle, TextureOptions, pos2, vec2};
pub use layout::Zoom;
use layout::{GAP, Layout, layout, step};
use select::{Hit, TextState};
use thumbs::Thumbs;
use wordcraft_pdfdoc::outline::OutlineEntry;
use wordcraft_pdfdoc::{OpenError, PageSize, PdfDoc, sample};
use wordcraft_pdfview::render_page;

/// Files that arrive from outside the frame (a picker or a drop read in the background): name and
/// bytes. The shell fills it; the app opens the last one.
pub type Inbox = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// What the platform provides.
#[derive(Default)]
pub struct Services {
    /// Ask the platform for a file; it arrives later in `inbox`. `None`: no Open button.
    pub open_async: Option<Box<dyn Fn()>>,
    pub inbox: Option<Inbox>,
}

/// Most pages drawn in one frame.
const RENDERS_PER_FRAME: usize = 2;
/// Pages above and below the window drawn ahead, in screen points.
const PREFETCH: f32 = 200.0;
const BACKDROP: Color32 = Color32::from_gray(0x4a);
const SAMPLE_PAGES: usize = 8;
const SIDEBAR_WIDTH: f32 = 200.0;

struct Loaded {
    doc: PdfDoc,
    name: String,
    sizes: Vec<PageSize>,
    outline: Vec<OutlineEntry>,
}

struct PasswordPrompt {
    name: String,
    bytes: Arc<Vec<u8>>,
    input: String,
    wrong: bool,
    focus: bool,
}

/// The viewer.
pub struct PdfApp {
    pub services: Services,
    loaded: Option<Loaded>,
    zoom: Zoom,
    /// The layout drawn last frame.
    layout: Option<Layout>,
    /// A page to scroll to on the next frame.
    scroll_to: Option<usize>,
    current: usize,
    last_offset: f32,
    last_scale: f32,
    pictures: Cache<TextureHandle>,
    failed: HashSet<usize>,
    message: Option<String>,
    password: Option<PasswordPrompt>,
    thumbs: Thumbs,
    /// The sidebar shows the table of contents instead of thumbnails.
    contents: bool,
    /// Selection, search matches and the text of the pages near the window.
    text: TextState,
    /// A search match to scroll to on the next frame.
    scroll_hit: Option<Hit>,
    /// Where the column of pages was drawn last frame, on the screen.
    column: Rect,
}

impl PdfApp {
    pub fn new(services: Services) -> Self {
        Self {
            services,
            loaded: None,
            zoom: Zoom::FitWidth,
            layout: None,
            scroll_to: None,
            current: 0,
            last_offset: 0.0,
            last_scale: 0.0,
            pictures: Cache::default(),
            failed: HashSet::new(),
            message: None,
            password: None,
            thumbs: Thumbs::default(),
            contents: false,
            text: TextState::default(),
            scroll_hit: None,
            column: Rect::NOTHING,
        }
    }

    /// Open a PDF. A damaged or unsupported file leaves what was open as it was and shows why; an
    /// encrypted one asks for its password.
    pub fn open_bytes(&mut self, name: &str, bytes: Vec<u8>) {
        self.try_open(name, Arc::new(bytes), None);
    }

    fn try_open(&mut self, name: &str, bytes: Arc<Vec<u8>>, password: Option<&str>) {
        match PdfDoc::open_shared(bytes.clone(), password) {
            Ok(doc) if doc.page_count() == 0 => {
                self.password = None;
                self.message = Some(format!("{name} has no pages."));
            }
            Ok(doc) => {
                let sizes = (0..doc.page_count()).map(|i| doc.page_size(i).unwrap_or(PageSize { width: 612.0, height: 792.0 })).collect();
                self.loaded = Some(Loaded { outline: doc.outline(), doc, name: name.to_string(), sizes });
                self.layout = None;
                self.scroll_to = Some(0);
                self.current = 0;
                self.last_offset = 0.0;
                self.last_scale = 0.0;
                self.pictures.clear();
                self.failed.clear();
                self.thumbs.clear();
                self.text.clear();
                self.scroll_hit = None;
                self.message = None;
                self.password = None;
            }
            Err(e @ (OpenError::PasswordRequired | OpenError::WrongPassword)) => {
                self.password =
                    Some(PasswordPrompt { name: name.to_string(), bytes, input: String::new(), wrong: e == OpenError::WrongPassword, focus: true });
            }
            Err(e) => {
                self.password = None;
                self.message = Some(format!("{name}: {e}"));
            }
        }
    }

    /// Try a password for the file that asked for one.
    pub fn submit_password(&mut self, password: &str) {
        if let Some(prompt) = self.password.take() {
            self.try_open(&prompt.name, prompt.bytes, Some(password));
        }
    }

    /// Give up on the file that asked for a password.
    pub fn cancel_password(&mut self) {
        self.password = None;
    }

    pub fn needs_password(&self) -> bool {
        self.password.is_some()
    }

    /// Why the last file did not open, if it didn't.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn page_count(&self) -> usize {
        self.loaded.as_ref().map_or(0, |l| l.sizes.len())
    }

    /// The page the reader is on (from 0).
    pub fn current_page(&self) -> usize {
        self.current
    }

    /// Scroll to a page (from 0) on the next frame.
    pub fn go_to_page(&mut self, page: usize) {
        if let Some(last) = self.page_count().checked_sub(1) {
            self.scroll_to = Some(page.min(last));
        }
    }

    /// The selected text, if anything is selected.
    pub fn selected_text(&mut self) -> Option<String> {
        let doc = &self.loaded.as_ref()?.doc;
        self.text.selected_text(doc)
    }

    /// Open or close the find bar.
    pub fn set_find_open(&mut self, open: bool) {
        self.text.search.open = open;
        self.text.search.focus = open;
    }

    pub fn find_open(&self) -> bool {
        self.text.search.open
    }

    /// Search the document for `query` (read a few pages per frame, so matches arrive over a few frames).
    pub fn set_find_query(&mut self, query: &str) {
        self.text.search.query = query.to_string();
    }

    /// Matches found so far.
    pub fn find_hits(&self) -> usize {
        self.text.search.hits().len()
    }

    /// Go to the next (or previous) match.
    pub fn find_step(&mut self, forward: bool) {
        if let Some(hit) = self.text.search.step(forward) {
            self.scroll_hit = Some(hit);
        }
    }

    /// Pages whose text is held.
    pub fn texts_held(&self) -> usize {
        self.text.store.held()
    }

    /// The zoom as a factor (1.0 is 100%), as of the last frame.
    pub fn zoom_factor(&self) -> f32 {
        self.layout.as_ref().map_or(1.0, Layout::factor)
    }

    pub fn set_zoom(&mut self, zoom: Zoom) {
        self.zoom = zoom;
    }

    pub fn zoom_in(&mut self) {
        self.zoom = Zoom::Factor(step(self.zoom_factor(), true));
    }

    pub fn zoom_out(&mut self) {
        self.zoom = Zoom::Factor(step(self.zoom_factor(), false));
    }

    /// Show or hide the thumbnail strip.
    pub fn set_thumbnails(&mut self, open: bool) {
        self.thumbs.open = open;
    }

    pub fn thumbnails_shown(&self) -> bool {
        self.thumbs.open
    }

    /// Show the table of contents (`true`) or the thumbnails in the sidebar.
    pub fn set_contents(&mut self, contents: bool) {
        self.contents = contents;
    }

    /// The entries of the open document's table of contents.
    pub fn outline(&self) -> &[OutlineEntry] {
        self.loaded.as_ref().map_or(&[], |l| l.outline.as_slice())
    }

    /// How many thumbnail pictures are held.
    pub fn thumbnails_held(&self) -> usize {
        self.thumbs.held()
    }

    /// How many page pictures are held.
    pub fn pictures_held(&self) -> usize {
        self.pictures.len()
    }

    /// Per-frame work before the interface: files that arrived.
    pub fn logic(&mut self, ctx: &egui::Context) {
        if let Some(inbox) = self.services.inbox.clone() {
            let files = std::mem::take(&mut *inbox.lock().unwrap_or_else(|e| e.into_inner()));
            if let Some((name, bytes)) = files.into_iter().last() {
                self.open_bytes(&name, bytes);
            }
        }
        // Dropped files: read here on native targets; in the browser they can only be read
        // asynchronously, so the web shell takes them first and they arrive in the inbox.
        let dropped = ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
        #[cfg(not(target_arch = "wasm32"))]
        for file in dropped {
            let name = file.path().file_name().map_or_else(|| "dropped file".to_string(), |n| n.to_string_lossy().into_owned());
            match file.bytes() {
                Ok(bytes) => self.open_bytes(&name, bytes),
                Err(e) => self.message = Some(format!("{name}: {e}")),
            }
        }
        #[cfg(target_arch = "wasm32")]
        drop(dropped);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.keys(&ctx);
        egui::Panel::top("pdf_toolbar").show(ui, |ui| self.toolbar(ui));
        if self.loaded.is_some() && self.text.search.open {
            egui::Panel::top("pdf_find").show(ui, |ui| self.find_bar(ui));
        }
        if self.loaded.is_some() && self.thumbs.open {
            let clicked = egui::Panel::left("pdf_thumbs_panel")
                .resizable(false)
                .exact_size(SIDEBAR_WIDTH)
                .frame(egui::Frame::NONE.fill(Color32::from_gray(0x38)))
                .show(ui, |ui| {
                    let has_contents = self.loaded.as_ref().is_some_and(|l| !l.outline.is_empty());
                    if !has_contents {
                        self.contents = false;
                    }
                    if has_contents {
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut self.contents, false, "Pages");
                            ui.selectable_value(&mut self.contents, true, "Contents");
                        });
                    }
                    match &self.loaded {
                        Some(l) if self.contents => contents_list(ui, &l.outline, self.current),
                        Some(l) => self.thumbs.show(ui, &l.doc, &l.sizes, self.current),
                        None => None,
                    }
                })
                .inner;
            if let Some(page) = clicked {
                self.go_to_page(page);
            }
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(BACKDROP)).show(ui, |ui| {
            self.banner(ui);
            if self.loaded.is_some() {
                self.pages(ui);
            } else {
                self.welcome(ui);
            }
        });
        self.password_window(&ctx);
    }

    fn keys(&mut self, ctx: &egui::Context) {
        if self.loaded.is_none() || self.password.is_some() {
            return;
        }
        let (find, escape, f3, shift_f3) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::F),
                self.text.search.open && i.consume_key(Modifiers::NONE, Key::Escape),
                i.consume_key(Modifiers::NONE, Key::F3),
                i.consume_key(Modifiers::SHIFT, Key::F3),
            )
        });
        if find {
            self.set_find_open(true);
        }
        if escape {
            self.set_find_open(false);
        }
        if f3 || shift_f3 {
            self.find_step(f3);
        }
        if select::copy_requested(ctx)
            && let Some(text) = self.selected_text()
        {
            ctx.copy_text(text);
        }
        let (pinch, down, up, home, end, plus, minus, zero) = ctx.input_mut(|i| {
            (
                i.zoom_delta(),
                i.consume_key(Modifiers::NONE, Key::PageDown),
                i.consume_key(Modifiers::NONE, Key::PageUp),
                i.consume_key(Modifiers::NONE, Key::Home),
                i.consume_key(Modifiers::NONE, Key::End),
                i.consume_key(Modifiers::COMMAND, Key::Equals) || i.consume_key(Modifiers::COMMAND, Key::Plus),
                i.consume_key(Modifiers::COMMAND, Key::Minus),
                i.consume_key(Modifiers::COMMAND, Key::Num0),
            )
        });
        if (pinch - 1.0).abs() > 1e-4 && pinch.is_finite() {
            self.zoom = Zoom::Factor(self.zoom_factor() * pinch);
        }
        if plus {
            self.zoom_in();
        }
        if minus {
            self.zoom_out();
        }
        if zero {
            self.zoom = Zoom::FitWidth;
        }
        let last = self.page_count().saturating_sub(1);
        if down {
            self.go_to_page((self.current + 1).min(last));
        }
        if up {
            self.go_to_page(self.current.saturating_sub(1));
        }
        if home {
            self.go_to_page(0);
        }
        if end {
            self.go_to_page(last);
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if self.services.open_async.is_some()
                && ui.button("Open…").clicked()
                && let Some(open) = &self.services.open_async
            {
                open();
            }
            if ui.button("Open sample").clicked() {
                self.open_bytes("Sample.pdf", sample::sample_pdf(SAMPLE_PAGES));
            }
            let Some(name) = self.loaded.as_ref().map(|l| l.name.clone()) else { return };
            ui.separator();
            ui.label(name);
            ui.separator();
            let mut shown = self.thumbs.open;
            if ui.toggle_value(&mut shown, "Pages").on_hover_text("Show page thumbnails").changed() {
                self.thumbs.open = shown;
            }
            let mut find = self.text.search.open;
            if ui.toggle_value(&mut find, "Find").on_hover_text("Search the text (Ctrl+F)").changed() {
                self.set_find_open(find);
            }
            ui.separator();
            let n = self.page_count();
            let mut page = self.current + 1;
            if ui.add(egui::DragValue::new(&mut page).range(1..=n.max(1)).speed(0.1)).changed() {
                self.go_to_page(page.saturating_sub(1));
            }
            ui.label(format!("/ {n}"));
            ui.separator();
            if ui.button("−").on_hover_text("Zoom out (Ctrl+−)").clicked() {
                self.zoom_out();
            }
            ui.label(format!("{:.0}%", self.zoom_factor() * 100.0));
            if ui.button("+").on_hover_text("Zoom in (Ctrl++)").clicked() {
                self.zoom_in();
            }
            if ui.button("Fit width").on_hover_text("Fit the page to the window (Ctrl+0)").clicked() {
                self.zoom = Zoom::FitWidth;
            }
        });
    }

    fn find_bar(&mut self, ui: &mut egui::Ui) {
        let mut step = None;
        let mut close = false;
        ui.horizontal(|ui| {
            let search = &mut self.text.search;
            let edit = ui.add(egui::TextEdit::singleline(&mut search.query).hint_text("Find in document").desired_width(220.0));
            if search.focus {
                edit.request_focus();
                search.focus = false;
            }
            if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                step = Some(!ui.input(|i| i.modifiers.shift));
                edit.request_focus();
            }
            if ui.button("▲").on_hover_text("Previous match (Shift+F3)").clicked() {
                step = Some(false);
            }
            if ui.button("▼").on_hover_text("Next match (F3)").clicked() {
                step = Some(true);
            }
            let hits = search.hits().len();
            let total = self.loaded.as_ref().map_or(0, |l| l.doc.page_count());
            let label = match (search.query.is_empty(), search.current()) {
                (true, _) => String::new(),
                (false, _) if hits == 0 && search.scanned() >= total => "No matches".to_string(),
                (false, Some(c)) => format!("{} of {hits}", c + 1),
                (false, None) => "Searching…".to_string(),
            };
            ui.label(label);
            if search.scanned() < total && !search.query.is_empty() {
                ui.weak(format!("({}/{total} pages)", search.scanned()));
            }
            if ui.small_button("×").on_hover_text("Close (Esc)").clicked() {
                close = true;
            }
        });
        if let Some(forward) = step {
            self.find_step(forward);
        }
        if close {
            self.set_find_open(false);
        }
    }

    fn banner(&mut self, ui: &mut egui::Ui) {
        let Some(message) = self.message.clone() else { return };
        ui.horizontal(|ui| {
            ui.colored_label(Color32::from_rgb(255, 190, 180), message);
            if ui.small_button("×").clicked() {
                self.message = None;
            }
        });
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.3).max(24.0));
            ui.label(egui::RichText::new("Open a PDF").size(26.0).color(Color32::WHITE));
            ui.add_space(8.0);
            let hint = if self.services.open_async.is_some() { "Drop a file here, or press Open…" } else { "Drop a file here, or open the sample" };
            ui.label(egui::RichText::new(hint).color(Color32::from_gray(0xdd)));
            ui.label(egui::RichText::new("Files stay on your computer: nothing is uploaded.").color(Color32::from_gray(0xbb)));
        });
    }

    fn pages(&mut self, ui: &mut egui::Ui) {
        let Self {
            loaded, zoom, layout: last_layout, scroll_to, scroll_hit, current, last_offset, last_scale, pictures, failed, text, column, ..
        } = self;
        let Some(loaded) = loaded.as_ref() else { return };
        let lay = layout(&loaded.sizes, *zoom, ui.available_width());
        let mut searching = false;
        if text.search.open || !text.search.query.is_empty() {
            searching = text.search.update(&loaded.doc);
            if text.search.jump
                && let Some(hit) = text.search.current_hit()
            {
                text.search.jump = false;
                *scroll_hit = Some(hit);
            }
        }
        // Where to scroll: to a page asked for, to a search match, or, when the zoom changed, to
        // the same place.
        let wanted = match (scroll_to.take(), scroll_hit.take()) {
            (Some(page), _) => lay.top(page).map(|t| (t - GAP).max(0.0)),
            (None, Some(hit)) => lay.top(hit.page).map(|t| (t + hit.top * lay.scale - select::HIT_MARGIN).max(0.0)),
            (None, None) if *last_scale > 0.0 && (lay.scale - *last_scale).abs() > 1e-4 => Some(*last_offset * lay.scale / *last_scale),
            (None, None) => None,
        };
        let mut area = egui::ScrollArea::both().id_salt("pdf_pages").auto_shrink([false, false]);
        if let Some(y) = wanted {
            area = area.vertical_scroll_offset(y);
        }
        let mut more = false;
        let out = area.show_viewport(ui, |ui, viewport| {
            let width = lay.content.x.max(ui.available_width());
            // Touch screens keep dragging for scrolling; the mouse selects text.
            let sense = if ui.input(|i| i.has_touch_screen()) { Sense::click() } else { Sense::click_and_drag() };
            let (rect, response) = ui.allocate_exact_size(vec2(width, lay.content.y), sense);
            *column = rect;
            more = draw_pages(ui, rect, viewport, &lay, &loaded.doc, pictures, failed, text);
            let on_screen = select::visible_pages(&lay, rect, viewport.min.y, viewport.max.y);
            select::interact(ui, &response, &on_screen, lay.scale, &loaded.doc, text);
        });
        *last_offset = out.state.offset.y;
        *last_scale = lay.scale;
        *current = lay.page_at(out.state.offset.y, out.inner_rect.height());
        *last_layout = Some(lay);
        if more || wanted.is_some() || searching {
            ui.ctx().request_repaint();
        }
    }

    fn password_window(&mut self, ctx: &egui::Context) {
        let Some(prompt) = &mut self.password else { return };
        let mut submit = None;
        let mut cancel = false;
        egui::Window::new("Password required")
            .id(Id::new("pdf-password"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, -40.0))
            .show(ctx, |ui| {
                ui.set_max_width(360.0);
                ui.label(format!("{} is password protected.", prompt.name));
                if prompt.wrong {
                    ui.colored_label(Color32::from_rgb(220, 60, 50), "That password is not correct.");
                }
                let edit = ui.add(egui::TextEdit::singleline(&mut prompt.input).password(true).hint_text("Password"));
                if prompt.focus {
                    edit.request_focus();
                    prompt.focus = false;
                }
                let entered = edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Open").clicked() || entered {
                        submit = Some(prompt.input.clone());
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if cancel {
            self.password = None;
        } else if let Some(password) = submit {
            self.submit_password(&password);
        }
    }
}

/// The table of contents as a list; returns the page of the entry clicked.
fn contents_list(ui: &mut egui::Ui, outline: &[OutlineEntry], current: usize) -> Option<usize> {
    let mut clicked = None;
    egui::ScrollArea::vertical().id_salt("pdf_contents").auto_shrink([false, false]).show(ui, |ui| {
        for entry in outline {
            ui.horizontal(|ui| {
                ui.add_space(8.0 * entry.depth as f32);
                let title = if entry.title.trim().is_empty() { "(untitled)" } else { entry.title.as_str() };
                let label = egui::RichText::new(title).color(Color32::WHITE);
                let response =
                    ui.add_enabled(entry.page.is_some(), egui::Button::selectable(entry.page == Some(current), label).frame_when_inactive(false));
                if response.clicked() {
                    clicked = entry.page;
                }
            });
        }
    });
    clicked
}

/// The width in pixels to draw a page at when it takes `page_width_px` on screen: rounded up to a
/// multiple of 16 so a small change of zoom reuses a picture, and held to what a texture can be
/// both ways.
fn picture_width(page_width_px: f32, aspect: f32, max_side: f32) -> u32 {
    let limit = max_side.min(if aspect > 1.0 { max_side / aspect } else { max_side }).max(16.0);
    let wanted = page_width_px.clamp(1.0, limit);
    (((wanted / 16.0).ceil() * 16.0).min(limit.floor())).max(1.0) as u32
}

/// Draw the pages that show in `viewport` (positions in the column whose top left is `rect.min`);
/// returns whether pages still wait for their picture.
fn draw_pages(
    ui: &mut egui::Ui,
    rect: Rect,
    viewport: Rect,
    lay: &Layout,
    doc: &PdfDoc,
    pictures: &mut Cache<TextureHandle>,
    failed: &mut HashSet<usize>,
    text: &mut TextState,
) -> bool {
    let ctx = ui.ctx().clone();
    let ppp = ctx.pixels_per_point();
    let max_side = ctx.input(|i| i.max_texture_side) as f32;
    let painter = ui.painter_at(ui.clip_rect());
    let mut budget = RENDERS_PER_FRAME;
    let mut more = false;
    for index in lay.visible(viewport.min.y - PREFETCH, viewport.max.y + PREFETCH) {
        let Some(local) = lay.page_rect(index, rect.width()) else { continue };
        let page = local.translate(rect.min.to_vec2());
        let width = picture_width(page.width() * ppp, page.height() / page.width().max(1.0), max_side);
        if pictures.exact(index, width).is_none() && !failed.contains(&index) {
            if budget == 0 {
                more = true;
            } else if page.intersects(ui.clip_rect()) {
                budget -= 1;
                match render_page(doc, index, width as f32) {
                    Ok(img) if (img.width as f32) <= max_side && (img.height as f32) <= max_side => {
                        let size = [img.width as usize, img.height as usize];
                        let image = egui::ColorImage::from_rgba_unmultiplied(size, &img.rgba);
                        let handle = ctx.load_texture(format!("pdf-page-{index}-{width}"), image, TextureOptions::LINEAR);
                        pictures.insert(index, width, handle, u64::from(img.width) * u64::from(img.height));
                    }
                    Ok(_) => {
                        failed.insert(index);
                    }
                    Err(e) => {
                        log::warn!("page {}: {e}", index + 1);
                        failed.insert(index);
                    }
                }
            }
        }
        painter.rect_filled(page.translate(vec2(0.0, 2.0)).expand(1.0), 0.0, Color32::from_black_alpha(70));
        painter.rect_filled(page, 0.0, Color32::WHITE);
        let picture = pictures.exact(index, width).map(TextureHandle::id).or_else(|| pictures.nearest(index, width).map(TextureHandle::id));
        match picture {
            Some(id) => {
                painter.image(id, page, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                let label = if failed.contains(&index) { "This page can't be drawn".to_string() } else { format!("Page {}", index + 1) };
                painter.text(page.center(), Align2::CENTER_CENTER, label, egui::FontId::proportional(16.0), Color32::from_gray(0x99));
            }
        }
        if page.intersects(ui.clip_rect())
            && let Some(page_text) = text.store.get(doc, index)
        {
            select::paint_highlights(&painter, page, lay.scale, index, &page_text, text);
        }
    }
    more
}

#[cfg(test)]
mod tests;
