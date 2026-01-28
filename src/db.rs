mod schema;

pub use schema::Database;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// User account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// Unique user ID.
    pub id: String,
    /// Username for login.
    pub username: String,
    /// Argon2 password hash.
    #[serde(skip_serializing)]
    pub password_hash: String,
    /// Display name.
    pub display_name: Option<String>,
    /// User role: "admin" or "user".
    pub role: String,
    /// Account creation timestamp.
    pub created_at: i64,
    /// Last login timestamp.
    pub last_login: Option<i64>,
}

/// Authentication session.
#[derive(Debug, Clone)]
pub struct Session {
    /// Session token.
    pub token: String,
    /// User ID.
    pub user_id: String,
    /// Device ID (optional).
    pub device_id: Option<String>,
    /// Expiration timestamp.
    pub expires_at: i64,
}

/// Library collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    /// Unique library ID.
    pub id: String,
    /// Library name.
    pub name: String,
    /// Path on filesystem.
    pub path: String,
    /// Whether library is public.
    pub is_public: bool,
    /// Owner user ID (None for system library).
    pub owner_id: Option<String>,
    /// Creation timestamp.
    pub created_at: i64,
}

/// Reading progress for a book.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingProgress {
    /// Progress ID.
    pub id: i64,
    /// User ID.
    pub user_id: String,
    /// Book ID.
    pub book_id: String,
    /// Device ID.
    pub device_id: Option<String>,
    /// Current page number.
    pub current_page: Option<i64>,
    /// Total pages in book.
    pub total_pages: Option<i64>,
    /// Reading percentage (0.0 - 100.0).
    pub percentage: Option<f64>,
    /// Current chapter name.
    pub current_chapter: Option<String>,
    /// Raw position data (KOReader format).
    pub position_data: Option<String>,
    /// Reading status.
    pub status: String,
    /// Started reading timestamp.
    pub started_at: Option<i64>,
    /// Finished reading timestamp.
    pub finished_at: Option<i64>,
    /// Last update timestamp.
    pub updated_at: i64,
}

/// Highlight/annotation in a book.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Highlight {
    /// Highlight ID.
    pub id: String,
    /// User ID.
    pub user_id: String,
    /// Book ID.
    pub book_id: String,
    /// Device ID.
    pub device_id: Option<String>,
    /// Page number.
    pub page: Option<i64>,
    /// Chapter name.
    pub chapter: Option<String>,
    /// Highlighted text.
    pub text: String,
    /// User note/annotation.
    pub note: Option<String>,
    /// Highlight color.
    pub color: String,
    /// Start position (KOReader format).
    pub pos0: Option<String>,
    /// End position (KOReader format).
    pub pos1: Option<String>,
    /// Creation timestamp.
    pub created_at: i64,
    /// Last update timestamp.
    pub updated_at: i64,
}

/// Bookmark in a book.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    /// Bookmark ID.
    pub id: String,
    /// User ID.
    pub user_id: String,
    /// Book ID.
    pub book_id: String,
    /// Page number.
    pub page: Option<i64>,
    /// Position data (KOReader format).
    pub position_data: Option<String>,
    /// Bookmark name.
    pub name: Option<String>,
    /// Creation timestamp.
    pub created_at: i64,
}

/// Reading statistics for a book.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingStats {
    /// User ID.
    pub user_id: String,
    /// Book ID.
    pub book_id: String,
    /// Total reading time in seconds.
    pub total_time_seconds: i64,
    /// Total pages read.
    pub pages_read: i64,
    /// Number of reading sessions.
    pub sessions_count: i64,
    /// Last update timestamp.
    pub updated_at: i64,
}

/// Device information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    /// Device ID.
    pub id: String,
    /// User ID.
    pub user_id: String,
    /// Device name.
    pub name: Option<String>,
    /// Device model.
    pub model: Option<String>,
    /// Last seen timestamp.
    pub last_seen: i64,
}

/// Stored book in database (full metadata cache).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredBook {
    /// Book ID.
    pub id: String,
    /// Library ID.
    pub library_id: String,
    /// File hash for identification.
    pub file_hash: Option<String>,
    /// Book title.
    pub title: String,
    /// Primary author.
    pub author: Option<String>,
    /// All authors (JSON array).
    pub authors_json: Option<String>,
    /// Book description.
    pub description: Option<String>,
    /// Publisher.
    pub publisher: Option<String>,
    /// Publication date.
    pub published: Option<String>,
    /// Language code.
    pub language: Option<String>,
    /// ISBN.
    pub isbn: Option<String>,
    /// Series name.
    pub series: Option<String>,
    /// Series index.
    pub series_index: Option<f32>,
    /// Tags (JSON array).
    pub tags_json: Option<String>,
    /// Absolute path to file.
    pub path: String,
    /// Book format.
    pub format: String,
    /// File size in bytes.
    pub file_size: i64,
    /// File modification time (for cache invalidation).
    pub mtime: i64,
    /// Page count.
    pub page_count: Option<i64>,
    /// Whether cover is available.
    pub cover_cached: bool,
    /// Creation timestamp.
    pub created_at: i64,
    /// Last update timestamp.
    pub updated_at: i64,
}

/// SDR backup (KOReader .sdr folder).
#[derive(Debug, Clone)]
pub struct SdrBackup {
    /// User ID.
    pub user_id: String,
    /// Book ID.
    pub book_id: String,
    /// Compressed .sdr data (tar.gz).
    pub data: Vec<u8>,
    /// Last page read (for quick comparison).
    pub last_page: Option<i64>,
    /// Reading percentage (for quick comparison).
    pub percent_finished: Option<f64>,
    /// Last update timestamp.
    pub updated_at: i64,
}

/// SDR info (metadata only, no data blob).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdrInfo {
    /// Book ID.
    pub book_id: String,
    /// Last page read.
    pub last_page: Option<i64>,
    /// Reading percentage.
    pub percent_finished: Option<f64>,
    /// Last update timestamp.
    pub updated_at: i64,
}

/// Timestamp helper.
pub fn now_timestamp() -> i64 {
    Utc::now().timestamp()
}

/// Convert timestamp to DateTime.
pub fn timestamp_to_datetime(ts: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(ts, 0).unwrap_or_else(Utc::now)
}
