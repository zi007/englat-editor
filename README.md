# ENGLAT Editor

A free, open-source word processor written in Rust. It runs natively on macOS, Windows and Linux,
and in the browser via WebAssembly, reads and writes `.docx`, and can export PDF.

ENGLAT Editor is an **independent ENGLAT.COM project** built on [WordCraft](https://github.com/storytold/wordcraft)
and [craft-libs](https://github.com/storytold/craft-libs) by the ArtCraft Team and contributors
(both MIT OR Apache-2.0). It is not made, endorsed or supported by the ArtCraft Team. See
[NOTICE](NOTICE) and [Licence](#licence).

> **Status: work in progress.** The rename and de-branding from WordCraft is done, the code builds,
> and the next steps are the ENGLAT icon, a measured WebAssembly build and the integration on
> [englat.com](https://englat.com). Nothing here is a release yet.

## What it does

The feature set is inherited from WordCraft: a ribbon interface, styles, tables, track changes,
references, mail merge, spelling and hyphenation, and a command system that is also exposed on the
command line and over MCP. The honest, area-by-area picture is in [ROADMAP.md](ROADMAP.md),
[docs/target-app-parity.md](docs/target-app-parity.md) and the generated
[docs/parity-checklist.md](docs/parity-checklist.md). Those documents describe the upstream project
and have not been re-measured for ENGLAT Editor yet.

## Quick start

```sh
git clone https://github.com/zi007/englat-editor
cd englat-editor
cargo run --release -p englat-editor -- --sample        # desktop app with the sample document
cargo run --release -p englat-editor -- report.docx     # open a document
```

Command line:

```sh
englat-editor-cli convert report.docx report.pdf        # docx, pdf, odt, rtf, html, md, tex, txt, png
englat-editor-cli text report.docx                      # plain text
englat-editor-cli inspect report.docx                   # structure as JSON
```

Web (needs [Trunk](https://trunkrs.dev) and the `wasm32-unknown-unknown` target):
`cd apps/englat-editor-web && trunk serve`, then open <http://127.0.0.1:8771/?sample>.

Requires a recent stable Rust toolchain (see `rust-version` in `Cargo.toml`). All dependencies,
including the vendored craft-libs snapshots in `vendor/`, resolve without fetching from the
upstream git repositories.

## Layout

| Path | Contents |
|---|---|
| `crates/` | engine, document model, layout, `.docx` I/O, PDF export, proofing, egui UI, MCP server |
| `apps/englat-editor` | desktop app |
| `apps/englat-editor-cli` | command-line tool and headless MCP server |
| `apps/englat-editor-web` | browser build (eframe/WebAssembly) |
| `vendor/craft-ui`, `vendor/craft-pdf` | pinned craft-libs snapshots (UI, printing, PDF writer, Krilla) |
| `docs/` | architecture, parity assessments, file formats, MCP and control protocol |

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Copyright (c) 2026 ArtCraft Team and the WordCraft contributors (original work), with modifications
by ENGLAT.COM and the ENGLAT Editor contributors. Required third-party notices, including fonts and
hyphenation data, are in [NOTICE](NOTICE) and [ATTRIBUTION.md](ATTRIBUTION.md).
