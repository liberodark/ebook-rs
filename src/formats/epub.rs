//! EPUB format handler.

use crate::error::{AppError, Result};
use crate::formats::FormatHandler;
use crate::library::book::Book;
use roxmltree::Document;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

/// Handler for EPUB files.
pub struct EpubHandler;

impl EpubHandler {
    /// Find the OPF file path from container.xml.
    fn find_opf_path(archive: &mut ZipArchive<File>) -> Result<String> {
        let mut container = archive.by_name("META-INF/container.xml")?;
        let mut content = String::new();
        container.read_to_string(&mut content)?;

        let doc = Document::parse(&content)?;

        doc.descendants()
            .find(|n| n.has_tag_name("rootfile"))
            .and_then(|n| n.attribute("full-path"))
            .map(String::from)
            .ok_or_else(|| AppError::InvalidFormat("No rootfile in container.xml".into()))
    }

    /// Parse the OPF file and extract metadata.
    fn parse_opf(content: &str, book: &mut Book) -> Result<Option<String>> {
        let doc = Document::parse(content)?;
        let mut cover_id: Option<String> = None;

        // Find metadata elements
        for node in doc.descendants() {
            match node.tag_name().name() {
                "title" => {
                    if let Some(text) = node.text() {
                        book.title = text.trim().to_string();
                    }
                }
                "creator" => {
                    if let Some(text) = node.text() {
                        book.authors.push(text.trim().to_string());
                    }
                }
                "description" => {
                    if let Some(text) = node.text() {
                        book.description = Some(text.trim().to_string());
                    }
                }
                "publisher" => {
                    if let Some(text) = node.text() {
                        book.publisher = Some(text.trim().to_string());
                    }
                }
                "language" => {
                    if let Some(text) = node.text() {
                        book.language = Some(text.trim().to_string());
                    }
                }
                "date" => {
                    if let Some(text) = node.text() {
                        book.published = Some(text.trim().to_string());
                    }
                }
                "subject" => {
                    if let Some(text) = node.text() {
                        book.tags.push(text.trim().to_string());
                    }
                }
                "identifier" => {
                    if let Some(text) = node.text() {
                        // Check if it looks like an ISBN
                        let trimmed = text.trim();
                        if trimmed.starts_with("978")
                            || trimmed.starts_with("979")
                            || trimmed.len() == 10
                            || trimmed.len() == 13
                        {
                            book.isbn = Some(trimmed.to_string());
                        }
                    }
                }
                "meta" => {
                    // Look for cover meta tag
                    if node.attribute("name") == Some("cover") {
                        cover_id = node.attribute("content").map(String::from);
                    }
                    // Look for series metadata (calibre format)
                    if node.attribute("name") == Some("calibre:series") {
                        book.series = node.attribute("content").map(String::from);
                    }
                    if node.attribute("name") == Some("calibre:series_index")
                        && let Some(idx) = node.attribute("content")
                    {
                        book.series_index = idx.parse().ok();
                    }
                }
                _ => {}
            }
        }

        // Find cover href from manifest
        if let Some(ref cover_id) = cover_id {
            for node in doc.descendants() {
                if node.tag_name().name() == "item" && node.attribute("id") == Some(cover_id) {
                    return Ok(node.attribute("href").map(String::from));
                }
            }
        }

        // Fallback: look for common cover image names in manifest
        for node in doc.descendants() {
            if node.tag_name().name() == "item"
                && let Some(href) = node.attribute("href")
            {
                let lower = href.to_lowercase();
                if lower.contains("cover")
                    && (lower.ends_with(".jpg")
                        || lower.ends_with(".jpeg")
                        || lower.ends_with(".png"))
                {
                    return Ok(Some(href.to_string()));
                }
            }
        }

        Ok(None)
    }

    /// Extract cover image from EPUB.
    fn extract_cover_from_archive(
        archive: &mut ZipArchive<File>,
        opf_dir: &str,
        cover_href: &str,
    ) -> Result<Vec<u8>> {
        // Build full path to cover
        let cover_path = if opf_dir.is_empty() {
            cover_href.to_string()
        } else {
            format!("{}/{}", opf_dir.trim_end_matches('/'), cover_href)
        };

        // Determine which path to use
        let file_names: Vec<String> = archive.file_names().map(String::from).collect();
        let actual_path = if file_names.iter().any(|n| n == &cover_path) {
            cover_path
        } else {
            cover_href.to_string()
        };

        let mut data = Vec::new();
        let mut file = archive.by_name(&actual_path)?;
        file.read_to_end(&mut data)?;

        // Convert to PNG if needed
        Self::ensure_png(data)
    }

    /// Ensure image data is PNG format.
    fn ensure_png(data: Vec<u8>) -> Result<Vec<u8>> {
        // Check if it's already PNG
        if data.starts_with(&[0x89, b'P', b'N', b'G']) {
            return Ok(data);
        }

        // Try to decode and re-encode as PNG
        let img = image::load_from_memory(&data)?;
        let mut png_data = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png_data),
            image::ImageFormat::Png,
        )?;

        Ok(png_data)
    }
}

impl FormatHandler for EpubHandler {
    fn extract_metadata(&self, book: &mut Book) -> Result<()> {
        let file = File::open(&book.path)?;
        let mut archive = ZipArchive::new(file)?;

        let opf_path = Self::find_opf_path(&mut archive)?;

        let mut opf_content = String::new();
        archive
            .by_name(&opf_path)?
            .read_to_string(&mut opf_content)?;

        let cover_href = Self::parse_opf(&opf_content, book)?;
        book.has_cover = cover_href.is_some();

        Ok(())
    }

    fn extract_cover(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        let file = File::open(path)?;
        let mut archive = ZipArchive::new(file)?;

        let opf_path = Self::find_opf_path(&mut archive)?;
        let opf_dir = opf_path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");

        let mut opf_content = String::new();
        {
            let mut opf_file = archive.by_name(&opf_path)?;
            opf_file.read_to_string(&mut opf_content)?;
        }

        // Create a temporary book to parse metadata
        let mut temp_book = Book::new(path.to_path_buf(), crate::config::BookFormat::Epub);
        let cover_href = Self::parse_opf(&opf_content, &mut temp_book)?;

        if let Some(href) = cover_href {
            let cover_data = Self::extract_cover_from_archive(&mut archive, opf_dir, &href)?;
            Ok(Some(cover_data))
        } else {
            Ok(None)
        }
    }

    fn page_count(&self, _path: &Path) -> Result<Option<u32>> {
        // EPUB doesn't have fixed pages
        Ok(None)
    }
}
