//! HTTP request handlers.

use crate::db::{self, Bookmark, Highlight, ReadingProgress};
use crate::error::{AppError, Result};
use crate::formats;
use crate::opds::{self, FeedBuilder, Link};
use crate::server::AppState;
use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tokio_util::io::ReaderStream;

/// OPDS content type.
const OPDS_MIME: &str = "application/atom+xml;profile=opds-catalog";

/// Build a response, returning 500 on error (which shouldn't happen).
fn build_response(status: StatusCode, content_type: &str, body: impl Into<Body>) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .body(body.into())
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Internal error"))
                .unwrap_or_default()
        })
}

// ============================================================================
// WEB PAGES
// ============================================================================

/// Index page (simple HTML).
pub async fn index(State(state): State<AppState>) -> Html<String> {
    let book_count = state.book_count();
    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{title}</title>
    <style>
        body {{ font-family: system-ui, sans-serif; max-width: 600px; margin: 2rem auto; padding: 0 1rem; }}
        h1 {{ color: #333; }}
        a {{ color: #0066cc; }}
        .stats {{ background: #f5f5f5; padding: 1rem; border-radius: 8px; margin: 1rem 0; }}
        code {{ background: #e8e8e8; padding: 0.2rem 0.4rem; border-radius: 4px; }}
    </style>
</head>
<body>
    <h1>📚 {title}</h1>
    <div class="stats">
        <p><strong>{book_count}</strong> books in library</p>
    </div>
    <h2>OPDS Catalog</h2>
    <p>Add this URL to your e-reader's OPDS catalog:</p>
    <p><code>/catalog</code></p>
    <h2>Links</h2>
    <ul>
        <li><a href="/catalog">OPDS Catalog (XML)</a></li>
        <li><a href="/opensearch.xml">OpenSearch Description</a></li>
        <li><a href="/api/stats">API Stats (JSON)</a></li>
    </ul>
</body>
</html>"#,
        title = state.config.server.title,
        book_count = book_count,
    );

    Html(html)
}

/// OpenSearch description.
pub async fn opensearch(State(state): State<AppState>) -> impl IntoResponse {
    let xml = opds::generate_opensearch(&state.config.server.title, &state.base_url());
    build_response(StatusCode::OK, "application/opensearchdescription+xml", xml)
}

// ============================================================================
// OPDS CATALOG
// ============================================================================

/// Catalog root feed.
pub async fn catalog_root(State(state): State<AppState>) -> impl IntoResponse {
    let base_url = state.base_url();

    let mut feed = FeedBuilder::new(
        format!("urn:uuid:{}", uuid::Uuid::new_v4()),
        &state.config.server.title,
    )
    .author("ebook-rs")
    .self_link(format!("{}/catalog", base_url))
    .start_link(format!("{}/catalog", base_url))
    .search_link(format!("{}/opensearch.xml", base_url));

    // Add navigation entries
    feed = feed.navigation_entry(opds::Entry {
        id: "urn:uuid:recent".to_string(),
        title: "Recent Books".to_string(),
        updated: chrono::Utc::now(),
        authors: Vec::new(),
        summary: Some("Recently added books".to_string()),
        content: None,
        links: vec![Link {
            rel: "subsection".to_string(),
            href: format!("{}/catalog/recent", base_url),
            link_type: "application/atom+xml;profile=opds-catalog;kind=acquisition".to_string(),
            title: Some("Recent Books".to_string()),
        }],
        categories: Vec::new(),
    });

    feed = feed.navigation_entry(opds::Entry {
        id: "urn:uuid:all".to_string(),
        title: "All Books".to_string(),
        updated: chrono::Utc::now(),
        authors: Vec::new(),
        summary: Some(format!("{} books total", state.book_count())),
        content: None,
        links: vec![Link {
            rel: "subsection".to_string(),
            href: format!("{}/catalog/all", base_url),
            link_type: "application/atom+xml;profile=opds-catalog;kind=acquisition".to_string(),
            title: Some("All Books".to_string()),
        }],
        categories: Vec::new(),
    });

    build_response(StatusCode::OK, OPDS_MIME, feed.build())
}

/// Recent books feed.
pub async fn catalog_recent(State(state): State<AppState>) -> impl IntoResponse {
    let base_url = state.base_url();
    let books = state.get_recent(50);

    let mut feed = FeedBuilder::new("urn:uuid:recent", "Recent Books")
        .self_link(format!("{}/catalog/recent", base_url))
        .start_link(format!("{}/catalog", base_url));

    for book in books {
        feed = feed.book_entry(&book, &base_url);
    }

    build_response(StatusCode::OK, OPDS_MIME, feed.build())
}

/// All books feed.
pub async fn catalog_all(State(state): State<AppState>) -> impl IntoResponse {
    let base_url = state.base_url();
    let mut books = state.get_all_books();
    books.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));

    let mut feed = FeedBuilder::new("urn:uuid:all", "All Books")
        .self_link(format!("{}/catalog/all", base_url))
        .start_link(format!("{}/catalog", base_url));

    for book in books {
        feed = feed.book_entry(&book, &base_url);
    }

    build_response(StatusCode::OK, OPDS_MIME, feed.build())
}

/// Search query parameters.
#[derive(Debug, Deserialize)]
pub struct SearchParams {
    q: String,
}

/// Search feed.
pub async fn catalog_search(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> impl IntoResponse {
    let base_url = state.base_url();
    let books = state.search(&params.q);

    let mut feed = FeedBuilder::new(
        format!("urn:uuid:search:{}", params.q),
        format!("Search: {}", params.q),
    )
    .self_link(format!(
        "{}/catalog/search?q={}",
        base_url,
        urlencoding::encode(&params.q)
    ))
    .start_link(format!("{}/catalog", base_url));

    for book in books {
        feed = feed.book_entry(&book, &base_url);
    }

    build_response(StatusCode::OK, OPDS_MIME, feed.build())
}

// ============================================================================
// BOOK HANDLERS
// ============================================================================

/// Book metadata (JSON).
pub async fn book_metadata(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crate::library::book::Book>> {
    let book = state
        .get_book(&id)
        .ok_or_else(|| AppError::NotFound(format!("Book not found: {}", id)))?;

    Ok(Json(book))
}

/// Book download.
pub async fn book_download(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response<Body>> {
    book_download_impl(state, id).await
}

/// Book download with extension (e.g., /download.pdf).
pub async fn book_download_with_ext(
    State(state): State<AppState>,
    Path((id, _ext)): Path<(String, String)>,
) -> Result<Response<Body>> {
    book_download_impl(state, id).await
}

/// Internal book download implementation.
async fn book_download_impl(state: AppState, id: String) -> Result<Response<Body>> {
    let book = state
        .get_book(&id)
        .ok_or_else(|| AppError::NotFound(format!("Book not found: {}", id)))?;

    let file = tokio::fs::File::open(&book.path).await?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let filename = book.filename();
    let content_disposition = format!("attachment; filename=\"{}\"", filename);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, book.format.mime_type())
        .header(header::CONTENT_DISPOSITION, content_disposition)
        .header(header::CONTENT_LENGTH, book.file_size)
        .body(body)
        .unwrap_or_else(|_| Response::default()))
}

/// Book cover image.
pub async fn book_cover(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response<Body>> {
    let book = state
        .get_book(&id)
        .ok_or_else(|| AppError::NotFound(format!("Book not found: {}", id)))?;

    let cover_data = state
        .get_cover(&book)
        .unwrap_or_else(|| generate_default_cover(&book.title));

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/jpeg")
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(cover_data))
        .unwrap_or_else(|_| Response::default()))
}

/// Book thumbnail image.
pub async fn book_thumbnail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response<Body>> {
    let book = state
        .get_book(&id)
        .ok_or_else(|| AppError::NotFound(format!("Book not found: {}", id)))?;

    let cover_data = state
        .get_cover(&book)
        .unwrap_or_else(|| generate_default_cover(&book.title));

    let img = image::load_from_memory(&cover_data)?;
    let thumb = img.thumbnail(
        state.config.cache.thumbnail_size,
        state.config.cache.thumbnail_size * 2,
    );

    let mut thumb_data = Vec::new();
    thumb.write_to(
        &mut std::io::Cursor::new(&mut thumb_data),
        image::ImageFormat::Png,
    )?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(thumb_data))
        .unwrap_or_else(|_| Response::default()))
}

/// Query parameters for placeholder generation.
#[derive(Debug, Deserialize)]
pub struct PlaceholderQuery {
    /// Cover width in pixels (default: 600).
    #[serde(default = "default_placeholder_width")]
    pub width: u32,
    /// JPEG quality 1-100 (default: 90).
    #[serde(default = "default_placeholder_quality")]
    pub quality: u8,
}

fn default_placeholder_width() -> u32 {
    600
}

fn default_placeholder_quality() -> u8 {
    90
}

/// Book placeholder PDF for CloudReader sync.
///
/// Returns a lightweight PDF with cover + metadata.
/// Query params: ?width=600&quality=90
pub async fn book_placeholder(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<PlaceholderQuery>,
) -> Result<Response<Body>> {
    let book = state
        .get_book(&id)
        .ok_or_else(|| AppError::NotFound(format!("Book not found: {}", id)))?;

    // Get cover image from cache
    let cover_data = state.get_cover(&book);

    // Generate placeholder PDF
    let options = formats::placeholder::PlaceholderOptions {
        width: params.width.clamp(200, 1200),
        quality: params.quality.clamp(50, 100),
    };

    let pdf_data =
        formats::placeholder::generate_placeholder(&book, cover_data.as_deref(), &options)?;

    let filename = book
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("placeholder.pdf");

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/pdf")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .header(header::CONTENT_LENGTH, pdf_data.len())
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(pdf_data))
        .unwrap_or_else(|_| Response::default()))
}

/// Generate a simple default cover image with the book title.
fn generate_default_cover(title: &str) -> Vec<u8> {
    use image::{Rgba, RgbaImage};

    let width = 300u32;
    let height = 400u32;

    // Generate a color based on title hash for variety
    let hash = title
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_add(b as u32));
    let hue = (hash % 360) as f32;
    let (r, g, b) = hsv_to_rgb(hue, 0.3, 0.4);

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

    // Add a border
    let border_color = Rgba([255, 255, 255, 60]);
    for x in 0..width {
        img.put_pixel(x, 0, border_color);
        img.put_pixel(x, height - 1, border_color);
    }
    for y in 0..height {
        img.put_pixel(0, y, border_color);
        img.put_pixel(width - 1, y, border_color);
    }

    // Encode to PNG
    let mut png_data = Vec::new();
    let _ = image::DynamicImage::ImageRgba8(img).write_to(
        &mut std::io::Cursor::new(&mut png_data),
        image::ImageFormat::Png,
    );

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

// ============================================================================
// AUTH API
// ============================================================================

/// Login request.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
    device_id: Option<String>,
}

/// Login response.
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    token: String,
    user_id: String,
    username: String,
    role: String,
}

/// Register request.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    username: String,
    password: String,
}

/// Auth login.
pub async fn auth_login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>> {
    let (user, token) = state
        .auth
        .login(&req.username, &req.password, req.device_id)?;

    Ok(Json(LoginResponse {
        token,
        user_id: user.id,
        username: user.username,
        role: user.role,
    }))
}

/// Auth register.
pub async fn auth_register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<LoginResponse>> {
    let _user = state.auth.register(&req.username, &req.password)?;
    let (user, token) = state.auth.login(&req.username, &req.password, None)?;

    Ok(Json(LoginResponse {
        token,
        user_id: user.id,
        username: user.username,
        role: user.role,
    }))
}

/// Auth logout.
pub async fn auth_logout(State(state): State<AppState>, headers: HeaderMap) -> Result<StatusCode> {
    if let Some(token) = extract_token(&headers) {
        state.auth.logout(&token)?;
    }
    Ok(StatusCode::OK)
}

/// Get current user info.
pub async fn auth_me(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<db::User>> {
    let user = get_authenticated_user(&state, &headers).await?;
    Ok(Json(user))
}

// ============================================================================
// SYNC API
// ============================================================================

/// Progress update request.
#[derive(Debug, Deserialize)]
pub struct ProgressUpdateRequest {
    device_id: Option<String>,
    current_page: Option<i64>,
    total_pages: Option<i64>,
    percentage: Option<f64>,
    current_chapter: Option<String>,
    position_data: Option<String>,
    status: Option<String>,
}

/// Get reading progress.
pub async fn sync_get_progress(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Result<Json<Option<ReadingProgress>>> {
    let user = get_authenticated_user(&state, &headers).await?;
    let progress = state.db.get_progress(&user.id, &book_id)?;
    Ok(Json(progress))
}

/// Update reading progress.
pub async fn sync_update_progress(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    Json(req): Json<ProgressUpdateRequest>,
) -> Result<StatusCode> {
    let user = get_authenticated_user(&state, &headers).await?;

    let progress = ReadingProgress {
        id: 0, // Auto-increment
        user_id: user.id,
        book_id,
        device_id: req.device_id,
        current_page: req.current_page,
        total_pages: req.total_pages,
        percentage: req.percentage,
        current_chapter: req.current_chapter,
        position_data: req.position_data,
        status: req.status.unwrap_or_else(|| "reading".to_string()),
        started_at: Some(db::now_timestamp()),
        finished_at: None,
        updated_at: db::now_timestamp(),
    };

    state.db.save_progress(&progress)?;
    Ok(StatusCode::OK)
}

/// Highlight request.
#[derive(Debug, Deserialize)]
pub struct HighlightRequest {
    id: Option<String>,
    device_id: Option<String>,
    page: Option<i64>,
    chapter: Option<String>,
    text: String,
    note: Option<String>,
    color: Option<String>,
    pos0: Option<String>,
    pos1: Option<String>,
}

/// Get highlights for a book.
pub async fn sync_get_highlights(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Result<Json<Vec<Highlight>>> {
    let user = get_authenticated_user(&state, &headers).await?;
    let highlights = state.db.get_highlights(&user.id, &book_id)?;
    Ok(Json(highlights))
}

/// Add a highlight.
pub async fn sync_add_highlight(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    Json(req): Json<HighlightRequest>,
) -> Result<Json<Highlight>> {
    let user = get_authenticated_user(&state, &headers).await?;

    let highlight = Highlight {
        id: req.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        user_id: user.id,
        book_id,
        device_id: req.device_id,
        page: req.page,
        chapter: req.chapter,
        text: req.text,
        note: req.note,
        color: req.color.unwrap_or_else(|| "yellow".to_string()),
        pos0: req.pos0,
        pos1: req.pos1,
        created_at: db::now_timestamp(),
        updated_at: db::now_timestamp(),
    };

    state.db.save_highlight(&highlight)?;
    Ok(Json(highlight))
}

/// Delete a highlight.
pub async fn sync_delete_highlight(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    let user = get_authenticated_user(&state, &headers).await?;
    state.db.delete_highlight(&id, &user.id)?;
    Ok(StatusCode::OK)
}

/// Bookmark request.
#[derive(Debug, Deserialize)]
pub struct BookmarkRequest {
    id: Option<String>,
    page: Option<i64>,
    position_data: Option<String>,
    name: Option<String>,
}

/// Get bookmarks for a book.
pub async fn sync_get_bookmarks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Result<Json<Vec<Bookmark>>> {
    let user = get_authenticated_user(&state, &headers).await?;
    let bookmarks = state.db.get_bookmarks(&user.id, &book_id)?;
    Ok(Json(bookmarks))
}

/// Add a bookmark.
pub async fn sync_add_bookmark(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    Json(req): Json<BookmarkRequest>,
) -> Result<Json<Bookmark>> {
    let user = get_authenticated_user(&state, &headers).await?;

    let bookmark = Bookmark {
        id: req.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        user_id: user.id,
        book_id,
        page: req.page,
        position_data: req.position_data,
        name: req.name,
        created_at: db::now_timestamp(),
    };

    state.db.save_bookmark(&bookmark)?;
    Ok(Json(bookmark))
}

/// Delete a bookmark.
pub async fn sync_delete_bookmark(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    let user = get_authenticated_user(&state, &headers).await?;
    state.db.delete_bookmark(&id, &user.id)?;
    Ok(StatusCode::OK)
}

// ============================================================================
// STATS API
// ============================================================================

/// API: Trigger library scan.
pub async fn api_scan(State(state): State<AppState>) -> Result<Json<ScanResponse>> {
    state.scan_all_libraries()?;

    Ok(Json(ScanResponse {
        total_books: state.book_count(),
    }))
}

/// Scan response.
#[derive(Serialize)]
pub struct ScanResponse {
    total_books: usize,
}

/// API: Get library statistics.
pub async fn api_stats(State(state): State<AppState>) -> Json<StatsResponse> {
    let books = state.get_all_books();
    let total_size: u64 = books.iter().map(|b| b.file_size).sum();

    let mut format_counts = std::collections::HashMap::new();
    for book in &books {
        *format_counts
            .entry(format!("{:?}", book.format))
            .or_insert(0) += 1;
    }

    Json(StatsResponse {
        total_books: books.len(),
        total_size_bytes: total_size,
        total_size_human: format_size(total_size),
        format_counts,
    })
}

/// Stats response.
#[derive(Serialize)]
pub struct StatsResponse {
    total_books: usize,
    total_size_bytes: u64,
    total_size_human: String,
    format_counts: std::collections::HashMap<String, usize>,
}

/// Format bytes to human-readable string.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

// ============================================================================
// LIBRARY SYNC API (for CloudReader)
// ============================================================================

/// Library entry for sync.
#[derive(Serialize)]
pub struct LibraryEntry {
    /// Book ID.
    pub id: String,
    /// Relative path within library (e.g., "Abara/Abara T01.pdf").
    pub path: String,
    /// Book title.
    pub title: String,
    /// Authors.
    pub authors: Vec<String>,
    /// File format (epub, pdf, cbz, etc.).
    pub format: String,
    /// File size in bytes.
    pub size: u64,
    /// Whether cover is available.
    pub has_cover: bool,
    /// Series name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    /// Series index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_index: Option<f32>,
    /// Description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Library sync response.
#[derive(Serialize)]
pub struct LibraryResponse {
    /// List of books.
    pub books: Vec<LibraryEntry>,
    /// Total count.
    pub total: usize,
}

/// Get full library listing for sync.
pub async fn api_library(State(state): State<AppState>) -> impl IntoResponse {
    let books_with_paths = state.get_books_with_paths();

    let entries: Vec<LibraryEntry> = books_with_paths
        .into_iter()
        .map(|(book, rel_path)| LibraryEntry {
            id: book.id,
            path: rel_path,
            title: book.title,
            authors: book.authors,
            format: format!("{:?}", book.format).to_lowercase(),
            size: book.file_size,
            has_cover: book.has_cover,
            series: book.series,
            series_index: book.series_index,
            description: book.description,
        })
        .collect();

    let total = entries.len();

    Json(LibraryResponse {
        books: entries,
        total,
    })
}

// ============================================================================
// SDR SYNC API (KOReader .sdr folders)
// ============================================================================

/// SDR info response (for listing).
#[derive(Serialize)]
pub struct SdrInfoResponse {
    pub book_id: String,
    pub last_page: Option<i64>,
    pub percent_finished: Option<f64>,
    pub updated_at: i64,
}

/// SDR list response.
#[derive(Serialize)]
pub struct SdrListResponse {
    pub sdrs: Vec<SdrInfoResponse>,
}

/// Get list of all SDR backups for user.
pub async fn sync_get_sdr_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SdrListResponse>> {
    let user = get_authenticated_user(&state, &headers).await?;
    let sdr_list = state.db.get_user_sdr_list(&user.id)?;

    let sdrs = sdr_list
        .into_iter()
        .map(|s| SdrInfoResponse {
            book_id: s.book_id,
            last_page: s.last_page,
            percent_finished: s.percent_finished,
            updated_at: s.updated_at,
        })
        .collect();

    Ok(Json(SdrListResponse { sdrs }))
}

/// Get SDR info for a specific book.
pub async fn sync_get_sdr_info(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Result<Json<Option<SdrInfoResponse>>> {
    let user = get_authenticated_user(&state, &headers).await?;
    let sdr_info = state.db.get_sdr_info(&user.id, &book_id)?;

    Ok(Json(sdr_info.map(|s| SdrInfoResponse {
        book_id: s.book_id,
        last_page: s.last_page,
        percent_finished: s.percent_finished,
        updated_at: s.updated_at,
    })))
}

/// Download SDR backup (returns tar.gz).
pub async fn sync_download_sdr(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Result<Response<Body>> {
    let user = get_authenticated_user(&state, &headers).await?;

    let sdr = state
        .db
        .get_sdr(&user.id, &book_id)?
        .ok_or_else(|| AppError::NotFound(format!("SDR not found for book: {}", book_id)))?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/gzip")
        .header(header::CONTENT_LENGTH, sdr.data.len())
        .body(Body::from(sdr.data))
        .map_err(|e| AppError::Internal(e.to_string()))
}

/// Upload SDR backup (receives tar.gz).
pub async fn sync_upload_sdr(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    body: axum::body::Bytes,
) -> Result<StatusCode> {
    let user = get_authenticated_user(&state, &headers).await?;

    // Extract metadata from tar.gz to get last_page and percent_finished
    let (last_page, percent_finished) = extract_sdr_metadata(&body)?;

    let sdr = db::SdrBackup {
        user_id: user.id,
        book_id,
        data: body.to_vec(),
        last_page,
        percent_finished,
        updated_at: db::now_timestamp(),
    };

    state.db.save_sdr(&sdr)?;
    Ok(StatusCode::OK)
}

/// Extract metadata from tar.gz SDR backup.
fn extract_sdr_metadata(data: &[u8]) -> Result<(Option<i64>, Option<f64>)> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    let decoder = GzDecoder::new(data);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive
        .entries()
        .map_err(|e| AppError::Internal(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| AppError::Internal(e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Look for metadata.*.lua file
        if let Some(name) = path.file_name() {
            let name_str = name.to_string_lossy();
            if name_str.starts_with("metadata.") && name_str.ends_with(".lua") {
                let mut content = String::new();
                entry
                    .read_to_string(&mut content)
                    .map_err(|e| AppError::Internal(e.to_string()))?;

                // Parse Lua table to extract last_page and percent_finished
                return Ok(parse_lua_metadata(&content));
            }
        }
    }

    Ok((None, None))
}

/// Parse KOReader Lua metadata file to extract progress info.
fn parse_lua_metadata(content: &str) -> (Option<i64>, Option<f64>) {
    let mut last_page: Option<i64> = None;
    let mut percent_finished: Option<f64> = None;

    for line in content.lines() {
        let line = line.trim();

        // Match ["last_page"] = 38,
        if line.starts_with("[\"last_page\"]")
            && let Some(value) = extract_lua_number(line)
        {
            last_page = Some(value as i64);
        }

        // Match ["percent_finished"] = 0.15966386554622,
        if line.starts_with("[\"percent_finished\"]")
            && let Some(value) = extract_lua_number(line)
        {
            percent_finished = Some(value);
        }
    }

    (last_page, percent_finished)
}

/// Extract number value from Lua assignment line.
fn extract_lua_number(line: &str) -> Option<f64> {
    // Find the = sign and parse the number after it
    let parts: Vec<&str> = line.split('=').collect();
    if parts.len() >= 2 {
        let value_part = parts[1].trim().trim_end_matches(',');
        value_part.parse::<f64>().ok()
    } else {
        None
    }
}

// ============================================================================
// HELPERS
// ============================================================================

/// Extract token from Authorization header.
fn extract_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Get authenticated user from token.
async fn get_authenticated_user(state: &AppState, headers: &HeaderMap) -> Result<db::User> {
    let token = extract_token(headers)
        .ok_or_else(|| AppError::InvalidFormat("Missing Authorization header".to_string()))?;

    state
        .auth
        .validate_token(&token)?
        .ok_or_else(|| AppError::InvalidFormat("Invalid or expired token".to_string()))
}
