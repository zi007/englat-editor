//! The table of contents (bookmarks) of a PDF: a flat list of entries in reading order, each with
//! its depth, title and the page it goes to. The file's tree is hostile input: it may loop, run
//! very deep or wide, or point nowhere, so reading is bounded and an entry that goes nowhere keeps
//! `page: None`.

use std::collections::{HashMap, HashSet};

use hayro_syntax::object::ObjectIdentifier;
use hayro_syntax::object::dict::keys::{DEST, DESTS, FIRST, NAMES, NEXT, OUTLINES, TITLE};
use hayro_syntax::object::{Array, Dict, MaybeRef, Name, ObjRef, Object};

use crate::PdfDoc;

/// Most entries returned.
pub const MAX_ENTRIES: usize = 20_000;
/// Deepest nesting followed; deeper entries are left out.
pub const MAX_DEPTH: usize = 16;
/// Longest title kept, in characters.
pub const MAX_TITLE_CHARS: usize = 300;
/// Most nodes visited in a name tree while looking for one name.
const MAX_TREE_NODES: usize = 10_000;

/// One bookmark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineEntry {
    /// Nesting, from 0.
    pub depth: usize,
    pub title: String,
    /// The page it goes to (from 0), if it could be worked out.
    pub page: Option<usize>,
}

impl PdfDoc {
    /// The table of contents, empty when the file has none (or it is unreadable).
    pub fn outline(&self) -> Vec<OutlineEntry> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read(self))).unwrap_or_default()
    }
}

fn read(doc: &PdfDoc) -> Vec<OutlineEntry> {
    let pdf = doc.pdf();
    let xref = pdf.xref();
    let Some(catalog) = xref.get::<Dict>(xref.root_id()) else { return Vec::new() };
    let Some(root) = catalog.get::<Dict>(OUTLINES) else { return Vec::new() };
    let pages: HashMap<ObjectIdentifier, usize> = pdf.pages().iter().enumerate().filter_map(|(i, p)| p.raw().obj_id().map(|id| (id, i))).collect();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    walk(&root, 0, &catalog, &pages, &mut out, &mut seen);
    out
}

/// The entries below `parent`, depth first.
fn walk(
    parent: &Dict,
    depth: usize,
    catalog: &Dict,
    pages: &HashMap<ObjectIdentifier, usize>,
    out: &mut Vec<OutlineEntry>,
    seen: &mut HashSet<ObjectIdentifier>,
) {
    if depth >= MAX_DEPTH {
        return;
    }
    let mut next = parent.get::<Dict>(FIRST);
    while let Some(item) = next {
        if out.len() >= MAX_ENTRIES {
            return;
        }
        // A loop in the sibling or child links visits an item a second time.
        match item.obj_id() {
            Some(id) if !seen.insert(id) => return,
            _ => {}
        }
        let title = item.get::<hayro_syntax::object::String>(TITLE).map(|s| decode_title(s.as_bytes())).unwrap_or_default();
        out.push(OutlineEntry { depth, title, page: destination(&item, catalog, pages) });
        walk(&item, depth + 1, catalog, pages, out, seen);
        next = item.get::<Dict>(NEXT);
    }
}

/// A text string: UTF-16 with a byte order mark, or (nearly) Latin-1 otherwise. Control characters
/// are dropped and the length is capped.
fn decode_title(bytes: &[u8]) -> String {
    let text: String = if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        char::decode_utf16(rest.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]]))).map(|r| r.unwrap_or('\u{FFFD}')).collect()
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        char::decode_utf16(rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]))).map(|r| r.unwrap_or('\u{FFFD}')).collect()
    } else if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(rest).into_owned()
    } else {
        bytes.iter().map(|b| char::from(*b)).collect()
    };
    text.chars().filter(|c| !c.is_control()).take(MAX_TITLE_CHARS).collect()
}

/// The page an item goes to: its `/Dest`, or the `/D` of a go-to action.
fn destination(item: &Dict, catalog: &Dict, pages: &HashMap<ObjectIdentifier, usize>) -> Option<usize> {
    if let Some(dest) = item.get::<Object>(DEST) {
        return resolve(dest, catalog, pages);
    }
    let action = item.get::<Dict>(b"A")?;
    if action.get::<Name>(b"S")?.as_str() != "GoTo" {
        return None;
    }
    resolve(action.get::<Object>(b"D")?, catalog, pages)
}

fn resolve(dest: Object, catalog: &Dict, pages: &HashMap<ObjectIdentifier, usize>) -> Option<usize> {
    match dest {
        Object::Array(a) => page_of(&a, pages),
        Object::Dict(d) => page_of(&d.get::<Array>(b"D")?, pages),
        Object::Name(n) => named(n.as_ref(), catalog, pages),
        Object::String(s) => named(s.as_bytes(), catalog, pages),
        _ => None,
    }
}

/// The page a destination array `[page /Fit ...]` names.
fn page_of(array: &Array, pages: &HashMap<ObjectIdentifier, usize>) -> Option<usize> {
    match array.raw_iter().next()? {
        MaybeRef::Ref(r) => pages.get(&ObjRef::into(r)).copied(),
        MaybeRef::NotRef(_) => None,
    }
}

/// A named destination: the catalog's `/Dests` dictionary, or its `/Names` `/Dests` name tree.
fn named(name: &[u8], catalog: &Dict, pages: &HashMap<ObjectIdentifier, usize>) -> Option<usize> {
    if let Some(dests) = catalog.get::<Dict>(DESTS)
        && let Some(found) = dests.get::<Object>(name)
    {
        return resolve_value(found, pages);
    }
    let tree = catalog.get::<Dict>(NAMES)?.get::<Dict>(DESTS)?;
    let mut budget = MAX_TREE_NODES;
    let found = tree_lookup(&tree, name, &mut budget)?;
    resolve_value(found, pages)
}

/// A destination value is an array or a dictionary holding one under `/D`.
fn resolve_value(value: Object, pages: &HashMap<ObjectIdentifier, usize>) -> Option<usize> {
    match value {
        Object::Array(a) => page_of(&a, pages),
        Object::Dict(d) => page_of(&d.get::<Array>(b"D")?, pages),
        _ => None,
    }
}

/// Look `name` up in a name tree node (leaf `/Names` pairs, or `/Kids`), visiting a bounded number
/// of nodes.
fn tree_lookup<'a>(node: &Dict<'a>, name: &[u8], budget: &mut usize) -> Option<Object<'a>> {
    *budget = budget.checked_sub(1)?;
    if let Some(names) = node.get::<Array>(b"Names") {
        let mut it = names.flex_iter();
        while let Some(key) = it.next::<hayro_syntax::object::String>() {
            let value = it.next::<Object>()?;
            if key.as_bytes() == name {
                return Some(value);
            }
        }
    }
    let kids = node.get::<Array>(b"Kids")?;
    for kid in kids.iter::<Dict>() {
        if let Some(found) = tree_lookup(&kid, name, budget) {
            return Some(found);
        }
        if *budget == 0 {
            break;
        }
    }
    None
}

#[cfg(test)]
pub(crate) fn decode_title_for_test(bytes: &[u8]) -> String {
    decode_title(bytes)
}
