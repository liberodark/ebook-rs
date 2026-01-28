mod cbz;
mod epub;
pub mod jxl;
mod pdf;
pub mod placeholder;

pub use cbz::CbzHandler;
pub use epub::EpubHandler;
pub use pdf::PdfHandler;

use crate::error::Result;
use crate::library::book::Book;
use std::path::Path;

/// Trait for format-specific book handlers.
pub trait FormatHandler: Send + Sync {
    /// Extract metadata from a book file.
    fn extract_metadata(&self, book: &mut Book) -> Result<()>;

    /// Extract cover image as PNG bytes.
    fn extract_cover(&self, path: &Path) -> Result<Option<Vec<u8>>>;

    /// Get the number of pages (if applicable).
    fn page_count(&self, path: &Path) -> Result<Option<u32>>;
}

/// Get the appropriate handler for a book format.
pub fn get_handler(format: crate::config::BookFormat) -> Box<dyn FormatHandler> {
    use crate::config::BookFormat;

    match format {
        BookFormat::Epub => Box::new(EpubHandler),
        BookFormat::Pdf => Box::new(PdfHandler),
        BookFormat::Cbz => Box::new(CbzHandler),
        // Fallback to CBZ handler for other comic formats (they're similar)
        BookFormat::Cbr | BookFormat::Cb7 => Box::new(CbzHandler),
        // For text formats, use a minimal handler
        _ => Box::new(MinimalHandler),
    }
}

/// Minimal handler for formats without special metadata.
struct MinimalHandler;

impl FormatHandler for MinimalHandler {
    fn extract_metadata(&self, _book: &mut Book) -> Result<()> {
        Ok(())
    }

    fn extract_cover(&self, _path: &Path) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }

    fn page_count(&self, _path: &Path) -> Result<Option<u32>> {
        Ok(None)
    }
}
