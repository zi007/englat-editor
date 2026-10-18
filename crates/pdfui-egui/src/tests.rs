use super::*;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use wordcraft_pdfdoc::sample::{MAX_SAMPLE_PAGES, sample_pdf, sample_pdf_sized};

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
    let (w, y) = (14.0 + 56.0, 8.0 + 2.0 * (112.0 * 792.0 / 612.0 + 18.0 + 8.0) + 40.0);
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
