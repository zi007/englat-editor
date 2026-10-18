# PDF editor (pdfcraft-web)

> **Last reviewed:** 2026-10-18 · **Last updated:** 2026-10-18 · **Change:** major (first version: plan only, no code yet) · **Status:** proposal, not started

A browser PDF editor that lives in this repository next to WordCraft and shares its libraries. This
page is the plan; nothing here is built yet. Rules for working in the code are in
[`AGENTS.md`](../AGENTS.md) and apply unchanged (never crash, no proprietary assets, Rust only,
never break wasm).

## Decision: a second app, not a second mode

`apps/pdfcraft-web` is its own binary and its own wasm bundle, built from the same workspace. It
shares the lower layers with WordCraft and adds crates of its own; it does not change `doc`,
`layout` or `docx`. Two reasons: the wasm size of one app does not grow with the other, and a PDF
bug cannot take the Word editor down. A shared shell (tabs for both document types) can be added
later on top of the same crates; the reverse is much harder.

## Constraints

- **Everything stays in the browser.** Files are opened and saved locally; nothing is uploaded.
- **Pure Rust.** No PDFium or pdf.js. If a milestone cannot be met without them, that is a
  decision for the owner, not a quiet dependency.
- **Never crash.** PDF files are hostile input: no `unwrap`/`expect`/`panic!`, bounded recursion
  and allocation, `get()` instead of indexing, a regression test with every crash fix.
- **No Microsoft, Adobe, Avid or Autodesk assets**, including icons, stamps and templates. Every
  asset has a row in `ATTRIBUTION.md`. Stamps and icons are drawn in code or public domain/CC0/OSI.
- **Licences.** Only MIT/Apache-2.0/BSD-style dependencies; no GPL/LGPL/AGPL.

## Crates (proposed)

| Layer | Crate | Job |
|---|---|---|
| L1 | `pdfdoc` | The editable PDF model: pages, boxes, rotation, annotations, form fields, outline; opens bytes, reports damage instead of failing, saves (full rewrite first, incremental later) |
| L3 | `pdfview` | Renders a page to pixels (hayro interpreter into `vello_cpu`), text layer for selection and search, thumbnails |
| L4 | `pdfengine` | `PdfSession` (document, selection, undo, view state) and a command registry for `pdf.*` commands, same conventions as the Word one |
| L6 | `pdfui-egui` | Viewer chrome built on `craft-ui` (icons, tabs, docking): toolbar, page strip, inspector, dialogs |
| app | `pdfcraft-web` | eframe web runner, WebGPU with WebGL2 fallback, like `wordcraft-web` |

Reused as they are: `geom` (units), `fonts` (font database, shaping, outlines), `render`, `pdf`
(writer, for new content) and the vendored `craft-ui`. Open question for milestone 0: the command
registry types live in `engine` (L4, Word-specific). Either `pdfengine` depends on a small shared
`commands` crate extracted from it, or it has its own copy; extracting is preferred, but it touches
`engine` and is its own change.

`cargo xtask layers` must learn the new crates (its table rejects unknown crates), and
`cargo xtask wasm` must include them.

## Dependencies to confirm in milestone 0

| Need | Candidate | To check |
|---|---|---|
| Parse, render | `hayro-syntax`, `hayro-interpret` (already used by `pdf` tests) | wasm32 build, speed on large files, fonts and image codecs it needs |
| Edit objects, write | `lopdf` (MIT) or `pdf-writer` + `hayro-syntax` | wasm32 build, handling of broken xref, encrypted files, incremental update |
| Fonts for new text | `fonts` crate | subsetting and embedding through the existing writer |

The existing writer (`krilla`) only authors new documents and cannot edit one; it stays the tool
for new content such as stamps and watermarks.

## Milestones

Each milestone ships on its own and keeps CI green.

0. **Spike (no UI).** Open, render page 1 and save an untouched copy in wasm, for a set of test
   files including damaged, encrypted and very large ones. Decide the editing library from the
   table above and the command registry question. Output: a short decision record in this file.
1. **Viewer.** Open a file (drop or picker), pages, zoom and fit, thumbnails, outline, text
   selection, copy, search, print through the browser. Keyboard and touch.
2. **Page operations.** Reorder, rotate, delete, insert blank, extract, split, merge several files;
   undo/redo; save a new file.
3. **Annotations and forms.** Highlight, underline, strike, note, free text, shapes, ink, stamps
   drawn in code, links; fill and flatten AcroForm fields; undo/redo; saved as standard annotations
   other readers show.
4. **Content editing.** Add text and images; move, resize, delete existing images and text
   blocks; edit existing text runs where the embedded font subset has the glyphs; redaction
   (the content is removed, not covered); metadata; password protection and removal.
5. **Later.** Signatures, OCR, compare, compress.

### What "maximum editing" can and cannot mean

Milestones 1–3 are well understood and complete in any reader. Editing existing text is limited by
how a PDF stores it: positioned glyphs from embedded font *subsets*, often without the words or
line breaks. In milestone 4 a text run can be changed where its font has the new characters, and
otherwise is replaced with a re-embedded font; reflowing a whole page like a word processor is not
promised. Turning a PDF into an editable Word document is the separate Word-side feature "PDF
Reflow" in [gaps.md](gaps.md) (#23); it can use `pdfdoc` as its reader.

## Testing

- Unit tests per crate; a hostile-input test that opens truncated and mutated files for every
  loader and every command (`pdf.*` params junk), like `hostile_params_never_panic` for Word.
- A corpus of small test PDFs we make ourselves or that are public domain, each listed in
  `ATTRIBUTION.md`; no files of unknown origin.
- Rendering checked headless against reference PNGs (as `ui_shot` does for Word); screenshots read,
  not assumed.
- Wasm: `cargo xtask wasm` for the library crates, and a browser smoke test of the built app.

## Docs to update when each milestone lands

[`gaps.md`](gaps.md) (the ranked work list), a new `pdf-parity.md` (what is done against a
reference editor, measured, not claimed), `roadmap.md` (Current focus) and `ROADMAP.md`'s progress
log, each with its status line and revision history bumped.

## Risks

- **Wasm size and speed** of a pure-Rust renderer on big scans; milestone 0 measures this before
  any UI work.
- **Library fit.** If no pure-Rust library edits PDFs safely enough, milestone 4 shrinks or
  needs a new decision.
- **CI time.** More crates mean longer builds; the FreeBSD job already takes about half an hour.
- **Scope.** This is months of work; each milestone stands alone so it can stop at any of them.
