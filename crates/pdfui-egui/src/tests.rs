use super::*;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use wordcraft_pdfdoc::sample::{MAX_SAMPLE_PAGES, sample_pdf, sample_pdf_outlined as sample_outlined, sample_pdf_sized};

type H = Harness<'static, PdfApp>;

const RC4: &[u8] = include_bytes!("../../pdfdoc/tests/data/encrypted-rc4.pdf");

fn window(app: PdfApp) -> H {
    Harness::builder().with_size(vec2(1000.0, 800.0)).build_ui_state(
        |ui, app: &mut PdfApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    )
}

fn steps(h: &mut H, n: usize) {
    for _ in 0..n {
        h.step();
    }
}

fn app_with(bytes: Vec<u8>) -> H {
    let mut app = PdfApp::new(Services::default());
    app.open_bytes("test.pdf", bytes);
    let mut h = window(app);
    steps(&mut h, 6);
    h
}

#[test]
fn it_starts_with_nothing_open_and_says_how_to_open_something() {
    let mut h = window(PdfApp::new(Services::default()));
    steps(&mut h, 3);
    assert_eq!(h.state().page_count(), 0);
    assert_eq!(h.state().message(), None);
    assert_eq!(h.state().pictures_held(), 0);
    assert!(!h.state().needs_password());
    h.get_by_label_contains("Open a PDF");
}

#[test]
fn a_sample_opens_fitted_to_the_window_with_its_first_pages_drawn() {
    let h = app_with(sample_pdf(5));
    let app = h.state();
    assert_eq!(app.page_count(), 5);
    assert_eq!(app.current_page(), 0);
    // The page fills the window's width less the margins (the test window's panel takes a little of
    // the 1000 points itself).
    let column = 612.0 * app.zoom_factor() * layout::POINT_TO_UI + 2.0 * layout::MARGIN;
    assert!((940.0..=1000.0).contains(&column), "a column {column} wide in a window 1000 wide");
    // Only the pages that show were drawn: at this size, not all five.
    assert!(app.pictures_held() >= 1 && app.pictures_held() < 5, "{} pictures", app.pictures_held());
}

#[test]
fn zoom_steps_change_the_size_and_fit_width_brings_it_back() {
    let mut h = app_with(sample_pdf(3));
    let fitted = h.state().zoom_factor();
    h.state_mut().zoom_out();
    steps(&mut h, 3);
    assert!(h.state().zoom_factor() < fitted, "zoomed out from {fitted}");
    h.state_mut().set_zoom(Zoom::Factor(1.0));
    steps(&mut h, 3);
    assert!((h.state().zoom_factor() - 1.0).abs() < 1e-3);
    h.state_mut().zoom_in();
    steps(&mut h, 3);
    assert!((h.state().zoom_factor() - 1.25).abs() < 1e-3);
    h.state_mut().set_zoom(Zoom::FitWidth);
    steps(&mut h, 3);
    assert!((h.state().zoom_factor() - fitted).abs() < 1e-3);
    // Far past the limits: held to them.
    h.state_mut().set_zoom(Zoom::Factor(1.0e6));
    steps(&mut h, 3);
    assert!(h.state().zoom_factor() <= layout::MAX_ZOOM + 1e-3);
}

#[test]
fn going_to_a_page_scrolls_there_and_the_keys_move_by_a_page() {
    let mut h = app_with(sample_pdf(20));
    h.state_mut().go_to_page(7);
    steps(&mut h, 4);
    assert_eq!(h.state().current_page(), 7);
    h.key_press(Key::PageDown);
    steps(&mut h, 4);
    assert_eq!(h.state().current_page(), 8);
    h.key_press(Key::PageUp);
    steps(&mut h, 4);
    assert_eq!(h.state().current_page(), 7);
    h.key_press(Key::End);
    steps(&mut h, 4);
    assert_eq!(h.state().current_page(), 19);
    h.key_press(Key::Home);
    steps(&mut h, 4);
    assert_eq!(h.state().current_page(), 0);
    // Past the end is the last page.
    h.state_mut().go_to_page(10_000);
    steps(&mut h, 4);
    assert_eq!(h.state().current_page(), 19);
}

#[test]
fn a_file_that_does_not_open_says_why_and_leaves_the_open_one_alone() {
    let mut h = app_with(sample_pdf(5));
    h.state_mut().open_bytes("notes.txt", b"just some text".to_vec());
    steps(&mut h, 2);
    assert!(h.state().message().is_some_and(|m| m.starts_with("notes.txt:")), "{:?}", h.state().message());
    assert_eq!(h.state().page_count(), 5, "the document that was open stays");
    h.state_mut().open_bytes("empty.pdf", Vec::new());
    assert!(h.state().message().is_some_and(|m| m.contains("empty")), "{:?}", h.state().message());
    // A good file clears the message.
    h.state_mut().open_bytes("good.pdf", sample_pdf(2));
    steps(&mut h, 2);
    assert_eq!(h.state().message(), None);
    assert_eq!(h.state().page_count(), 2);
}

#[test]
fn an_encrypted_file_asks_for_its_password() {
    let mut h = app_with(sample_pdf(2));
    h.state_mut().open_bytes("secret.pdf", RC4.to_vec());
    steps(&mut h, 2);
    assert!(h.state().needs_password());
    assert_eq!(h.state().page_count(), 2, "until it opens, the document that was open stays");
    h.get_by_label_contains("is password protected");
    h.state_mut().submit_password("not it");
    steps(&mut h, 2);
    assert!(h.state().needs_password());
    h.get_by_label_contains("not correct");
    h.state_mut().cancel_password();
    assert!(!h.state().needs_password());
    h.state_mut().open_bytes("secret.pdf", RC4.to_vec());
    h.state_mut().submit_password("secret");
    steps(&mut h, 4);
    assert!(!h.state().needs_password());
    assert_eq!(h.state().page_count(), 3);
    assert_eq!(h.state().message(), None);
}

/// Hostile input: a page that claims to be a billion points across is laid out as the largest page
/// and drawn within the limits (the reader's own size is tiny to draw); a page too thin to draw at
/// all is refused, not allocated.
#[test]
fn pages_of_hostile_sizes_are_laid_out_and_drawn_within_the_limits_or_refused() {
    let h = app_with(sample_pdf_sized(2, 1.0e9, 1.0e9));
    assert_eq!(h.state().page_count(), 2);
    assert!(h.state().pictures_held() <= 2);
    let h = app_with(sample_pdf_sized(2, 1.0e12, 5.0));
    assert_eq!(h.state().page_count(), 2);
    assert_eq!(h.state().pictures_held(), 0);
    assert!(!h.state().failed.is_empty());
}

#[test]
fn a_long_document_keeps_a_bounded_number_of_pictures() {
    let mut h = app_with(sample_pdf(MAX_SAMPLE_PAGES));
    assert_eq!(h.state().page_count(), MAX_SAMPLE_PAGES);
    h.state_mut().set_zoom(Zoom::Factor(layout::MIN_ZOOM));
    for page in [0, 500, 4_000, 9_999, 2_500, 7_000] {
        h.state_mut().go_to_page(page);
        steps(&mut h, 8);
        assert!(h.state().pictures_held() <= cache::MAX_ENTRIES, "{} pictures at page {page}", h.state().pictures_held());
    }
    // At this zoom a window shows several pages: the reader is on one of the first ones.
    assert!((7_000..=7_004).contains(&h.state().current_page()), "{}", h.state().current_page());
}

#[test]
fn files_arriving_in_the_inbox_are_opened_and_the_last_one_wins() {
    let inbox = Inbox::default();
    let mut h = window(PdfApp::new(Services { inbox: Some(inbox.clone()), ..Default::default() }));
    inbox.lock().unwrap().push(("a.pdf".into(), sample_pdf(2)));
    inbox.lock().unwrap().push(("b.pdf".into(), sample_pdf(4)));
    steps(&mut h, 3);
    assert_eq!(h.state().page_count(), 4);
    assert!(inbox.lock().unwrap().is_empty());
    h.get_by_label_contains("b.pdf");
}

#[test]
fn the_open_button_is_there_only_when_the_platform_can_open_files() {
    let mut h = window(PdfApp::new(Services::default()));
    steps(&mut h, 2);
    assert!(h.query_by_label("Open…").is_none());
    h.get_by_label("Open sample");
    let asked = std::rc::Rc::new(std::cell::Cell::new(0));
    let counter = asked.clone();
    let mut h = window(PdfApp::new(Services { open_async: Some(Box::new(move || counter.set(counter.get() + 1))), ..Default::default() }));
    steps(&mut h, 2);
    h.get_by_label("Open…").click();
    steps(&mut h, 2);
    assert_eq!(asked.get(), 1);
}

#[test]
fn the_sample_button_opens_the_sample() {
    let mut h = window(PdfApp::new(Services::default()));
    steps(&mut h, 2);
    h.get_by_label("Open sample").click();
    steps(&mut h, 4);
    assert_eq!(h.state().page_count(), SAMPLE_PAGES);
}

#[test]
fn a_picture_is_a_multiple_of_sixteen_wide_and_never_past_what_a_texture_can_be() {
    assert_eq!(picture_width(100.0, 1.3, 4096.0), 112);
    assert_eq!(picture_width(112.0, 1.3, 4096.0), 112);
    // Held to the texture side, and for a tall page to the side by its height too.
    assert_eq!(picture_width(10_000.0, 1.0, 2048.0), 2048);
    assert_eq!(picture_width(10_000.0, 4.0, 2048.0), 512);
    for bad in [0.0, -3.0, f32::NAN] {
        let w = picture_width(bad, 1.3, 2048.0);
        assert!((1..=2048).contains(&w), "{bad} -> {w}");
    }
    assert!(picture_width(500.0, 1.0, 1.0) >= 1);
}

#[test]
fn the_thumbnail_strip_draws_pages_and_a_click_goes_to_one() {
    let mut h = app_with(sample_pdf(8));
    assert!(!h.state().thumbnails_shown());
    h.state_mut().set_thumbnails(true);
    steps(&mut h, 12);
    assert!(h.state().thumbnails_held() >= 2, "thumbnails drawn: {}", h.state().thumbnails_held());
    assert!(h.state().thumbnails_held() <= 16);
    // Click the third thumbnail: its cell is below the first two, inside the strip.
    let (w, y) = (8.0 + 56.0, 8.0 + 2.0 * (112.0 * 792.0 / 612.0 + 18.0 + 8.0) + 40.0);
    let at = egui::pos2(w, y + 40.0);
    h.hover_at(at);
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.step();
    }
    steps(&mut h, 6);
    assert_eq!(h.state().current_page(), 2);
}

#[test]
fn the_contents_list_shows_the_outline_and_a_click_goes_to_its_page() {
    let mut h = app_with(sample_outlined(6));
    assert_eq!(h.state().outline().len(), 6);
    h.state_mut().set_thumbnails(true);
    h.state_mut().set_contents(true);
    steps(&mut h, 6);
    h.get_by_label("Page 5").click();
    steps(&mut h, 6);
    assert_eq!(h.state().current_page(), 4);
}

#[test]
fn a_file_without_bookmarks_has_no_contents_tab() {
    let mut h = app_with(sample_pdf(3));
    h.state_mut().set_thumbnails(true);
    h.state_mut().set_contents(true);
    steps(&mut h, 6);
    assert!(h.state().outline().is_empty());
    assert!(h.query_by_label("Contents").is_none());
}

/// The screen position of the middle of character `index` of page 0's text, found by looking the
/// characters up the way the window draws them.
fn char_pos(h: &H, page: usize, index: usize) -> egui::Pos2 {
    let app = h.state();
    let lay = app.layout.as_ref().unwrap();
    let doc = &app.loaded.as_ref().unwrap().doc;
    let text = wordcraft_pdfview::text::page_text(doc, page).unwrap();
    let r = text.chars[index].rect;
    let origin = app.column.min + lay.page_rect(page, app.column.width()).unwrap().min.to_vec2();
    origin + vec2((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0) * lay.scale
}

fn index_of(h: &H, page: usize, needle: &str) -> (usize, usize) {
    let doc = &h.state().loaded.as_ref().unwrap().doc;
    wordcraft_pdfview::text::page_text(doc, page).unwrap().find(needle)[0]
}

fn drag(h: &mut H, from: egui::Pos2, to: egui::Pos2) {
    h.hover_at(from);
    h.step();
    h.drag_at(from);
    h.step();
    for k in 1..=4 {
        h.hover_at(from + (to - from) * (k as f32 / 4.0));
        h.step();
    }
    h.drop_at(to);
    steps(h, 2);
}

#[test]
fn dragging_across_text_selects_it_and_a_click_clears_it() {
    let mut h = app_with(sample_pdf(2));
    let (first, last) = index_of(&h, 0, "quick brown");
    let (a, b) = (char_pos(&h, 0, first), char_pos(&h, 0, last));
    drag(&mut h, a, b);
    assert_eq!(h.state_mut().selected_text().as_deref(), Some("quick brown"));
    // A click somewhere else clears the selection.
    let at = char_pos(&h, 0, index_of(&h, 0, "Pack my").0);
    h.hover_at(at);
    h.step();
    h.drag_at(at);
    h.step();
    h.drop_at(at);
    steps(&mut h, 2);
    assert_eq!(h.state_mut().selected_text(), None);
}

#[test]
fn a_selection_dragged_backwards_reads_the_same_and_copy_puts_it_on_the_clipboard() {
    let mut h = app_with(sample_pdf(1));
    let (first, last) = index_of(&h, 0, "lazy dog");
    let (a, b) = (char_pos(&h, 0, last), char_pos(&h, 0, first));
    drag(&mut h, a, b);
    assert_eq!(h.state_mut().selected_text().as_deref(), Some("lazy dog"));
    h.event(egui::Event::Copy);
    h.step();
    let copied = h.output().platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t == "lazy dog"));
    assert!(copied, "{:?}", h.output().platform_output.commands);
}

#[test]
fn copy_with_nothing_selected_copies_nothing() {
    let mut h = app_with(sample_pdf(1));
    h.event(egui::Event::Copy);
    h.step();
    assert!(!h.output().platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::CopyText(_))));
}

#[test]
fn a_search_finds_every_match_ignoring_case_and_steps_to_other_pages() {
    let mut h = app_with(sample_pdf(4));
    h.state_mut().set_find_open(true);
    h.state_mut().set_find_query("QUICK");
    steps(&mut h, 12);
    // "quick" is on two lines of each of the four pages.
    assert_eq!(h.state().find_hits(), 8);
    assert_eq!(h.state().current_page(), 0);
    for _ in 0..3 {
        h.state_mut().find_step(true);
        steps(&mut h, 3);
    }
    assert_eq!(h.state().current_page(), 1, "the fourth match is on the second page");
    h.state_mut().find_step(false);
    h.state_mut().find_step(false);
    h.state_mut().find_step(false);
    h.state_mut().find_step(false);
    steps(&mut h, 3);
    assert_eq!(h.state().find_hits(), 8, "stepping doesn't change the matches");
    // A new query replaces the matches; no match means none.
    h.state_mut().set_find_query("zzzz");
    steps(&mut h, 8);
    assert_eq!(h.state().find_hits(), 0);
    h.state_mut().set_find_query("");
    steps(&mut h, 3);
    assert_eq!(h.state().find_hits(), 0);
}

#[test]
fn ctrl_f_opens_the_find_bar_and_escape_closes_it() {
    let mut h = app_with(sample_pdf(2));
    assert!(!h.state().find_open());
    h.key_press_modifiers(Modifiers::COMMAND, Key::F);
    steps(&mut h, 3);
    assert!(h.state().find_open());
    h.key_press(Key::Escape);
    steps(&mut h, 3);
    assert!(!h.state().find_open());
}

#[test]
fn opening_another_file_drops_the_selection_and_the_matches() {
    let mut h = app_with(sample_pdf(2));
    h.state_mut().set_find_open(true);
    h.state_mut().set_find_query("quick");
    steps(&mut h, 8);
    assert!(h.state().find_hits() > 0);
    h.state_mut().open_bytes("other.pdf", sample_pdf(1));
    steps(&mut h, 8);
    // The query stays, and is run again on the new document: one page, two lines.
    assert_eq!(h.state().find_hits(), 2);
    assert_eq!(h.state_mut().selected_text(), None);
}

#[test]
fn the_text_held_for_selecting_stays_bounded_in_a_long_document() {
    let mut h = app_with(sample_pdf(MAX_SAMPLE_PAGES));
    for page in (0..MAX_SAMPLE_PAGES).step_by(7) {
        h.state_mut().go_to_page(page);
        steps(&mut h, 4);
    }
    assert!(h.state().texts_held() <= 48, "{} pages of text held", h.state().texts_held());
}
