# ENGLAT Editor: instructions for agents

ENGLAT Editor is an independent, ENGLAT.COM-maintained fork of WordCraft (MIT OR Apache-2.0).
Upstream design documents under `docs/` still describe the inherited architecture; read
`docs/architecture.md` before changing structure.

## Rules
- Work on a task branch and open a PR; never commit to `main`, release or publish without the
  owner's explicit approval.
- Keep licences intact: do not edit `LICENSE-MIT`, `LICENSE-APACHE` or the upstream parts of
  `NOTICE`; keep "WordCraft contributors" and "ArtCraft Team" copyright lines. New third-party
  material needs a row in `ATTRIBUTION.md` (`cargo xtask assets` enforces it).
- Do not use the ArtCraft name, logos or brand files, and do not imply ArtCraft endorsement.
- No Microsoft, Adobe, Avid or Autodesk icons, images, templates, fonts or other assets, and no
  copying from their products or from GPL/LGPL/AGPL code. Assets must be original or openly
  licensed.
- Never commit secrets, tokens or credentials.
- No new network calls, telemetry or paid services without the owner's approval.

## Engineering
- No panics in non-test code (`unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`); errors are
  `Result`. Input-derived numbers are hostile: bound sizes and use checked arithmetic.
- Everything user-visible is a command in `crates/engine/src/cmd/`; the ribbon, shortcuts, CLI,
  control channel and MCP dispatch by command id.
- Nothing below the UI crate may depend on egui/eframe/winit. Do not break the WebAssembly build.
- Before a commit, run `cargo xtask ci` (fmt, clippy, tests, assets, layers, wasm) when feasible and
  report exactly which checks ran.
