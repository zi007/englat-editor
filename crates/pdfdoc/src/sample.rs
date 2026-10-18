//! Sample PDFs made in code: our own text and colours, no files. They give the viewer something to
//! show before a file is opened, and give tests a well-formed file of any length or page size.
//!
//! Each page has its number in bold white on a blue rectangle, three lines of text in the standard
//! Helvetica (no embedded font) and a rule: text and vector drawing, which is enough to see that a page
//! was drawn and where.

/// The most pages a sample has, whatever is asked.
pub const MAX_SAMPLE_PAGES: usize = 10_000;

/// A well-formed PDF of `pages` US Letter pages (at least one, at most [`MAX_SAMPLE_PAGES`]).
pub fn sample_pdf(pages: usize) -> Vec<u8> {
    sample_pdf_sized(pages, 612.0, 792.0)
}

/// Like [`sample_pdf`], with every page `width` by `height` points. The values are written as they
/// are, so tests can ask for sizes a real file would never have.
pub fn sample_pdf_sized(pages: usize, width: f64, height: f64) -> Vec<u8> {
    build(pages, width, height, false)
}

/// A sample with a table of contents (at least three pages): "Chapter 1" (page 1) with the nested
/// "Section" (page 2), then one entry per later page; the last one reaches its page through a named
/// destination, the others directly.
pub fn sample_pdf_outlined(pages: usize) -> Vec<u8> {
    build(pages.max(3), 612.0, 792.0, true)
}

fn build(pages: usize, width: f64, height: f64, outlined: bool) -> Vec<u8> {
    let pages = pages.clamp(1, MAX_SAMPLE_PAGES);
    let extra = 5 + 2 * pages; // id of the outline root
    let page_id = |i: usize| 5 + 2 * i + 1;
    let mut objects: Vec<Vec<u8>> = Vec::new();
    // 1 catalog, 2 page tree, 3 and 4 fonts; then a content stream and a page for each page.
    let kids: Vec<String> = (0..pages).map(|i| format!("{} 0 R", 5 + 2 * i + 1)).collect();
    if outlined {
        let last = page_id(pages - 1);
        objects.push(format!("<< /Type /Catalog /Pages 2 0 R /Outlines {extra} 0 R /Dests << /last [{last} 0 R /Fit] >> >>").into_bytes());
    } else {
        objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    }
    objects.push(format!("<< /Type /Pages /Count {pages} /Kids [{}] >>", kids.join(" ")).into_bytes());
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".to_vec());
    for i in 0..pages {
        let n = i + 1;
        let top = height - 72.0;
        let content = format!(
            "q 0.12 0.35 0.75 rg 54 {} 220 36 re f Q\n\
             q 0.5 w 54 {} m {} {} l S Q\n\
             q BT 1 g /F2 26 Tf 62 {} Td (Page {n} of {pages}) Tj ET Q\n\
             BT /F1 12 Tf 54 {} Td 16 TL (The quick brown fox jumps over the lazy dog.) Tj T* (Pack my box with five dozen liquor jugs.) Tj T* (How vexingly quick daft zebras jump!) Tj ET\n",
            top - 36.0,
            top - 60.0,
            width - 54.0,
            top - 60.0,
            top - 28.0,
            top - 84.0,
        );
        let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        stream.extend_from_slice(content.as_bytes());
        stream.extend_from_slice(b"endstream");
        objects.push(stream);
        let content_id = 5 + 2 * i;
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Contents {content_id} 0 R /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> >>"
            )
            .into_bytes(),
        );
    }
    if outlined {
        // Items 0 (Chapter 1), 1 (Section, its child), 2.. (later pages) are ids extra + 1 + k.
        let item = |k: usize| extra + 1 + k;
        let tops: Vec<usize> = std::iter::once(0).chain(2..pages).collect();
        objects.push(format!("<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {} >>", item(0), item(pages - 1), pages).into_bytes());
        for k in 0..pages {
            let title = match k {
                0 => "Chapter 1".to_string(),
                1 => "Section".to_string(),
                _ => format!("Page {}", k + 1),
            };
            let target = if k + 1 == pages { "/Dest /last".to_string() } else { format!("/Dest [{} 0 R /Fit]", page_id(k)) };
            let parent = if k == 1 { item(0) } else { extra };
            let pos = tops.iter().position(|t| *t == k);
            let prev = pos.and_then(|p| p.checked_sub(1)).map(|p| format!(" /Prev {} 0 R", item(tops[p]))).unwrap_or_default();
            let next = pos.and_then(|p| tops.get(p + 1)).map(|t| format!(" /Next {} 0 R", item(*t))).unwrap_or_default();
            let child = if k == 0 { format!(" /First {} 0 R /Last {} 0 R /Count 1", item(1), item(1)) } else { String::new() };
            objects.push(format!("<< /Title ({title}) /Parent {parent} 0 R {target}{prev}{next}{child} >>").into_bytes());
        }
    }
    let mut out = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (k, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", k + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}
