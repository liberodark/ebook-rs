//! JPEG XL decoding support using the official jxl-rs crate.
//!
//! This module provides JXL decoding functionality using the pure Rust
//! implementation from libjxl/jxl-rs.

use crate::error::{AppError, Result};

/// Check if data is a JPEG XL file by examining its signature.
pub fn is_jxl(data: &[u8]) -> bool {
    use jxl::api::{JxlSignatureType, ProcessingResult, check_signature};

    match check_signature(data) {
        ProcessingResult::Complete { result } => matches!(
            result,
            Some(JxlSignatureType::Codestream) | Some(JxlSignatureType::Container)
        ),
        ProcessingResult::NeedsMoreInput { .. } => false,
    }
}

/// Decode a JPEG XL image to RGBA8 pixel data.
///
/// Returns (width, height, rgba_data) on success.
pub fn decode_jxl(data: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    use jxl::api::{
        JxlColorType, JxlDataFormat, JxlDecoder, JxlDecoderOptions, JxlOutputBuffer,
        JxlPixelFormat, ProcessingResult,
    };
    use jxl::headers::extra_channels::ExtraChannel;
    use jxl::image::{OwnedRawImage, Rect};

    // Create decoder with default options
    let options = JxlDecoderOptions::default();
    let decoder = JxlDecoder::<jxl::api::states::Initialized>::new(options);

    // Process to get image info
    let mut input = data;
    let mut decoder = match decoder.process(&mut input) {
        Ok(ProcessingResult::Complete { result }) => result,
        Ok(ProcessingResult::NeedsMoreInput { .. }) => {
            return Err(AppError::InvalidFormat("Incomplete JXL data".into()));
        }
        Err(e) => {
            return Err(AppError::InvalidFormat(format!("JXL header error: {}", e)));
        }
    };

    // Get basic info
    let info = decoder.basic_info();
    let (width, height) = info.size;

    // Check for alpha channel
    let has_alpha = info
        .extra_channels
        .iter()
        .any(|ec| ec.ec_type == ExtraChannel::Alpha);

    // Set pixel format: RGBA or RGB, U8
    let color_type = if has_alpha {
        JxlColorType::Rgba
    } else {
        JxlColorType::Rgb
    };
    let samples_per_pixel = color_type.samples_per_pixel();

    let pixel_format = JxlPixelFormat {
        color_type,
        color_data_format: Some(JxlDataFormat::U8 { bit_depth: 8 }),
        extra_channel_format: vec![],
    };
    decoder.set_pixel_format(pixel_format);

    // Process to get frame info
    let decoder = match decoder.process(&mut input) {
        Ok(ProcessingResult::Complete { result }) => result,
        Ok(ProcessingResult::NeedsMoreInput { .. }) => {
            return Err(AppError::InvalidFormat("Incomplete JXL frame data".into()));
        }
        Err(e) => {
            return Err(AppError::InvalidFormat(format!("JXL frame error: {}", e)));
        }
    };

    // Prepare output buffer
    // For U8 output: bytes_per_row = width * samples_per_pixel
    let bytes_per_row = width * samples_per_pixel;
    let mut raw_image =
        OwnedRawImage::new_zeroed_with_padding((bytes_per_row, height), (0, 0), (0, 0))
            .map_err(|e| AppError::Internal(format!("Failed to create image buffer: {}", e)))?;

    let rect = Rect {
        origin: (0, 0),
        size: (bytes_per_row, height),
    };

    let mut buffers = vec![JxlOutputBuffer::from_image_rect_mut(
        raw_image.get_rect_mut(rect),
    )];

    // Decode the frame
    match decoder.process(&mut input, &mut buffers) {
        Ok(ProcessingResult::Complete { .. }) => {}
        Ok(ProcessingResult::NeedsMoreInput { .. }) => {
            return Err(AppError::InvalidFormat("Incomplete JXL pixel data".into()));
        }
        Err(e) => {
            return Err(AppError::InvalidFormat(format!("JXL decode error: {}", e)));
        }
    };

    // Copy data from raw_image to output
    let mut output_data = Vec::with_capacity(bytes_per_row * height);
    for y in 0..height {
        output_data.extend_from_slice(raw_image.row(y));
    }

    // If RGB (no alpha), convert to RGBA by adding alpha channel
    let rgba_data = if has_alpha {
        output_data
    } else {
        let mut rgba = Vec::with_capacity(width * height * 4);
        for chunk in output_data.chunks(3) {
            rgba.extend_from_slice(chunk);
            rgba.push(255); // Alpha = 255 (fully opaque)
        }
        rgba
    };

    Ok((width as u32, height as u32, rgba_data))
}

/// Decode a JPEG XL image directly to a DynamicImage.
pub fn decode_to_image(data: &[u8]) -> Result<image::DynamicImage> {
    let (width, height, rgba_data) = decode_jxl(data)?;

    image::RgbaImage::from_raw(width, height, rgba_data)
        .map(image::DynamicImage::ImageRgba8)
        .ok_or_else(|| AppError::Internal("Failed to create image from JXL data".into()))
}
