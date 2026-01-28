//! CBZ (Comic Book ZIP) format handler.

use crate::error::Result;
use crate::formats::FormatHandler;
use crate::formats::jxl as jxl_decoder;
use crate::library::book::Book;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

/// Handler for CBZ files (and similar comic book archives).
pub struct CbzHandler;

impl CbzHandler {
    /// Check if a filename is an image.
    fn is_image_file(name: &str) -> bool {
        let lower = name.to_lowercase();
        lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".png")
            || lower.ends_with(".gif")
            || lower.ends_with(".webp")
            || lower.ends_with(".jxl")
    }

    /// Get sorted list of image files in archive.
    fn get_image_files(archive: &ZipArchive<File>) -> Vec<String> {
        let mut images: Vec<String> = archive
            .file_names()
            .filter(|name| Self::is_image_file(name))
            .filter(|name| !name.contains("__MACOSX")) // Skip macOS metadata
            .map(String::from)
            .collect();

        // Sort naturally (so page2 comes before page10)
        images.sort_by(|a, b| natord_compare(a, b));

        images
    }

    /// Convert image data to PNG, with JXL support.
    fn to_png(data: &[u8]) -> Result<Vec<u8>> {
        let img = if jxl_decoder::is_jxl(data) {
            jxl_decoder::decode_to_image(data)?
        } else {
            image::load_from_memory(data)?
        };

        let mut png_data = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png_data),
            image::ImageFormat::Png,
        )?;

        Ok(png_data)
    }
}

impl FormatHandler for CbzHandler {
    fn extract_metadata(&self, book: &mut Book) -> Result<()> {
        let file = File::open(&book.path)?;
        let archive = ZipArchive::new(file)?;

        let images = Self::get_image_files(&archive);
        book.page_count = Some(images.len() as u32);
        book.has_cover = !images.is_empty();

        // Try to parse series info from filename
        // Common patterns: "Series Name v01", "Series Name #01", "Series Name - 01"
        let filename = book.title.clone();
        if let Some((series, index)) = parse_comic_filename(&filename) {
            book.series = Some(series);
            book.series_index = Some(index);
        }

        Ok(())
    }

    fn extract_cover(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        let file = File::open(path)?;
        let mut archive = ZipArchive::new(file)?;

        let images = Self::get_image_files(&archive);
        let first_image = match images.first() {
            Some(name) => name,
            None => return Ok(None),
        };

        let mut data = Vec::new();
        archive.by_name(first_image)?.read_to_end(&mut data)?;

        let png_data = Self::to_png(&data)?;
        Ok(Some(png_data))
    }

    fn page_count(&self, path: &Path) -> Result<Option<u32>> {
        let file = File::open(path)?;
        let archive = ZipArchive::new(file)?;

        let count = Self::get_image_files(&archive).len();
        Ok(Some(count as u32))
    }
}

/// Natural string comparison for sorting.
fn natord_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let mut a_chars = a.chars().peekable();
    let mut b_chars = b.chars().peekable();

    loop {
        match (a_chars.peek(), b_chars.peek()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(&ac), Some(&bc)) => {
                if ac.is_ascii_digit() && bc.is_ascii_digit() {
                    // Compare numbers
                    let a_num: String = a_chars
                        .by_ref()
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    let b_num: String = b_chars
                        .by_ref()
                        .take_while(|c| c.is_ascii_digit())
                        .collect();

                    let a_val: u64 = a_num.parse().unwrap_or(0);
                    let b_val: u64 = b_num.parse().unwrap_or(0);

                    match a_val.cmp(&b_val) {
                        std::cmp::Ordering::Equal => continue,
                        other => return other,
                    }
                } else {
                    // Compare characters
                    a_chars.next();
                    b_chars.next();

                    match ac.to_lowercase().cmp(bc.to_lowercase()) {
                        std::cmp::Ordering::Equal => continue,
                        other => return other,
                    }
                }
            }
        }
    }
}

/// Parse comic book filename to extract series and volume/issue number.
fn parse_comic_filename(filename: &str) -> Option<(String, f32)> {
    // Patterns to try:
    // "Series Name v01" or "Series Name Vol. 01"
    // "Series Name #01" or "Series Name - 01"
    // "Series Name 01"

    let patterns = [
        (r"(.+?)\s*[vV](?:ol\.?\s*)?(\d+(?:\.\d+)?)", true),
        (r"(.+?)\s*#(\d+(?:\.\d+)?)", true),
        (r"(.+?)\s*-\s*(\d+(?:\.\d+)?)", true),
        (r"(.+?)\s+(\d+(?:\.\d+)?)$", true),
    ];

    for (_pattern, _) in patterns {
        // Simple pattern matching without regex crate
        // This is a simplified version - you might want to add regex crate for robust parsing
        if let Some(idx) = filename.rfind(['v', 'V', '#', '-']) {
            let (series_part, num_part) = filename.split_at(idx);
            let series = series_part.trim().trim_end_matches(['-', '#', ' ']).trim();

            let num_str: String = num_part
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '.')
                .collect();

            if let Ok(num) = num_str.parse::<f32>()
                && !series.is_empty()
            {
                return Some((series.to_string(), num));
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_natord_compare() {
        assert_eq!(natord_compare("page1", "page2"), std::cmp::Ordering::Less);
        assert_eq!(natord_compare("page2", "page10"), std::cmp::Ordering::Less);
        assert_eq!(
            natord_compare("page10", "page2"),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn test_parse_comic_filename() {
        assert_eq!(
            parse_comic_filename("One Piece v01"),
            Some(("One Piece".to_string(), 1.0))
        );
        assert_eq!(
            parse_comic_filename("Spider-Man #123"),
            Some(("Spider-Man".to_string(), 123.0))
        );
    }
}
