//! The PDF a person opens: its pages and their sizes.
//!
//! [`PdfDoc::open`] reads a file with `hayro-syntax`, which recovers from many broken files
//! (a missing cross-reference table, a cut-off end) and refuses the rest. A PDF is hostile input:
//! nothing here panics on a bad file, sizes are sanitised, and a panic inside the parser comes back
//! as [`OpenError::Crashed`] on native targets. (On `wasm32-unknown-unknown` a panic aborts the
//! module, so the guarantee there rests on the parser not panicking; see `docs/pdf-editor.md`.)
//!
//! This is the read side. Editing and saving live in later crates (`docs/pdf-editor.md`).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod outline;
pub mod sample;

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

pub use hayro_syntax::Pdf;
use hayro_syntax::{DecryptionError, LoadPdfError};

/// The largest file [`PdfDoc::open`] takes. The whole file is held in memory (in the browser too),
/// and PDF tools typically need a few times its size.
pub const MAX_FILE_BYTES: usize = 1 << 30;

/// Smallest and largest page side [`PdfDoc::page_size`] reports, in points (1/72 inch). The PDF
/// specification's limit is 14 400 points (200 inches); files can say anything.
pub const MIN_PAGE_POINTS: f32 = 1.0;
pub const MAX_PAGE_POINTS: f32 = 14_400.0;

/// A page that has no usable size is shown as US Letter.
const FALLBACK_PAGE: PageSize = PageSize { width: 612.0, height: 792.0 };

/// Why a file did not open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenError {
    /// No bytes at all.
    Empty,
    /// Over [`MAX_FILE_BYTES`].
    TooLarge { bytes: usize },
    /// Encrypted, and it opens only with a password (none was given, or it was empty).
    PasswordRequired,
    /// Encrypted, and the password given is not the right one.
    WrongPassword,
    /// Encrypted in a way this reader can't open.
    UnsupportedEncryption,
    /// Not a PDF, or too damaged to read.
    Invalid,
    /// The reader failed on this file (a panic inside it, caught on native targets).
    Crashed,
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "The file is empty."),
            Self::TooLarge { bytes } => write!(f, "The file is too large ({} MB; the limit is {} MB).", bytes >> 20, MAX_FILE_BYTES >> 20),
            Self::PasswordRequired => write!(f, "This PDF is password protected."),
            Self::WrongPassword => write!(f, "That password is not correct."),
            Self::UnsupportedEncryption => write!(f, "This PDF uses encryption that can't be opened here."),
            Self::Invalid => write!(f, "This file is not a PDF, or it is too damaged to read."),
            Self::Crashed => write!(f, "This PDF could not be read."),
        }
    }
}

impl std::error::Error for OpenError {}

/// A page's size in points, its rotation applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

impl PageSize {
    /// A size a viewer can lay out: finite, positive, each side within
    /// [`MIN_PAGE_POINTS`]..=[`MAX_PAGE_POINTS`] with the proportions kept; the fallback for a size
    /// that is not a size.
    fn sane(width: f32, height: f32) -> Self {
        if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
            return FALLBACK_PAGE;
        }
        let down = (MAX_PAGE_POINTS / width.max(height)).min(1.0);
        Self { width: (width * down).max(MIN_PAGE_POINTS), height: (height * down).max(MIN_PAGE_POINTS) }
    }
}

/// An opened PDF.
pub struct PdfDoc {
    pdf: Pdf,
    bytes: usize,
}

impl PdfDoc {
    /// Open a PDF from its bytes. `password` is tried when the file is encrypted (the empty
    /// password is tried anyway, as most protected files open with it for viewing).
    pub fn open(bytes: Vec<u8>, password: Option<&str>) -> Result<Self, OpenError> {
        Self::open_shared(Arc::new(bytes), password)
    }

    /// Like [`PdfDoc::open`], for bytes the caller keeps (to try another password on the same
    /// file without copying it).
    pub fn open_shared(bytes: Arc<Vec<u8>>, password: Option<&str>) -> Result<Self, OpenError> {
        check_len(bytes.len())?;
        let len = bytes.len();
        let given = password.unwrap_or("");
        let loaded = catch_unwind(AssertUnwindSafe(|| Pdf::new_with_password(bytes, given)));
        match loaded {
            Ok(Ok(pdf)) => Ok(Self { pdf, bytes: len }),
            Ok(Err(LoadPdfError::Decryption(DecryptionError::PasswordProtected))) => {
                Err(if given.is_empty() { OpenError::PasswordRequired } else { OpenError::WrongPassword })
            }
            Ok(Err(LoadPdfError::Decryption(DecryptionError::UnsupportedAlgorithm | DecryptionError::InvalidEncryption))) => {
                Err(OpenError::UnsupportedEncryption)
            }
            Ok(Err(_)) => Err(OpenError::Invalid),
            Err(_) => Err(OpenError::Crashed),
        }
    }

    /// How many pages the file has.
    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// The size of page `index` (from 0), or `None` if there is no such page.
    pub fn page_size(&self, index: usize) -> Option<PageSize> {
        let page = self.pdf.pages().get(index)?;
        let (w, h) = page.render_dimensions();
        Some(PageSize::sane(w, h))
    }

    /// Size of the file as opened, in bytes.
    pub fn byte_len(&self) -> usize {
        self.bytes
    }

    /// The underlying reader, for the crates that draw pages.
    pub fn pdf(&self) -> &Pdf {
        &self.pdf
    }
}

fn check_len(len: usize) -> Result<(), OpenError> {
    match len {
        0 => Err(OpenError::Empty),
        n if n > MAX_FILE_BYTES => Err(OpenError::TooLarge { bytes: n }),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
