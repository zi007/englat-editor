# PDF editor (pdfcraft-web)

> **Last reviewed:** 2026-10-18 · **Last updated:** 2026-10-18 · **Change:** minor (milestone 1 first slice: open, scroll, zoom, page jump, password; previously minor: milestone 0 results and library decision; previously major: first version, plan only) · **Status:** milestone 0 done, milestone 1 started

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

## Dependencies (confirmed in milestone 0)

| Need | Candidate | To check |
|---|---|---|
| Parse, render | `hayro-syntax`, `hayro-interpret` (already used by `pdf` tests) | wasm32 build, speed on large files, fonts and image codecs it needs |
| Edit objects, write | `lopdf` (MIT) or `pdf-writer` + `hayro-syntax` | wasm32 build, handling of broken xref, encrypted files, incremental update |
| Fonts for new text | `fonts` crate | subsetting and embedding through the existing writer |

The existing writer (`krilla`) only authors new documents and cannot edit one; it stays the tool
for new content such as stamps and watermarks.

## Milestones

Each milestone ships on its own and keeps CI green.

0. **Spike (no UI). Done, see [Milestone 0 results](#milestone-0-results).** Open, render page 1 and save an untouched copy in wasm, for a set of test
   files including damaged, encrypted and very large ones. Decide the editing library from the
   table above and the command registry question. Output: a short decision record in this file.
1. **Viewer. First slice done, see [Milestone 1 progress](#milestone-1-progress).** Open a file (drop or picker), pages, zoom and fit, thumbnails, outline, text
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

## Milestone 0 results

Run on 2026-10-18 with `hayro` 0.8.0 / `hayro-syntax` 0.8.0 and `lopdf` 0.45.0 (default features
off), natively on all 26 files and as `wasm32-unknown-unknown` run in Node 22 (V8, the same engine as Chrome) on 9 of them (the good ones, the 60 MB and 3000-page files, one corrupted copy, garbage, empty and the truncated paper). The
spike was a throw-away crate outside the workspace; it opened each file, rendered page 1 at 1x (hayro)
and loaded, saved and reloaded it (lopdf). Corpus: 26 files, each a file we made, public domain, or
mutated from those: a 3-page file, a 3000-page file, a 60 MB file with 500 pages of images, a
WordCraft export, a LibreOffice sample, a truncated paper, 12 randomly corrupted copies, an empty
file, garbage, a header only, a file without its xref table, a half-cut file, and RC4-128, AES-128
and AES-256 encrypted copies (user password `secret`, made with `lopdf`).

| Question | Result |
|---|---|
| Builds for wasm32 | Both do. `lopdf` needs `getrandom = { version = "0.4", features = ["wasm_js"] }` as a wasm32-only dependency (it pulls `getrandom` for encryption); `hayro` needs nothing. |
| Panics on hostile input | None in any of the files, native or wasm. Guard calls with `catch_unwind` natively; note that on `wasm32-unknown-unknown` a panic aborts the module (Rust's default there), so the guard cannot catch it. "Never panic" has to hold by construction and by fuzzing the loaders, and this milestone did not fuzz. How `Session::run`'s guard behaves in the existing web build is not verified here. |
| Speed (wasm, Node) | `hayro` opens the 60 MB file and renders page 1 in 135 ms and the 3000-page file in 27 ms (opening is lazy). `lopdf` loads, saves and reloads the 60 MB file in 382 ms. Page 1 of these files is simple; heavy pages were not measured. |
| Memory (wasm) | For the 60 MB file the module's memory peaked at 135 MB for `hayro` and 298 MB for `lopdf`. The probe itself holds two copies of the input (120 MB), so `hayro` added little (it parses lazily) and `lopdf`, which loads, saves and reloads, added about 180 MB, roughly three times the file. All far below the 4 GB wasm32 limit, but a very large file means a visible allocation. |
| Damaged files | `hayro` is lenient: it recovered a file without its xref table, a half-cut file and 5 of 12 randomly corrupted copies, and refused the rest cleanly. `lopdf` is stricter and refused more, but it **opened 4 corrupted copies as documents with 0 pages and saved them**, which would destroy the content. Rule: never save a file through `lopdf` unless its page count matches what `hayro` found. |
| Encrypted files | `hayro` opens RC4-128, AES-128 and AES-256 files with the right password, and reports "password protected" for an empty or wrong one. `lopdf` does the same, and can also write all three. Only files `lopdf` made were tried; files from other producers are not covered. The `hayro` README says encrypted PDFs are unsupported, but the parser decrypts; the README looks out of date. |
| Rendering correctness | Page 1 of the WordCraft export and the LibreOffice sample matched `pdftoppm` by eye. These are plain text pages: this says nothing about images, gradients, transparency or complex fonts. `hayro` documents these gaps: blending and isolation, knockout groups, colour-key masking. |
| Size | The wasm module with both libraries is 5.4 MB (2.0 MB gzipped), before `wasm-opt` and with `hayro`'s embedded standard fonts. Using the `fonts` crate instead of the embedded ones should shrink it. |
| Editing support | `lopdf` has `IncrementalDocument` (append-only saves that keep the original bytes, which keeps signatures valid) and writes encryption. |

### Decision

- **Reader and renderer: `hayro`.** It is the source of truth for "what is in this file".
- **Object editing and saving: `lopdf`**, behind a `pdfdoc` API that refuses to save when the
  `lopdf` page count differs from `hayro`'s, and uses incremental saves where possible.
- **Open question (not settled by this spike):** the shared `commands` crate for the registry; decide
  when `pdfengine` is started.

### Not done in milestone 0

- Fuzzing of both loaders; a licence check of the full dependency trees (`cargo deny` is not in CI
  yet); heavy-page rendering performance; a browser (not Node) run; text extraction, search and
  font handling.

## Milestone 1 progress

First slice, in the browser app `apps/pdfcraft-web` (and tested natively):

| Done | Where |
|---|---|
| Open a PDF from the file picker, a drop, or the built-in sample; the file is read by the page and never uploaded | `pdfcraft-web`, `pdfui-egui` |
| Encrypted files ask for the password (RC4-128, AES-128, AES-256 tested); a wrong password says so | `pdfdoc`, `pdfui-egui` |
| Damaged, empty and non-PDF files say why and leave the open document alone | `pdfdoc`, `pdfui-egui` |
| Pages in a scrolling column, drawn as they scroll into view, a few per frame | `pdfui-egui`, `pdfview` |
| Zoom: buttons, Ctrl+scroll or pinch, Ctrl+0 fits the width; fit width by default; scroll position kept when zooming | `pdfui-egui` |
| Page number box and PageUp, PageDown, Home, End | `pdfui-egui` |
| Memory held to a fixed amount however long the document (least recently used pictures dropped) | `pdfui-egui` |
| Limits a file can't raise: page sizes made sane, pictures capped in size and pixels, texture side respected | `pdfdoc`, `pdfview`, `pdfui-egui` |

Checked in Chromium (WebGL2, software rasteriser) with the built wasm: the sample opens at 131% fitted to
the window, PageDown twice reads page 3, `+` goes to 150% with horizontal scrolling, End reads page 8.
The wasm module is 15.7 MB (4.9 MB gzipped) before `wasm-opt`, most of it egui and wgpu.

Not done yet in milestone 1: thumbnails, outline, text selection, copy and search, printing, touch
gestures beyond the egui defaults, and a text layer. Known limits of this slice:

- **Drawing happens on the interface thread.** A very heavy page can make a frame slow, and in the
  browser there is no second thread. Moving it off (a worker) is later work.
- **Pictures are at most 4096 pixels a side**, so a page zoomed far in is a scaled picture, not
  re-drawn sharp. Tiled drawing is later work.
- **No commands yet.** The viewer is plain UI state; `AGENTS.md` says every user-visible action is a
  registered command. `pdfengine` (the registry, undo, MCP and CLI access) arrives with milestone 2,
  when there is something to undo.
- English text only: the interface strings are not yet in the translation catalogs.

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
