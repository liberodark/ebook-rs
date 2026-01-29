//! PDF placeholder generation for CloudReader.
//!
//! Creates lightweight PDF files containing:
//! - Cover image (resized)
//! - Book metadata (title, author, description)
//! - CloudReader marker for identification

use crate::error::{AppError, Result};
use crate::library::book::Book;
use image::ImageReader;
use image::codecs::jpeg::JpegEncoder;
use lopdf::{Document, Object, Stream, dictionary};
use std::io::Cursor;

/// Default placeholder cover width in pixels.
pub const DEFAULT_WIDTH: u32 = 600;

/// Default JPEG quality (1-100).
pub const DEFAULT_QUALITY: u8 = 90;

/// Placeholder generation options.
#[derive(Debug, Clone)]
pub struct PlaceholderOptions {
    /// Cover image width in pixels.
    pub width: u32,
    /// JPEG quality (1-100).
    pub quality: u8,
}

impl Default for PlaceholderOptions {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            quality: DEFAULT_QUALITY,
        }
    }
}

/// Generate a PDF placeholder for a book.
pub fn generate_placeholder(
    book: &Book,
    cover_data: Option<&[u8]>,
    options: &PlaceholderOptions,
) -> Result<Vec<u8>> {
    // Load and resize cover image
    let (jpeg_data, img_width, img_height) = if let Some(data) = cover_data {
        resize_cover(data, options.width, options.quality)?
    } else {
        let default_cover = generate_default_cover(&book.title);
        resize_cover(&default_cover, options.width, options.quality)?
    };

    // Create PDF document
    let mut doc = Document::with_version("1.5");

    // Author
    let author = if book.authors.is_empty() {
        "Unknown".to_string()
    } else {
        book.authors.join(", ")
    };

    // Subject/Description (truncate if too long, respecting UTF-8 boundaries)
    let subject = book
        .description
        .as_ref()
        .map(|d| {
            if d.len() > 500 {
                // Find a valid UTF-8 boundary
                let mut end = 497;
                while end > 0 && !d.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}...", &d[..end])
            } else {
                d.clone()
            }
        })
        .unwrap_or_default();

    // Keywords including CloudReader marker
    let keywords = format!("cloudreader,placeholder,{}", book.id);

    // Add Info dictionary with metadata
    let info_id = doc.add_object(dictionary! {
        "Title" => Object::string_literal(book.title.clone()),
        "Author" => Object::string_literal(author),
        "Subject" => Object::string_literal(subject),
        "Keywords" => Object::string_literal(keywords),
        "Creator" => Object::string_literal("CloudReader Placeholder".to_string()),
        "Producer" => Object::string_literal("ebook-rs".to_string()),
    });

    // Set trailer Info reference
    doc.trailer.set("Info", Object::Reference(info_id));

    // Create image XObject
    let image_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => img_width as i64,
            "Height" => img_height as i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Filter" => "DCTDecode",
        },
        jpeg_data,
    );
    let image_id = doc.add_object(image_stream);

    // Page dimensions in points (72 DPI)
    let page_width = img_width as f32;
    let page_height = img_height as f32;

    // Create content stream to draw the image
    let content = format!("q {} 0 0 {} 0 0 cm /Im1 Do Q", page_width, page_height);
    let content_stream = Stream::new(dictionary! {}, content.into_bytes());
    let content_id = doc.add_object(content_stream);

    // Create Resources dictionary
    let resources_id = doc.add_object(dictionary! {
        "XObject" => dictionary! {
            "Im1" => Object::Reference(image_id),
        },
    });

    // Create page
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), page_width.into(), page_height.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Reference(resources_id),
    });

    // Create page tree
    let pages_id = doc.add_object(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1,
    });

    // Update page Parent reference
    if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(page_id) {
        dict.set("Parent", Object::Reference(pages_id));
    }

    // Create catalog
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    });

    // Set trailer Root
    doc.trailer.set("Root", Object::Reference(catalog_id));

    // Save to bytes
    let mut pdf_bytes = Vec::new();
    doc.save_to(&mut pdf_bytes)
        .map_err(|e| AppError::Internal(format!("Failed to save PDF: {}", e)))?;

    Ok(pdf_bytes)
}

/// Resize cover image to specified width, maintaining aspect ratio.
fn resize_cover(data: &[u8], target_width: u32, quality: u8) -> Result<(Vec<u8>, u32, u32)> {
    let img = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| AppError::Internal(format!("Failed to read image: {}", e)))?
        .decode()
        .map_err(|e| AppError::Internal(format!("Failed to decode image: {}", e)))?;

    let orig_width = img.width();
    let orig_height = img.height();

    // Calculate new dimensions
    let scale = target_width as f32 / orig_width as f32;
    let new_width = target_width;
    let new_height = (orig_height as f32 * scale) as u32;

    // Resize using Lanczos3 filter
    let resized = img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3);

    // Encode as JPEG
    let mut jpeg_data = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut jpeg_data, quality);
    encoder
        .encode_image(&resized)
        .map_err(|e| AppError::Internal(format!("Failed to encode JPEG: {}", e)))?;

    Ok((jpeg_data, new_width, new_height))
}

/// Generate a simple default cover image with the book title.
fn generate_default_cover(title: &str) -> Vec<u8> {
    use image::{Rgba, RgbaImage};

    let width = 400u32;
    let height = 600u32;

    // Generate a color based on title hash
    let hash = title
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_add(b as u32));
    let hue = (hash % 360) as f32;
    let (r, g, b) = hsv_to_rgb(hue, 0.4, 0.5);

    // Create image with gradient background
    let mut img = RgbaImage::new(width, height);

    for y in 0..height {
        let factor = y as f32 / height as f32;
        let r2 = (r as f32 * (1.0 - factor * 0.3)) as u8;
        let g2 = (g as f32 * (1.0 - factor * 0.3)) as u8;
        let b2 = (b as f32 * (1.0 - factor * 0.3)) as u8;
        for x in 0..width {
            img.put_pixel(x, y, Rgba([r2, g2, b2, 255]));
        }
    }

    // Encode as PNG
    let mut png_data = Vec::new();
    img.write_to(&mut Cursor::new(&mut png_data), image::ImageFormat::Png)
        .unwrap_or_default();

    png_data
}

/// Convert HSV to RGB.
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}
