use super::*;
use crate::sample::{sample_pdf, sample_pdf_sized};

#[test]
fn opens_a_sample_and_reports_its_pages() {
    let bytes = sample_pdf(3);
    let len = bytes.len();
    let doc = PdfDoc::open(bytes, None).unwrap();
    assert_eq!(doc.page_count(), 3);
    assert_eq!(doc.byte_len(), len);
    for i in 0..3 {
        assert_eq!(doc.page_size(i), Some(PageSize { width: 612.0, height: 792.0 }), "page {i}");
    }
    assert_eq!(doc.page_size(3), None, "no fourth page");
    assert_eq!(doc.page_size(usize::MAX), None);
}

#[test]
fn a_sample_has_at_least_one_and_at_most_the_most_pages() {
    assert_eq!(PdfDoc::open(sample_pdf(0), None).unwrap().page_count(), 1);
    assert_eq!(PdfDoc::open(sample_pdf(25), None).unwrap().page_count(), 25);
}

#[test]
fn nothing_and_not_a_pdf_are_refused_by_name() {
    assert_eq!(PdfDoc::open(Vec::new(), None).err(), Some(OpenError::Empty));
    assert_eq!(PdfDoc::open(b"This is plain text, not a PDF.".to_vec(), None).err(), Some(OpenError::Invalid));
    assert_eq!(PdfDoc::open(b"%PDF-1.7\n".to_vec(), None).err(), Some(OpenError::Invalid));
    assert!(!OpenError::Invalid.to_string().is_empty());
}

#[test]
fn a_file_over_the_limit_is_refused_before_it_is_read() {
    assert_eq!(check_len(MAX_FILE_BYTES), Ok(()));
    assert_eq!(check_len(MAX_FILE_BYTES + 1), Err(OpenError::TooLarge { bytes: MAX_FILE_BYTES + 1 }));
    assert_eq!(check_len(0), Err(OpenError::Empty));
}

#[test]
fn page_sizes_a_file_can_claim_are_made_sane() {
    let sane = |s: PageSize| {
        s.width.is_finite()
            && s.height.is_finite()
            && (MIN_PAGE_POINTS..=MAX_PAGE_POINTS).contains(&s.width)
            && (MIN_PAGE_POINTS..=MAX_PAGE_POINTS).contains(&s.height)
    };
    // A huge page keeps its proportions inside the limit.
    let huge = PageSize::sane(2_000_000.0, 1_000_000.0);
    assert!(sane(huge) && (huge.width / huge.height - 2.0).abs() < 1e-3, "{huge:?}");
    // Not sizes at all: shown as Letter.
    for (w, h) in [(0.0, 0.0), (-5.0, 100.0), (100.0, -5.0), (f32::NAN, 10.0), (10.0, f32::INFINITY), (f32::NEG_INFINITY, 1.0)] {
        assert_eq!(PageSize::sane(w, h), FALLBACK_PAGE, "{w} x {h}");
    }
    // Very thin: not thinner than a point.
    assert!(sane(PageSize::sane(1.0e9, 1.0)));
    // And through a real file.
    for (w, h) in [(1.0e9, 1.0e9), (0.0, 0.0), (-10.0, 20.0), (1.0e12, 5.0), (0.001, 0.001)] {
        let doc = PdfDoc::open(sample_pdf_sized(1, w, h), None).unwrap();
        assert!(doc.page_size(0).is_some_and(sane), "{w} x {h}: {:?}", doc.page_size(0));
    }
}

/// Hostile input: every cut-off of a file and a few hundred files with random bytes changed open or
/// are refused, and none of them takes the reader down.
#[test]
fn damaged_files_are_opened_or_refused_and_never_crash_the_reader() {
    let good = sample_pdf(3);
    let touch = |bytes: Vec<u8>, what: &str| match PdfDoc::open(bytes, None) {
        Ok(doc) => {
            for i in 0..doc.page_count().min(50) {
                let _ = doc.page_size(i);
            }
        }
        Err(e) => assert_ne!(e, OpenError::Crashed, "{what}"),
    };
    for len in 0..good.len() {
        touch(good.get(..len).unwrap_or_default().to_vec(), &format!("cut at {len}"));
    }
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for round in 0..300 {
        let mut bytes = good.clone();
        for _ in 0..(1 + round % 40) {
            let at = (next() % bytes.len() as u64) as usize;
            if let Some(b) = bytes.get_mut(at) {
                *b = (next() % 256) as u8;
            }
        }
        touch(bytes, &format!("mutation {round}"));
    }
}

const RC4: &[u8] = include_bytes!("../tests/data/encrypted-rc4.pdf");
const AES256: &[u8] = include_bytes!("../tests/data/encrypted-aes256.pdf");

#[test]
fn an_encrypted_file_asks_for_its_password_and_opens_with_it() {
    for (name, bytes) in [("RC4-128", RC4), ("AES-256", AES256)] {
        assert_eq!(PdfDoc::open(bytes.to_vec(), None).err(), Some(OpenError::PasswordRequired), "{name}");
        assert_eq!(PdfDoc::open(bytes.to_vec(), Some("")).err(), Some(OpenError::PasswordRequired), "{name}");
        assert_eq!(PdfDoc::open(bytes.to_vec(), Some("wrong")).err(), Some(OpenError::WrongPassword), "{name}");
        let doc = PdfDoc::open(bytes.to_vec(), Some("secret")).unwrap();
        assert_eq!(doc.page_count(), 3, "{name}");
        assert_eq!(doc.page_size(0), Some(PageSize { width: 612.0, height: 792.0 }), "{name}");
    }
}
