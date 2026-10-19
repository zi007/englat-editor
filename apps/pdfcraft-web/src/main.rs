//! The PDF viewer in the browser.
//!
//! Runs [`wordcraft_pdfui_egui::PdfApp`] through eframe's web runner (wgpu: WebGPU where available,
//! WebGL2 otherwise). Build with `trunk build --release` from this directory (output in
//! `dist/pdfcraft-web`). Files are opened in the browser: they are read by the page and never
//! uploaded.
//!
//! URL flags: `?webgl` forces WebGL2; `?sample` opens the sample PDF.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
fn main() {
    web::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("pdfcraft-web only runs in the browser: build it with `trunk build --release` in apps/pdfcraft-web");
}
