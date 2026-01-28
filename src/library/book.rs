//! Book metadata model.

use crate::config::BookFormat;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Represents a book or comic in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    /// Unique identifier for the book.
    pub id: String,

    /// Book title.
    pub title: String,

    /// Authors (may be empty).
    pub authors: Vec<String>,

    /// Book description or summary.
    pub description: Option<String>,

    /// Publisher name.
    pub publisher: Option<String>,

    /// Publication date.
    pub published: Option<String>,

    /// Language code (e.g., "en", "fr").
    pub language: Option<String>,

    /// ISBN or other identifier.
    pub isbn: Option<String>,

    /// Series name.
    pub series: Option<String>,

    /// Position in series.
    pub series_index: Option<f32>,

    /// Subject/genre tags.
    pub tags: Vec<String>,

    /// File format.
    pub format: BookFormat,

    /// Path to the book file.
    pub path: PathBuf,

    /// File size in bytes.
    pub file_size: u64,

    /// Last modified time.
    pub modified: DateTime<Utc>,

    /// Whether a cover image is available.
    pub has_cover: bool,

    /// Number of pages (if known).
    pub page_count: Option<u32>,
}

impl Book {
    /// Create a new book with minimal information.
    pub fn new(path: PathBuf, format: BookFormat) -> Self {
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        // Generate a deterministic UUID based on the file path
        let id = Uuid::new_v5(&Uuid::NAMESPACE_URL, path.to_string_lossy().as_bytes()).to_string();

        Self {
            id,
            title,
            authors: Vec::new(),
            description: None,
            publisher: None,
            published: None,
            language: None,
            isbn: None,
            series: None,
            series_index: None,
            tags: Vec::new(),
            format,
            path,
            file_size: 0,
            modified: Utc::now(),
            has_cover: false,
            page_count: None,
        }
    }

    /// Get the filename of the book.
    pub fn filename(&self) -> &str {
        self.path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
    }

    /// Get display name for authors.
    pub fn authors_display(&self) -> String {
        if self.authors.is_empty() {
            "Unknown Author".to_string()
        } else {
            self.authors.join(", ")
        }
    }

    /// Get the relative path within the library.
    pub fn relative_path(&self, library_root: &std::path::Path) -> Option<PathBuf> {
        self.path.strip_prefix(library_root).ok().map(PathBuf::from)
    }
}

impl Default for Book {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: "Unknown".to_string(),
            authors: Vec::new(),
            description: None,
            publisher: None,
            published: None,
            language: None,
            isbn: None,
            series: None,
            series_index: None,
            tags: Vec::new(),
            format: BookFormat::Epub,
            path: PathBuf::new(),
            file_size: 0,
            modified: Utc::now(),
            has_cover: false,
            page_count: None,
        }
    }
}

/// Represents a directory/category in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    /// Unique identifier.
    pub id: String,

    /// Display name.
    pub name: String,

    /// Path relative to library root.
    pub path: PathBuf,

    /// Number of books in this category (including subcategories).
    pub book_count: usize,

    /// Number of direct subcategories.
    pub subcategory_count: usize,
}

impl Category {
    /// Create a new category from a directory path.
    pub fn new(path: PathBuf, library_root: &std::path::Path) -> Self {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        let relative = path
            .strip_prefix(library_root)
            .unwrap_or(&path)
            .to_path_buf();

        let id =
            Uuid::new_v5(&Uuid::NAMESPACE_URL, relative.to_string_lossy().as_bytes()).to_string();

        Self {
            id,
            name,
            path: relative,
            book_count: 0,
            subcategory_count: 0,
        }
    }
}
