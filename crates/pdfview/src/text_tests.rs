use super::*;
use wordcraft_pdfdoc::sample::sample_pdf;

fn open(bytes: Vec<u8>) -> PdfDoc {
    PdfDoc::open(bytes, None).unwrap()
}

fn chars(s: &str, line: u32, x0: f32, w: f32, top: f32) -> Vec<TextChar> {
    s.chars().enumerate().map(|(k, ch)| TextChar { ch, rect: [x0 + w * k as f32, top, x0 + w * (k + 1) as f32, top + 10.0], line }).collect()
}

#[test]
fn the_sample_page_text_is_read_in_order_with_line_breaks() {
    let doc = open(sample_pdf(2));
    let text = page_text(&doc, 0).unwrap();
    let all = text.text();
    assert!(all.contains("The quick brown fox jumps over the lazy dog."), "{all:?}");
    assert!(all.contains("Pack my box with five dozen liquor jugs."), "{all:?}");
    assert!(all.contains("How vexingly quick daft zebras jump!"), "{all:?}");
    let fox = all.find("The quick").unwrap();
    let jugs = all.find("Pack my").unwrap();
    assert!(fox < jugs, "lines come in drawing order");
    assert!(all[fox..jugs].contains('\n'), "a line break between the first two lines");
}

#[test]
fn characters_sit_where_the_page_draws_them() {
    let doc = open(sample_pdf(1));
    let text = page_text(&doc, 0).unwrap();
    // The body text is 12 points high, starting 54 points from the left; later lines are lower.
    let at = |needle: &str| text.chars[text.find(needle)[0].0].clone();
    let first = at("The quick");
    assert!((first.rect[0] - 54.0).abs() < 1.5, "{:?}", first.rect);
    let h = first.rect[3] - first.rect[1];
    assert!((h - 12.0).abs() < 1.0, "a 12 point glyph box is about 12 high, got {h}");
    let pack = at("Pack my");
    assert!(pack.rect[1] > first.rect[1] + 10.0, "the second line is below the first");
    assert!(text.chars.iter().all(|c| c.rect.iter().all(|v| v.is_finite())));
}

#[test]
fn a_page_that_does_not_exist_or_has_no_text_is_handled() {
    let doc = open(sample_pdf(1));
    assert_eq!(page_text(&doc, 5), Err(RenderError::NoSuchPage));
    assert_eq!(PageText::default().text(), "");
    assert_eq!(PageText::default().index_at(1.0, 1.0), None);
    assert!(PageText::default().find("a").is_empty());
}

#[test]
fn a_point_picks_the_character_under_it_or_the_nearest_one_on_that_line() {
    let mut t = PageText { chars: chars("abc", 0, 10.0, 5.0, 100.0) };
    t.chars.extend(chars("xyz", 1, 10.0, 5.0, 120.0));
    assert_eq!(t.index_at(12.0, 105.0), Some(0));
    assert_eq!(t.index_at(22.0, 105.0), Some(2));
    // Past the end of line 0, still nearest to its last character, not to line 1.
    assert_eq!(t.index_at(300.0, 104.0), Some(2));
    // Between the lines, nearer the second.
    assert_eq!(t.index_at(11.0, 118.0), Some(3));
    assert_eq!(t.index_at(f32::NAN, 1.0), None);
}

#[test]
fn copied_text_has_spaces_for_gaps_and_newlines_for_lines() {
    let mut chars_ = chars("ab", 0, 10.0, 5.0, 100.0);
    chars_.extend(chars("cd", 0, 30.0, 5.0, 100.0)); // a wide gap: a space
    chars_.extend(chars("ef", 1, 10.0, 5.0, 120.0));
    let t = PageText { chars: chars_ };
    assert_eq!(t.text(), "ab cd\nef");
    assert_eq!(t.text_between(1, 4), "b cd\ne");
    assert_eq!(t.text_between(4, 1), "b cd\ne", "either order");
    assert_eq!(t.text_between(0, 999), "ab cd\nef", "an end past the page is clamped");
}

#[test]
fn selection_boxes_are_one_per_line() {
    let mut chars_ = chars("abc", 0, 10.0, 5.0, 100.0);
    chars_.extend(chars("def", 1, 10.0, 5.0, 120.0));
    let t = PageText { chars: chars_ };
    let boxes = t.boxes_between(1, 4);
    assert_eq!(boxes, vec![[15.0, 100.0, 25.0, 110.0], [10.0, 120.0, 20.0, 130.0]]);
}

#[test]
fn search_ignores_case_and_does_not_cross_lines() {
    let mut chars_ = chars("Fox fox", 0, 10.0, 5.0, 100.0);
    chars_.extend(chars("ox", 1, 10.0, 5.0, 120.0));
    let t = PageText { chars: chars_ };
    assert_eq!(t.find("FOX"), vec![(0, 2), (4, 6)]);
    assert_eq!(t.find("xox"), Vec::<(usize, usize)>::new(), "x at the end of line 0 and 'ox' on line 1 don't join");
    assert!(t.find("").is_empty());
    assert!(t.find("a much longer needle than the page has characters").is_empty());
}

#[test]
fn damaged_files_never_panic_when_their_text_is_read() {
    let good = sample_pdf(2);
    for cut in (0..good.len()).step_by(37) {
        let mut bytes = good.clone();
        bytes.truncate(cut);
        if let Ok(doc) = PdfDoc::open(bytes, None) {
            for page in 0..doc.page_count().min(3) {
                let _ = page_text(&doc, page);
            }
        }
    }
}
