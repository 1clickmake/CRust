use crate::error::AppError;
use axum::http::StatusCode;
use bytes::Bytes;
use image::imageops::FilterType;
use image::ImageFormat;
use std::io::Cursor;

pub struct ProcessedImage {
    pub data: Bytes,
    pub ext: String,
    pub mime: String,
}

/// Resize image to fit within max_width and optionally convert to lightweight WebP.
/// Animated GIFs are kept as-is to preserve frames.
pub fn process_uploaded_image(
    data: Bytes,
    orig_ext: &str,
    orig_mime: Option<&str>,
    max_width: i64,
    convert_webp: bool,
) -> Result<ProcessedImage, AppError> {
    let lower_ext = orig_ext.to_ascii_lowercase();
    let default_mime = match lower_ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    };

    // Keep GIFs untouched so animations stay intact
    if lower_ext == "gif" {
        return Ok(ProcessedImage {
            data,
            ext: lower_ext,
            mime: orig_mime.unwrap_or(default_mime).to_string(),
        });
    }

    let need_resize = max_width > 0;
    if !need_resize && !convert_webp {
        return Ok(ProcessedImage {
            data,
            ext: lower_ext,
            mime: orig_mime.unwrap_or(default_mime).to_string(),
        });
    }

    let img = image::load_from_memory(&data).map_err(|e| {
        AppError::ApiError(
            StatusCode::BAD_REQUEST,
            format!("유효하지 않은 이미지 파일입니다: {}", e),
        )
    })?;

    let mut resized = false;
    let working = if max_width > 0 && img.width() > max_width as u32 {
        resized = true;
        img.resize(max_width as u32, u32::MAX, FilterType::Lanczos3)
    } else {
        img
    };

    // If neither resized nor changing format (e.g. already webp), keep original bytes
    if !resized && (!convert_webp || lower_ext == "webp") {
        return Ok(ProcessedImage {
            data,
            ext: lower_ext,
            mime: orig_mime.unwrap_or(default_mime).to_string(),
        });
    }

    let (target_format, target_ext, target_mime) = if convert_webp {
        (ImageFormat::WebP, "webp", "image/webp")
    } else {
        match lower_ext.as_str() {
            "png" => (ImageFormat::Png, "png", "image/png"),
            "webp" => (ImageFormat::WebP, "webp", "image/webp"),
            _ => (ImageFormat::Jpeg, "jpg", "image/jpeg"),
        }
    };

    let mut buf = Cursor::new(Vec::new());
    working.write_to(&mut buf, target_format).map_err(|e| {
        AppError::ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("이미지 변환 중 오류가 발생했습니다: {}", e),
        )
    })?;

    Ok(ProcessedImage {
        data: Bytes::from(buf.into_inner()),
        ext: target_ext.to_string(),
        mime: target_mime.to_string(),
    })
}
