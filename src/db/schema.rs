use crate::db::*;
use crate::error::{AppError, Result};
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::sync::Arc;

/// Database wrapper for thread-safe access.
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Open or create database at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)
            .map_err(|e| AppError::Internal(format!("Failed to open database: {}", e)))?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        db.initialize_schema()?;
        Ok(db)
    }

    /// Open in-memory database (for testing).
    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| AppError::Internal(format!("Failed to open database: {}", e)))?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        db.initialize_schema()?;
        Ok(db)
    }

    /// Initialize database schema.
    fn initialize_schema(&self) -> Result<()> {
        let conn = self.conn.lock();

        conn.execute_batch(
            r#"
            -- Users table
            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                display_name TEXT,
                role TEXT NOT NULL DEFAULT 'user',
                created_at INTEGER NOT NULL,
                last_login INTEGER
            );

            -- Sessions table
            CREATE TABLE IF NOT EXISTS sessions (
                token TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT,
                expires_at INTEGER NOT NULL,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            );

            -- Libraries table
            CREATE TABLE IF NOT EXISTS libraries (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                path TEXT NOT NULL,
                is_public INTEGER NOT NULL DEFAULT 1,
                owner_id TEXT,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (owner_id) REFERENCES users(id) ON DELETE SET NULL
            );

            -- Library access table
            CREATE TABLE IF NOT EXISTS library_access (
                user_id TEXT NOT NULL,
                library_id TEXT NOT NULL,
                can_write INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (user_id, library_id),
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (library_id) REFERENCES libraries(id) ON DELETE CASCADE
            );

            -- Books table (full metadata cache)
            CREATE TABLE IF NOT EXISTS books (
                id TEXT PRIMARY KEY,
                library_id TEXT NOT NULL,
                file_hash TEXT,
                title TEXT NOT NULL,
                author TEXT,
                authors_json TEXT,
                description TEXT,
                publisher TEXT,
                published TEXT,
                language TEXT,
                isbn TEXT,
                series TEXT,
                series_index REAL,
                tags_json TEXT,
                path TEXT NOT NULL,
                format TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                mtime INTEGER NOT NULL DEFAULT 0,
                page_count INTEGER,
                cover_cached INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                FOREIGN KEY (library_id) REFERENCES libraries(id) ON DELETE CASCADE
            );

            -- Reading progress table
            CREATE TABLE IF NOT EXISTS reading_progress (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id TEXT NOT NULL,
                book_id TEXT NOT NULL,
                device_id TEXT,
                current_page INTEGER,
                total_pages INTEGER,
                percentage REAL,
                current_chapter TEXT,
                position_data TEXT,
                status TEXT NOT NULL DEFAULT 'reading',
                started_at INTEGER,
                finished_at INTEGER,
                updated_at INTEGER NOT NULL,
                UNIQUE (user_id, book_id, device_id),
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
            );

            -- Highlights table
            CREATE TABLE IF NOT EXISTS highlights (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                book_id TEXT NOT NULL,
                device_id TEXT,
                page INTEGER,
                chapter TEXT,
                text TEXT NOT NULL,
                note TEXT,
                color TEXT NOT NULL DEFAULT 'yellow',
                pos0 TEXT,
                pos1 TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
            );

            -- Bookmarks table
            CREATE TABLE IF NOT EXISTS bookmarks (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                book_id TEXT NOT NULL,
                page INTEGER,
                position_data TEXT,
                name TEXT,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
            );

            -- Reading statistics table
            CREATE TABLE IF NOT EXISTS reading_stats (
                user_id TEXT NOT NULL,
                book_id TEXT NOT NULL,
                total_time_seconds INTEGER NOT NULL DEFAULT 0,
                pages_read INTEGER NOT NULL DEFAULT 0,
                sessions_count INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (user_id, book_id),
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
            );

            -- Devices table
            CREATE TABLE IF NOT EXISTS devices (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                name TEXT,
                model TEXT,
                last_seen INTEGER NOT NULL,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            );

            -- SDR backups table (KOReader .sdr folders)
            CREATE TABLE IF NOT EXISTS sdr_backups (
                user_id TEXT NOT NULL,
                book_id TEXT NOT NULL,
                data BLOB NOT NULL,
                last_page INTEGER,
                percent_finished REAL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (user_id, book_id),
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
            );

            -- Indexes
            CREATE INDEX IF NOT EXISTS idx_books_library ON books(library_id);
            CREATE INDEX IF NOT EXISTS idx_books_hash ON books(file_hash);
            CREATE INDEX IF NOT EXISTS idx_progress_user_book ON reading_progress(user_id, book_id);
            CREATE INDEX IF NOT EXISTS idx_highlights_user_book ON highlights(user_id, book_id);
            CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
            CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);
            CREATE INDEX IF NOT EXISTS idx_sdr_user ON sdr_backups(user_id);
            "#,
        )
        .map_err(|e| AppError::Internal(format!("Failed to initialize schema: {}", e)))?;

        Ok(())
    }

    // ========== USER OPERATIONS ==========

    /// Create a new user.
    pub fn create_user(&self, user: &User) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO users (id, username, password_hash, display_name, role, created_at, last_login)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                user.id,
                user.username,
                user.password_hash,
                user.display_name,
                user.role,
                user.created_at,
                user.last_login,
            ],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint") {
                AppError::InvalidFormat(format!("Username '{}' already exists", user.username))
            } else {
                AppError::Internal(format!("Failed to create user: {}", e))
            }
        })?;
        Ok(())
    }

    /// Get user by username.
    pub fn get_user_by_username(&self, username: &str) -> Result<Option<User>> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT id, username, password_hash, display_name, role, created_at, last_login
             FROM users WHERE username = ?1",
            params![username],
            |row| {
                Ok(User {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    password_hash: row.get(2)?,
                    display_name: row.get(3)?,
                    role: row.get(4)?,
                    created_at: row.get(5)?,
                    last_login: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::Internal(format!("Failed to get user: {}", e)))
    }

    /// Get user by ID.
    pub fn get_user_by_id(&self, id: &str) -> Result<Option<User>> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT id, username, password_hash, display_name, role, created_at, last_login
             FROM users WHERE id = ?1",
            params![id],
            |row| {
                Ok(User {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    password_hash: row.get(2)?,
                    display_name: row.get(3)?,
                    role: row.get(4)?,
                    created_at: row.get(5)?,
                    last_login: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::Internal(format!("Failed to get user: {}", e)))
    }

    /// List all users.
    pub fn list_users(&self) -> Result<Vec<User>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, username, password_hash, display_name, role, created_at, last_login
                 FROM users ORDER BY username",
            )
            .map_err(|e| AppError::Internal(format!("Failed to prepare query: {}", e)))?;

        let users = stmt
            .query_map([], |row| {
                Ok(User {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    password_hash: row.get(2)?,
                    display_name: row.get(3)?,
                    role: row.get(4)?,
                    created_at: row.get(5)?,
                    last_login: row.get(6)?,
                })
            })
            .map_err(|e| AppError::Internal(format!("Failed to list users: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(format!("Failed to collect users: {}", e)))?;

        Ok(users)
    }

    /// Update user password.
    pub fn update_user_password(&self, username: &str, password_hash: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let rows = conn
            .execute(
                "UPDATE users SET password_hash = ?1 WHERE username = ?2",
                params![password_hash, username],
            )
            .map_err(|e| AppError::Internal(format!("Failed to update password: {}", e)))?;
        Ok(rows > 0)
    }

    /// Update user last login.
    pub fn update_user_last_login(&self, user_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE users SET last_login = ?1 WHERE id = ?2",
            params![now_timestamp(), user_id],
        )
        .map_err(|e| AppError::Internal(format!("Failed to update last login: {}", e)))?;
        Ok(())
    }

    /// Delete user.
    pub fn delete_user(&self, username: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let rows = conn
            .execute("DELETE FROM users WHERE username = ?1", params![username])
            .map_err(|e| AppError::Internal(format!("Failed to delete user: {}", e)))?;
        Ok(rows > 0)
    }

    // ========== SESSION OPERATIONS ==========

    /// Create session.
    pub fn create_session(&self, session: &Session) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO sessions (token, user_id, device_id, expires_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                session.token,
                session.user_id,
                session.device_id,
                session.expires_at,
            ],
        )
        .map_err(|e| AppError::Internal(format!("Failed to create session: {}", e)))?;
        Ok(())
    }

    /// Get session by token.
    pub fn get_session(&self, token: &str) -> Result<Option<Session>> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT token, user_id, device_id, expires_at FROM sessions WHERE token = ?1",
            params![token],
            |row| {
                Ok(Session {
                    token: row.get(0)?,
                    user_id: row.get(1)?,
                    device_id: row.get(2)?,
                    expires_at: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::Internal(format!("Failed to get session: {}", e)))
    }

    /// Delete session.
    pub fn delete_session(&self, token: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM sessions WHERE token = ?1", params![token])
            .map_err(|e| AppError::Internal(format!("Failed to delete session: {}", e)))?;
        Ok(())
    }

    /// Cleanup expired sessions.
    pub fn cleanup_expired_sessions(&self) -> Result<usize> {
        let conn = self.conn.lock();
        let rows = conn
            .execute(
                "DELETE FROM sessions WHERE expires_at < ?1",
                params![now_timestamp()],
            )
            .map_err(|e| AppError::Internal(format!("Failed to cleanup sessions: {}", e)))?;
        Ok(rows)
    }

    // ========== LIBRARY OPERATIONS ==========

    /// Create library.
    pub fn create_library(&self, library: &Library) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO libraries (id, name, path, is_public, owner_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                library.id,
                library.name,
                library.path,
                library.is_public,
                library.owner_id,
                library.created_at,
            ],
        )
        .map_err(|e| AppError::Internal(format!("Failed to create library: {}", e)))?;
        Ok(())
    }

    /// Get library by name.
    pub fn get_library_by_name(&self, name: &str) -> Result<Option<Library>> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT id, name, path, is_public, owner_id, created_at
             FROM libraries WHERE name = ?1",
            params![name],
            |row| {
                Ok(Library {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    path: row.get(2)?,
                    is_public: row.get(3)?,
                    owner_id: row.get(4)?,
                    created_at: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::Internal(format!("Failed to get library: {}", e)))
    }

    /// List all libraries.
    pub fn list_libraries(&self) -> Result<Vec<Library>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, path, is_public, owner_id, created_at
                 FROM libraries ORDER BY name",
            )
            .map_err(|e| AppError::Internal(format!("Failed to prepare query: {}", e)))?;

        let libraries = stmt
            .query_map([], |row| {
                Ok(Library {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    path: row.get(2)?,
                    is_public: row.get(3)?,
                    owner_id: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| AppError::Internal(format!("Failed to list libraries: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(format!("Failed to collect libraries: {}", e)))?;

        Ok(libraries)
    }

    /// Get libraries accessible by user.
    pub fn get_user_libraries(&self, user_id: &str) -> Result<Vec<Library>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT l.id, l.name, l.path, l.is_public, l.owner_id, l.created_at
                 FROM libraries l
                 LEFT JOIN library_access la ON l.id = la.library_id
                 WHERE l.is_public = 1 OR l.owner_id = ?1 OR la.user_id = ?1
                 ORDER BY l.name",
            )
            .map_err(|e| AppError::Internal(format!("Failed to prepare query: {}", e)))?;

        let libraries = stmt
            .query_map(params![user_id], |row| {
                Ok(Library {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    path: row.get(2)?,
                    is_public: row.get(3)?,
                    owner_id: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| AppError::Internal(format!("Failed to get libraries: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(format!("Failed to collect libraries: {}", e)))?;

        Ok(libraries)
    }

    /// Delete library.
    pub fn delete_library(&self, name: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let rows = conn
            .execute("DELETE FROM libraries WHERE name = ?1", params![name])
            .map_err(|e| AppError::Internal(format!("Failed to delete library: {}", e)))?;
        Ok(rows > 0)
    }

    /// Update library path.
    pub fn update_library_path(&self, name: &str, path: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let rows = conn
            .execute(
                "UPDATE libraries SET path = ?1 WHERE name = ?2",
                params![path, name],
            )
            .map_err(|e| AppError::Internal(format!("Failed to update library path: {}", e)))?;
        Ok(rows > 0)
    }

    // ========== PROGRESS OPERATIONS ==========

    /// Save or update reading progress.
    pub fn save_progress(&self, progress: &ReadingProgress) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO reading_progress 
             (user_id, book_id, device_id, current_page, total_pages, percentage, 
              current_chapter, position_data, status, started_at, finished_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT (user_id, book_id, device_id) DO UPDATE SET
                current_page = excluded.current_page,
                total_pages = excluded.total_pages,
                percentage = excluded.percentage,
                current_chapter = excluded.current_chapter,
                position_data = excluded.position_data,
                status = excluded.status,
                started_at = COALESCE(reading_progress.started_at, excluded.started_at),
                finished_at = excluded.finished_at,
                updated_at = excluded.updated_at",
            params![
                progress.user_id,
                progress.book_id,
                progress.device_id,
                progress.current_page,
                progress.total_pages,
                progress.percentage,
                progress.current_chapter,
                progress.position_data,
                progress.status,
                progress.started_at,
                progress.finished_at,
                progress.updated_at,
            ],
        )
        .map_err(|e| AppError::Internal(format!("Failed to save progress: {}", e)))?;
        Ok(())
    }

    /// Get reading progress for a book.
    pub fn get_progress(&self, user_id: &str, book_id: &str) -> Result<Option<ReadingProgress>> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT id, user_id, book_id, device_id, current_page, total_pages, percentage,
                    current_chapter, position_data, status, started_at, finished_at, updated_at
             FROM reading_progress 
             WHERE user_id = ?1 AND book_id = ?2
             ORDER BY updated_at DESC, id DESC LIMIT 1",
            params![user_id, book_id],
            |row| {
                Ok(ReadingProgress {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    book_id: row.get(2)?,
                    device_id: row.get(3)?,
                    current_page: row.get(4)?,
                    total_pages: row.get(5)?,
                    percentage: row.get(6)?,
                    current_chapter: row.get(7)?,
                    position_data: row.get(8)?,
                    status: row.get(9)?,
                    started_at: row.get(10)?,
                    finished_at: row.get(11)?,
                    updated_at: row.get(12)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::Internal(format!("Failed to get progress: {}", e)))
    }

    // ========== HIGHLIGHT OPERATIONS ==========

    /// Save a highlight.
    pub fn save_highlight(&self, highlight: &Highlight) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO highlights 
             (id, user_id, book_id, device_id, page, chapter, text, note, color, pos0, pos1, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT (id) DO UPDATE SET
                note = excluded.note,
                color = excluded.color,
                updated_at = excluded.updated_at",
            params![
                highlight.id,
                highlight.user_id,
                highlight.book_id,
                highlight.device_id,
                highlight.page,
                highlight.chapter,
                highlight.text,
                highlight.note,
                highlight.color,
                highlight.pos0,
                highlight.pos1,
                highlight.created_at,
                highlight.updated_at,
            ],
        )
        .map_err(|e| AppError::Internal(format!("Failed to save highlight: {}", e)))?;
        Ok(())
    }

    /// Get highlights for a book.
    pub fn get_highlights(&self, user_id: &str, book_id: &str) -> Result<Vec<Highlight>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, user_id, book_id, device_id, page, chapter, text, note, color, pos0, pos1, created_at, updated_at
                 FROM highlights WHERE user_id = ?1 AND book_id = ?2
                 ORDER BY page, created_at",
            )
            .map_err(|e| AppError::Internal(format!("Failed to prepare query: {}", e)))?;

        let highlights = stmt
            .query_map(params![user_id, book_id], |row| {
                Ok(Highlight {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    book_id: row.get(2)?,
                    device_id: row.get(3)?,
                    page: row.get(4)?,
                    chapter: row.get(5)?,
                    text: row.get(6)?,
                    note: row.get(7)?,
                    color: row.get(8)?,
                    pos0: row.get(9)?,
                    pos1: row.get(10)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                })
            })
            .map_err(|e| AppError::Internal(format!("Failed to get highlights: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(format!("Failed to collect highlights: {}", e)))?;

        Ok(highlights)
    }

    /// Delete highlight.
    pub fn delete_highlight(&self, id: &str, user_id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let rows = conn
            .execute(
                "DELETE FROM highlights WHERE id = ?1 AND user_id = ?2",
                params![id, user_id],
            )
            .map_err(|e| AppError::Internal(format!("Failed to delete highlight: {}", e)))?;
        Ok(rows > 0)
    }

    // ========== BOOKMARK OPERATIONS ==========

    /// Save a bookmark.
    pub fn save_bookmark(&self, bookmark: &Bookmark) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO bookmarks (id, user_id, book_id, page, position_data, name, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (id) DO UPDATE SET
                name = excluded.name",
            params![
                bookmark.id,
                bookmark.user_id,
                bookmark.book_id,
                bookmark.page,
                bookmark.position_data,
                bookmark.name,
                bookmark.created_at,
            ],
        )
        .map_err(|e| AppError::Internal(format!("Failed to save bookmark: {}", e)))?;
        Ok(())
    }

    /// Get bookmarks for a book.
    pub fn get_bookmarks(&self, user_id: &str, book_id: &str) -> Result<Vec<Bookmark>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, user_id, book_id, page, position_data, name, created_at
                 FROM bookmarks WHERE user_id = ?1 AND book_id = ?2
                 ORDER BY page, created_at",
            )
            .map_err(|e| AppError::Internal(format!("Failed to prepare query: {}", e)))?;

        let bookmarks = stmt
            .query_map(params![user_id, book_id], |row| {
                Ok(Bookmark {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    book_id: row.get(2)?,
                    page: row.get(3)?,
                    position_data: row.get(4)?,
                    name: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })
            .map_err(|e| AppError::Internal(format!("Failed to get bookmarks: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(format!("Failed to collect bookmarks: {}", e)))?;

        Ok(bookmarks)
    }

    /// Delete bookmark.
    pub fn delete_bookmark(&self, id: &str, user_id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let rows = conn
            .execute(
                "DELETE FROM bookmarks WHERE id = ?1 AND user_id = ?2",
                params![id, user_id],
            )
            .map_err(|e| AppError::Internal(format!("Failed to delete bookmark: {}", e)))?;
        Ok(rows > 0)
    }

    // ========== BOOK OPERATIONS ==========

    /// Save or update a book.
    pub fn save_book(&self, book: &StoredBook) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO books 
             (id, library_id, file_hash, title, author, authors_json, description, publisher, 
              published, language, isbn, series, series_index, tags_json, path, format, 
              file_size, mtime, page_count, cover_cached, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)
             ON CONFLICT (id) DO UPDATE SET
                file_hash = excluded.file_hash,
                title = excluded.title,
                author = excluded.author,
                authors_json = excluded.authors_json,
                description = excluded.description,
                publisher = excluded.publisher,
                published = excluded.published,
                language = excluded.language,
                isbn = excluded.isbn,
                series = excluded.series,
                series_index = excluded.series_index,
                tags_json = excluded.tags_json,
                path = excluded.path,
                file_size = excluded.file_size,
                mtime = excluded.mtime,
                page_count = excluded.page_count,
                cover_cached = excluded.cover_cached,
                updated_at = excluded.updated_at",
            params![
                book.id,
                book.library_id,
                book.file_hash,
                book.title,
                book.author,
                book.authors_json,
                book.description,
                book.publisher,
                book.published,
                book.language,
                book.isbn,
                book.series,
                book.series_index,
                book.tags_json,
                book.path,
                book.format,
                book.file_size,
                book.mtime,
                book.page_count,
                book.cover_cached,
                book.created_at,
                book.updated_at,
            ],
        )
        .map_err(|e| AppError::Internal(format!("Failed to save book: {}", e)))?;
        Ok(())
    }

    /// Get book by ID.
    pub fn get_book(&self, id: &str) -> Result<Option<StoredBook>> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT id, library_id, file_hash, title, author, authors_json, description, publisher,
                    published, language, isbn, series, series_index, tags_json, path, format,
                    file_size, mtime, page_count, cover_cached, created_at, updated_at
             FROM books WHERE id = ?1",
            params![id],
            Self::row_to_stored_book,
        )
        .optional()
        .map_err(|e| AppError::Internal(format!("Failed to get book: {}", e)))
    }

    /// Get book by file hash.
    pub fn get_book_by_hash(&self, hash: &str) -> Result<Option<StoredBook>> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT id, library_id, file_hash, title, author, authors_json, description, publisher,
                    published, language, isbn, series, series_index, tags_json, path, format,
                    file_size, mtime, page_count, cover_cached, created_at, updated_at
             FROM books WHERE file_hash = ?1",
            params![hash],
            Self::row_to_stored_book,
        )
        .optional()
        .map_err(|e| AppError::Internal(format!("Failed to get book by hash: {}", e)))
    }

    /// Get books in a library.
    pub fn get_library_books(&self, library_id: &str) -> Result<Vec<StoredBook>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, library_id, file_hash, title, author, authors_json, description, publisher,
                        published, language, isbn, series, series_index, tags_json, path, format,
                        file_size, mtime, page_count, cover_cached, created_at, updated_at
                 FROM books WHERE library_id = ?1
                 ORDER BY title",
            )
            .map_err(|e| AppError::Internal(format!("Failed to prepare query: {}", e)))?;

        let books = stmt
            .query_map(params![library_id], Self::row_to_stored_book)
            .map_err(|e| AppError::Internal(format!("Failed to get books: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(format!("Failed to collect books: {}", e)))?;

        Ok(books)
    }

    /// Get all books from all libraries.
    pub fn get_all_books(&self) -> Result<Vec<StoredBook>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, library_id, file_hash, title, author, authors_json, description, publisher,
                        published, language, isbn, series, series_index, tags_json, path, format,
                        file_size, mtime, page_count, cover_cached, created_at, updated_at
                 FROM books ORDER BY title",
            )
            .map_err(|e| AppError::Internal(format!("Failed to prepare query: {}", e)))?;

        let books = stmt
            .query_map([], Self::row_to_stored_book)
            .map_err(|e| AppError::Internal(format!("Failed to get all books: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(format!("Failed to collect books: {}", e)))?;

        Ok(books)
    }

    /// Helper to convert a row to StoredBook.
    fn row_to_stored_book(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredBook> {
        Ok(StoredBook {
            id: row.get(0)?,
            library_id: row.get(1)?,
            file_hash: row.get(2)?,
            title: row.get(3)?,
            author: row.get(4)?,
            authors_json: row.get(5)?,
            description: row.get(6)?,
            publisher: row.get(7)?,
            published: row.get(8)?,
            language: row.get(9)?,
            isbn: row.get(10)?,
            series: row.get(11)?,
            series_index: row.get(12)?,
            tags_json: row.get(13)?,
            path: row.get(14)?,
            format: row.get(15)?,
            file_size: row.get(16)?,
            mtime: row.get(17)?,
            page_count: row.get(18)?,
            cover_cached: row.get(19)?,
            created_at: row.get(20)?,
            updated_at: row.get(21)?,
        })
    }

    /// Delete books not in the given list of IDs (cleanup removed files).
    pub fn delete_books_not_in(&self, library_id: &str, keep_ids: &[String]) -> Result<usize> {
        if keep_ids.is_empty() {
            return Ok(0);
        }

        let conn = self.conn.lock();
        let placeholders: Vec<String> = keep_ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "DELETE FROM books WHERE library_id = ? AND id NOT IN ({})",
            placeholders.join(",")
        );

        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&library_id];
        for id in keep_ids {
            params.push(id);
        }

        let deleted = conn
            .execute(&sql, rusqlite::params_from_iter(params))
            .map_err(|e| AppError::Internal(format!("Failed to delete books: {}", e)))?;

        Ok(deleted)
    }

    /// Delete a single book by ID.
    pub fn delete_book(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let rows = conn
            .execute("DELETE FROM books WHERE id = ?1", params![id])
            .map_err(|e| AppError::Internal(format!("Failed to delete book: {}", e)))?;
        Ok(rows > 0)
    }

    // ========== SDR BACKUP OPERATIONS ==========

    /// Save or update an SDR backup.
    pub fn save_sdr(&self, sdr: &SdrBackup) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO sdr_backups (user_id, book_id, data, last_page, percent_finished, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (user_id, book_id) DO UPDATE SET
                data = excluded.data,
                last_page = excluded.last_page,
                percent_finished = excluded.percent_finished,
                updated_at = excluded.updated_at",
            params![
                sdr.user_id,
                sdr.book_id,
                sdr.data,
                sdr.last_page,
                sdr.percent_finished,
                sdr.updated_at,
            ],
        )
        .map_err(|e| AppError::Internal(format!("Failed to save SDR backup: {}", e)))?;
        Ok(())
    }

    /// Get SDR backup data.
    pub fn get_sdr(&self, user_id: &str, book_id: &str) -> Result<Option<SdrBackup>> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT user_id, book_id, data, last_page, percent_finished, updated_at
             FROM sdr_backups WHERE user_id = ?1 AND book_id = ?2",
            params![user_id, book_id],
            |row| {
                Ok(SdrBackup {
                    user_id: row.get(0)?,
                    book_id: row.get(1)?,
                    data: row.get(2)?,
                    last_page: row.get(3)?,
                    percent_finished: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::Internal(format!("Failed to get SDR backup: {}", e)))
    }

    /// Get SDR info (metadata only, no data blob).
    pub fn get_sdr_info(&self, user_id: &str, book_id: &str) -> Result<Option<SdrInfo>> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT book_id, last_page, percent_finished, updated_at
             FROM sdr_backups WHERE user_id = ?1 AND book_id = ?2",
            params![user_id, book_id],
            |row| {
                Ok(SdrInfo {
                    book_id: row.get(0)?,
                    last_page: row.get(1)?,
                    percent_finished: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::Internal(format!("Failed to get SDR info: {}", e)))
    }

    /// Get all SDR info for a user (for sync comparison).
    pub fn get_user_sdr_list(&self, user_id: &str) -> Result<Vec<SdrInfo>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT book_id, last_page, percent_finished, updated_at
                 FROM sdr_backups WHERE user_id = ?1",
            )
            .map_err(|e| AppError::Internal(format!("Failed to prepare query: {}", e)))?;

        let sdrs = stmt
            .query_map(params![user_id], |row| {
                Ok(SdrInfo {
                    book_id: row.get(0)?,
                    last_page: row.get(1)?,
                    percent_finished: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })
            .map_err(|e| AppError::Internal(format!("Failed to get SDR list: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(format!("Failed to collect SDR list: {}", e)))?;

        Ok(sdrs)
    }

    /// Delete SDR backup.
    pub fn delete_sdr(&self, user_id: &str, book_id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let rows = conn
            .execute(
                "DELETE FROM sdr_backups WHERE user_id = ?1 AND book_id = ?2",
                params![user_id, book_id],
            )
            .map_err(|e| AppError::Internal(format!("Failed to delete SDR backup: {}", e)))?;
        Ok(rows > 0)
    }
}
