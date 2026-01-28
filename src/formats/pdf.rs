use crate::error::{AppError, Result};
use crate::formats::FormatHandler;
use crate::library::book::Book;
use lopdf::Document;
use std::path::Path;

/// Handler for PDF files.
pub struct PdfHandler;

impl PdfHandler {
    /// Extract text content from a PDF info dictionary value.
    fn extract_text(obj: &lopdf::Object) -> Option<String> {
        match obj {
            lopdf::Object::String(bytes, _) => {
                // Try UTF-16BE first (starts with BOM)
                if bytes.starts_with(&[0xFE, 0xFF]) {
                    let utf16: Vec<u16> = bytes[2..]
                        .chunks(2)
                        .map(|chunk| {
                            u16::from_be_bytes([chunk[0], chunk.get(1).copied().unwrap_or(0)])
                        })
                        .collect();
                    String::from_utf16(&utf16).ok()
                } else {
                    // Try as UTF-8 or Latin-1
                    String::from_utf8(bytes.clone())
                        .or_else(|_| {
                            Ok::<_, std::string::FromUtf8Error>(
                                bytes.iter().map(|&b| b as char).collect(),
                            )
                        })
                        .ok()
                }
            }
            lopdf::Object::Name(name) => String::from_utf8(name.clone()).ok(),
            _ => None,
        }
    }
}

impl FormatHandler for PdfHandler {
    fn extract_metadata(&self, book: &mut Book) -> Result<()> {
        let doc = Document::load(&book.path).map_err(|e| AppError::Pdf(e.to_string()))?;

        // Get page count
        book.page_count = Some(doc.get_pages().len() as u32);

        // Try to get document info
        if let Ok(info_dict) = doc.trailer.get(b"Info")
            && let Ok(info_ref) = info_dict.as_reference()
            && let Ok(info) = doc.get_dictionary(info_ref)
        {
            // Title
            if let Ok(title) = info.get(b"Title")
                && let Some(text) = Self::extract_text(title)
            {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    book.title = trimmed.to_string();
                }
            }

            // Author
            if let Ok(author) = info.get(b"Author")
                && let Some(text) = Self::extract_text(author)
            {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    book.authors = vec![trimmed.to_string()];
                }
            }

            // Subject (used as description)
            if let Ok(subject) = info.get(b"Subject")
                && let Some(text) = Self::extract_text(subject)
            {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    book.description = Some(trimmed.to_string());
                }
            }

            // Keywords (used as tags)
            if let Ok(keywords) = info.get(b"Keywords")
                && let Some(text) = Self::extract_text(keywords)
            {
                book.tags = text
                    .split([',', ';'])
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }

            // Producer/Creator as publisher fallback
            if let Ok(producer) = info.get(b"Producer")
                && let Some(text) = Self::extract_text(producer)
            {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    book.publisher = Some(trimmed.to_string());
                }
            }
        }

        // PDF might have a cover, but extraction is complex
        // For now, mark as having cover if it has pages
        book.has_cover = book.page_count.map(|c| c > 0).unwrap_or(false);

        Ok(())
    }

    fn extract_cover(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        let doc = Document::load(path).map_err(|e| AppError::Pdf(e.to_string()))?;

        // Get first page - get_pages() returns BTreeMap<u32, ObjectId>
        let pages = doc.get_pages();
        let Some(&first_page_id) = pages.values().next() else {
            return Ok(None);
        };

        // Get page dictionary
        let Ok(page) = doc.get_dictionary(first_page_id) else {
            return Ok(None);
        };

        // Get Resources - handle both direct dict and reference
        let resources = match page.get(b"Resources") {
            Ok(lopdf::Object::Reference(r)) => doc.get_dictionary(*r).ok(),
            Ok(lopdf::Object::Dictionary(d)) => Some(d),
            _ => None,
        };

        let Some(resources) = resources else {
            return Ok(None);
        };

        // Get XObject dictionary
        let xobjects = match resources.get(b"XObject") {
            Ok(lopdf::Object::Reference(r)) => doc.get_dictionary(*r).ok(),
            Ok(lopdf::Object::Dictionary(d)) => Some(d),
            _ => None,
        };

        let Some(xobjects) = xobjects else {
            return Ok(None);
        };

        // Find first image XObject
        for (_name, obj) in xobjects.iter() {
            let lopdf::Object::Reference(xobj_ref) = obj else {
                continue;
            };

            let Ok(lopdf::Object::Stream(xobj_stream)) = doc.get_object(*xobj_ref) else {
                continue;
            };

            // Check if it's an image
            let is_image = matches!(
                xobj_stream.dict.get(b"Subtype"),
                Ok(lopdf::Object::Name(n)) if n == b"Image"
            );

            if !is_image {
                continue;
            }

            // Check filter - we support DCTDecode (JPEG)
            let is_dct = match xobj_stream.dict.get(b"Filter") {
                Ok(lopdf::Object::Name(n)) => n == b"DCTDecode",
                Ok(lopdf::Object::Array(arr)) => arr
                    .iter()
                    .any(|item| matches!(item, lopdf::Object::Name(n) if n == b"DCTDecode")),
                _ => false,
            };

            if is_dct {
                // For DCTDecode, try content first, then decompressed_content
                let data = if !xobj_stream.content.is_empty() {
                    xobj_stream.content.clone()
                } else if let Ok(decoded) = xobj_stream.decompressed_content() {
                    decoded
                } else {
                    continue;
                };

                // Verify it's JPEG
                if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
                    return Ok(Some(data));
                }
            }

            // For other filters, try to decode
            if let Ok(data) = xobj_stream.decompressed_content() {
                // Check if it looks like JPEG
                if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
                    return Ok(Some(data));
                }

                // Check if it looks like PNG
                if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                    return Ok(Some(data));
                }

                // Raw image data - try to convert to PNG
                let width = match xobj_stream.dict.get(b"Width") {
                    Ok(lopdf::Object::Integer(i)) => Some(*i as u32),
                    _ => None,
                };
                let height = match xobj_stream.dict.get(b"Height") {
                    Ok(lopdf::Object::Integer(i)) => Some(*i as u32),
                    _ => None,
                };

                if let (Some(w), Some(h)) = (width, height) {
                    // Try to create image from raw RGB data
                    if let Some(img) = image::RgbImage::from_raw(w, h, data.clone()) {
                        let mut png_data = Vec::new();
                        if image::DynamicImage::ImageRgb8(img)
                            .write_to(
                                &mut std::io::Cursor::new(&mut png_data),
                                image::ImageFormat::Png,
                            )
                            .is_ok()
                        {
                            return Ok(Some(png_data));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    fn page_count(&self, path: &Path) -> Result<Option<u32>> {
        let doc = Document::load(path).map_err(|e| AppError::Pdf(e.to_string()))?;

        Ok(Some(doc.get_pages().len() as u32))
    }
}
