use super::*;
use wordcraft_pdfdoc::sample::{sample_pdf, sample_pdf_sized};

fn open(bytes: Vec<u8>) -> PdfDoc {
    PdfDoc::open(bytes, None).unwrap()
}

fn pixel(img: &PageImage, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * img.width + x) * 4) as usize;
    [img.rgba[at], img.rgba[at + 1], img.rgba[at + 2], img.rgba[at + 3]]
}

fn near(a: [u8; 4], b: [u8; 3]) -> bool {
    (0..3).all(|k| (i32::from(a[k]) - i32::from(b[k])).abs() <= 12) && a[3] == 255
}

#[test]
fn a_sample_page_is_drawn_where_the_page_says() {
    let doc = open(sample_pdf(2));
    let img = render_page(&doc, 0, 306.0).unwrap();
    // Half size: a 612 x 792 page at 306 pixels wide.
    assert_eq!((img.width, img.height), (306, 396));
    assert_eq!(img.rgba.len(), 306 * 396 * 4);
    // The page is white and opaque, and the corner is empty.
    assert_eq!(pixel(&img, 2, 2), [255, 255, 255, 255]);
    // The blue rectangle is 220 x 36 points, 72 points down and 54 across: here at half size.
    assert!(near(pixel(&img, 125, 40), [31, 89, 191]), "{:?}", pixel(&img, 125, 40));
    assert_eq!(pixel(&img, 100, 200), [255, 255, 255, 255], "below it the page is white");
    // There is dark text on the page (small and anti-aliased at half size); the blue rectangle isn't dark.
    let dark = img.rgba.chunks_exact(4).filter(|p| p[0] < 170 && p[1] < 170 && p[2] < 170).count();
    assert!(dark > 150, "{dark} dark pixels");
}

#[test]
fn a_larger_width_gives_a_larger_picture_of_the_same_page() {
    let doc = open(sample_pdf(1));
    let small = render_page(&doc, 0, 200.0).unwrap();
    let large = render_page(&doc, 0, 800.0).unwrap();
    assert_eq!((small.width, small.height), (200, 258));
    assert_eq!((large.width, large.height), (800, 1035));
}

#[test]
fn a_missing_page_is_named() {
    let doc = open(sample_pdf(2));
    assert_eq!(render_page(&doc, 2, 100.0), Err(RenderError::NoSuchPage));
    assert_eq!(render_page(&doc, usize::MAX, 100.0), Err(RenderError::NoSuchPage));
}

#[test]
fn the_width_asked_for_is_held_to_the_limits() {
    let letter = (612.0, 792.0);
    let draw = |target| scale_for(letter.0, letter.1, target).map(|s| ((letter.0 * s) as u32, (letter.1 * s) as u32));
    assert_eq!(draw(612.0), Some((612, 792)));
    // Asked for far more than the limits: reduced, and still within them.
    let (w, h) = draw(1.0e7).unwrap();
    assert!(w <= MAX_SIDE_PX && h <= MAX_SIDE_PX && u64::from(w) * u64::from(h) <= MAX_PIXELS, "{w} x {h}");
    // Not sizes.
    for bad in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(draw(bad), None, "{bad}");
    }
    // A page too thin or too large for a picture within the limits is refused, not stretched.
    assert_eq!(scale_for(1.0e9, 1.0, 100.0), None);
    assert_eq!(scale_for(1.0, 1.0e9, 100.0), None);
    assert_eq!(scale_for(f32::NAN, 10.0, 100.0), None);
    assert_eq!(scale_for(0.0, 0.0, 100.0), None);
    // A tall page is held to the longest side, not only to the width.
    let (w, h) = scale_for(100.0, 10_000.0, 100.0).map(|s| ((100.0 * s) as u32, (10_000.0 * s) as u32)).unwrap();
    assert!(h <= MAX_SIDE_PX, "{w} x {h}");
}

/// Hostile input: the sizes a file claims for its pages never become an allocation.
#[test]
fn a_page_of_any_claimed_size_is_drawn_within_the_limits_or_refused() {
    for (w, h) in [(1.0e9, 1.0e9), (1.0e12, 5.0), (0.0, 0.0), (-10.0, 20.0), (0.001, 0.001), (100000.0, 100000.0), (14400.0, 14400.0), (612.0, 792.0)] {
        let doc = open(sample_pdf_sized(1, w, h));
        match render_page(&doc, 0, 800.0) {
            Ok(img) => assert!(img.width <= MAX_SIDE_PX && img.height <= MAX_SIDE_PX && u64::from(img.width) * u64::from(img.height) <= MAX_PIXELS, "{w} x {h}"),
            Err(e) => assert_ne!(e, RenderError::Crashed, "{w} x {h}"),
        }
    }
}

/// Hostile input: damaged files that still open are drawn or refused, and none takes the
/// rasteriser down.
#[test]
fn damaged_pages_are_drawn_or_refused_and_never_crash_the_rasteriser() {
    let good = sample_pdf(2);
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut drawn = 0;
    for round in 0..250 {
        let mut bytes = good.clone();
        for _ in 0..(1 + round % 30) {
            let at = (next() % bytes.len() as u64) as usize;
            if let Some(b) = bytes.get_mut(at) {
                *b = (next() % 256) as u8;
            }
        }
        let Ok(doc) = PdfDoc::open(bytes, None) else { continue };
        for page in 0..doc.page_count().min(3) {
            match render_page(&doc, page, 200.0) {
                Ok(_) => drawn += 1,
                Err(e) => assert_ne!(e, RenderError::Crashed, "mutation {round}, page {page}"),
            }
        }
    }
    assert!(drawn > 0, "some damaged files still draw");
}
