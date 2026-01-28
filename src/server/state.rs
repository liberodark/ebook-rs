//! Application state shared across handlers.

use crate::auth::AuthService;
use crate::config::{BookFormat, Config};
use crate::db::{self, Database, StoredBook};
use crate::error::Result;
use crate::formats;
use crate::library::book::Book;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Application configuration.
    pub config: Arc<Config>,
    /// Database connection.
    pub db: Database,
    /// Authentication service.
    pub auth: Arc<AuthService>,
    /// In-memory book cache (for quick access).
    books: Arc<parking_lot::RwLock<Vec<Book>>>,
    /// Whether initial load from DB is complete.
    loaded: Arc<AtomicBool>,
    /// Whether a scan is currently in progress.
    scanning: Arc<AtomicBool>,
}

impl AppState {
    /// Create new application state with database.
    pub fn new_with_db(config: Config, db: Database, auth: AuthService) -> Self {
        Self {
            config: Arc::new(config),
            db,
            auth: Arc::new(auth),
            books: Arc::new(parking_lot::RwLock::new(Vec::new())),
            loaded: Arc::new(AtomicBool::new(false)),
            scanning: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get the base URL for generating links.
    pub fn base_url(&self) -> String {
        String::new()
    }

    /// Load books from database into memory cache (instant startup).
    pub fn load_from_db(&self) -> Result<()> {
        if self.loaded.load(Ordering::Relaxed) {
            return Ok(());
        }

        tracing::info!("Loading books from database...");
        let start = std::time::Instant::now();

        let stored_books = self.db.get_all_books()?;
        let books: Vec<Book> = stored_books
            .into_iter()
            .filter_map(|sb| Self::stored_to_book(&sb))
            .collect();

        let count = books.len();
        *self.books.write() = books;
        self.loaded.store(true, Ordering::Relaxed);

        tracing::info!(books = count, elapsed = ?start.elapsed(), "Loaded from database");
        Ok(())
    }

    /// Convert StoredBook to Book.
    fn stored_to_book(sb: &StoredBook) -> Option<Book> {
        let format = BookFormat::from_extension(sb.format.trim_matches('"'))?;
        let path = PathBuf::from(&sb.path);

        // Parse authors from JSON or single author
        let authors = sb
            .authors_json
            .as_ref()
            .and_then(|j| serde_json::from_str::<Vec<String>>(j).ok())
            .unwrap_or_else(|| sb.author.clone().into_iter().collect());

        Some(Book {
            id: sb.id.clone(),
            title: sb.title.clone(),
            authors,
            description: sb.description.clone(),
            publisher: sb.publisher.clone(),
            published: sb.published.clone(),
            language: sb.language.clone(),
            isbn: sb.isbn.clone(),
            series: sb.series.clone(),
            series_index: sb.series_index,
            tags: sb
                .tags_json
                .as_ref()
                .and_then(|j| serde_json::from_str::<Vec<String>>(j).ok())
                .unwrap_or_default(),
            path,
            format,
            file_size: sb.file_size as u64,
            page_count: sb.page_count.map(|p| p as u32),
            has_cover: sb.cover_cached,
            modified: chrono::DateTime::from_timestamp(sb.mtime, 0)
                .unwrap_or_else(chrono::Utc::now),
        })
    }

    /// Convert Book to StoredBook.
    fn book_to_stored(book: &Book, library_id: &str) -> StoredBook {
        let now = db::now_timestamp();
        let mtime = book.modified.timestamp();

        StoredBook {
            id: book.id.clone(),
            library_id: library_id.to_string(),
            file_hash: None,
            title: book.title.clone(),
            author: book.authors.first().cloned(),
            authors_json: Some(serde_json::to_string(&book.authors).unwrap_or_default()),
            description: book.description.clone(),
            publisher: book.publisher.clone(),
            published: book.published.clone(),
            language: book.language.clone(),
            isbn: book.isbn.clone(),
            series: book.series.clone(),
            series_index: book.series_index,
            tags_json: if book.tags.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&book.tags).unwrap_or_default())
            },
            path: book.path.to_string_lossy().to_string(),
            format: format!("{:?}", book.format).to_lowercase(),
            file_size: book.file_size as i64,
            mtime,
            page_count: book.page_count.map(|p| p as i64),
            cover_cached: book.has_cover,
            created_at: now,
            updated_at: now,
        }
    }

    /// Scan all libraries incrementally (only changed files).
    pub fn scan_all_libraries(&self) -> Result<()> {
        // Prevent concurrent scans
        if self.scanning.swap(true, Ordering::SeqCst) {
            tracing::info!("Scan already in progress, skipping");
            return Ok(());
        }

        let result = self.do_incremental_scan();
        self.scanning.store(false, Ordering::SeqCst);
        result
    }

    /// Perform the actual incremental scan.
    fn do_incremental_scan(&self) -> Result<()> {
        let libraries = self.db.list_libraries()?;
        let start = std::time::Instant::now();
        let mut total_new = 0;
        let mut total_updated = 0;
        let mut total_unchanged = 0;
        let mut total_removed = 0;

        for library in libraries {
            let lib_path = PathBuf::from(&library.path);
            if !lib_path.exists() {
                tracing::warn!(library = %library.name, path = %library.path, "Library path does not exist");
                continue;
            }

            tracing::info!(library = %library.name, "Scanning library (incremental)");

            // Get existing books from DB for this library
            let existing = self.db.get_library_books(&library.id)?;
            let existing_map: HashMap<String, StoredBook> =
                existing.into_iter().map(|b| (b.id.clone(), b)).collect();

            // Scan filesystem
            let (new, updated, unchanged, scanned_ids) =
                self.scan_directory_incremental(&lib_path, &library.id, &existing_map)?;

            total_new += new;
            total_updated += updated;
            total_unchanged += unchanged;

            // Remove books that no longer exist on filesystem
            let removed_count: usize = existing_map
                .keys()
                .filter(|id| !scanned_ids.contains(*id))
                .count();

            if removed_count > 0 {
                total_removed += removed_count;
                for id in existing_map.keys() {
                    if !scanned_ids.contains(id) {
                        let _ = self.db.delete_book(id);
                    }
                }
                tracing::info!(library = %library.name, removed = removed_count, "Removed deleted books");
            }

            tracing::info!(
                library = %library.name,
                new = new,
                updated = updated,
                unchanged = unchanged,
                "Library scan complete"
            );
        }

        // Reload from DB to update in-memory cache
        self.loaded.store(false, Ordering::Relaxed);
        self.load_from_db()?;

        tracing::info!(
            new = total_new,
            updated = total_updated,
            unchanged = total_unchanged,
            removed = total_removed,
            elapsed = ?start.elapsed(),
            "Full scan complete"
        );

        Ok(())
    }

    /// Scan a directory incrementally, comparing with existing DB entries.
    fn scan_directory_incremental(
        &self,
        path: &PathBuf,
        library_id: &str,
        existing: &HashMap<String, StoredBook>,
    ) -> Result<(usize, usize, usize, Vec<String>)> {
        // Collect files first
        let files: Vec<_> = walkdir::WalkDir::new(path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .filter_map(|e| {
                let file_path = e.path().to_path_buf();
                let extension = file_path.extension()?.to_str()?;
                let format = BookFormat::from_extension(extension)?;
                let metadata = std::fs::metadata(&file_path).ok()?;
                Some((file_path, format, metadata))
            })
            .collect();

        tracing::info!(files = files.len(), "Found files to process");

        // Separate files into: unchanged (skip), needs_processing (new/updated)
        let mut scanned_ids = Vec::with_capacity(files.len());
        let mut to_process = Vec::new();
        let mut unchanged_count = 0;

        for (file_path, format, metadata) in files {
            let id = uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_URL,
                file_path.to_string_lossy().as_bytes(),
            )
            .to_string();

            scanned_ids.push(id.clone());

            let file_size = metadata.len() as i64;
            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            // Check if file has changed
            if let Some(existing_book) = existing.get(&id)
                && existing_book.mtime == mtime
                && existing_book.file_size == file_size
            {
                // Unchanged - skip
                unchanged_count += 1;
                continue;
            }

            // Needs processing (new or updated)
            to_process.push((file_path, format, metadata, id));
        }

        let to_process_count = to_process.len();
        if to_process_count == 0 {
            return Ok((0, 0, unchanged_count, scanned_ids));
        }

        let workers = self.config.scan.workers;
        tracing::info!(
            to_process = to_process_count,
            unchanged = unchanged_count,
            workers = workers,
            "Processing new/updated files"
        );

        // Process with configurable parallelism
        let new_count = AtomicUsize::new(0);
        let updated_count = AtomicUsize::new(0);
        let processed = AtomicUsize::new(0);
        let library_id_owned = library_id.to_string();

        // Build thread pool with limited workers
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap_or_else(|_| rayon::ThreadPoolBuilder::new().build().unwrap());

        pool.install(|| {
            to_process
                .par_iter()
                .for_each(|(file_path, format, metadata, id)| {
                    let is_new = !existing.contains_key(id);
                    if is_new {
                        new_count.fetch_add(1, Ordering::Relaxed);
                    } else {
                        updated_count.fetch_add(1, Ordering::Relaxed);
                    }

                    // Extract metadata
                    if let Ok(book) = self.extract_book_metadata(file_path, id, *format, metadata) {
                        let stored = Self::book_to_stored(&book, &library_id_owned);
                        // Save immediately (SQLite handles locking via parking_lot::Mutex)
                        let _ = self.db.save_book(&stored);
                    }

                    // Progress logging every 100 files
                    let done = processed.fetch_add(1, Ordering::Relaxed) + 1;
                    if done.is_multiple_of(100) || done == to_process_count {
                        let percent = (done * 100) / to_process_count;
                        tracing::info!(
                            "Processing... {}/{} ({}%)",
                            done,
                            to_process_count,
                            percent
                        );
                    }
                });
        });

        Ok((
            new_count.load(Ordering::Relaxed),
            updated_count.load(Ordering::Relaxed),
            unchanged_count,
            scanned_ids,
        ))
    }

    /// Extract metadata from a single book file.
    fn extract_book_metadata(
        &self,
        file_path: &std::path::Path,
        id: &str,
        format: BookFormat,
        metadata: &std::fs::Metadata,
    ) -> Result<Book> {
        let title = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let mut book = Book {
            id: id.to_string(),
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
            path: file_path.to_path_buf(),
            format,
            file_size: metadata.len(),
            page_count: None,
            has_cover: false,
            modified: chrono::DateTime::from_timestamp(mtime, 0).unwrap_or_else(chrono::Utc::now),
        };

        // Extract metadata
        let handler = formats::get_handler(format);
        if let Err(e) = handler.extract_metadata(&mut book) {
            tracing::debug!(path = %file_path.display(), error = %e, "Failed to extract metadata");
        }

        Ok(book)
    }

    /// Start a background scan (non-blocking).
    pub fn start_background_scan(&self) {
        let state = self.clone();
        std::thread::spawn(move || {
            if let Err(e) = state.scan_all_libraries() {
                tracing::error!(error = %e, "Background scan failed");
            }
        });
    }

    /// Get all books.
    pub fn get_all_books(&self) -> Vec<Book> {
        self.books.read().clone()
    }

    /// Get book by ID.
    pub fn get_book(&self, id: &str) -> Option<Book> {
        self.books.read().iter().find(|b| b.id == id).cloned()
    }

    /// Get recent books.
    pub fn get_recent(&self, limit: usize) -> Vec<Book> {
        let mut books = self.books.read().clone();
        books.sort_by(|a, b| b.modified.cmp(&a.modified));
        books.truncate(limit);
        books
    }

    /// Search books.
    pub fn search(&self, query: &str) -> Vec<Book> {
        let query = query.to_lowercase();
        self.books
            .read()
            .iter()
            .filter(|b| {
                b.title.to_lowercase().contains(&query)
                    || b.authors.iter().any(|a| a.to_lowercase().contains(&query))
            })
            .cloned()
            .collect()
    }

    /// Get book count.
    pub fn book_count(&self) -> usize {
        self.books.read().len()
    }

    /// Get all books with their relative paths for sync API.
    pub fn get_books_with_paths(&self) -> Vec<(Book, String)> {
        let libraries = match self.db.list_libraries() {
            Ok(libs) => libs,
            Err(_) => return Vec::new(),
        };

        let books = self.books.read();
        let mut result = Vec::new();

        for book in books.iter() {
            for lib in &libraries {
                let lib_path = PathBuf::from(&lib.path);
                if let Some(rel) = book.relative_path(&lib_path) {
                    result.push((book.clone(), rel.to_string_lossy().to_string()));
                    break;
                }
            }
        }

        result
    }

    /// Get path to cached cover file.
    fn cover_cache_path(&self, book_id: &str) -> PathBuf {
        self.config
            .cache
            .covers_dir
            .join(format!("{}.jpg", book_id))
    }

    /// Get cover for a book, using cache when available.
    pub fn get_cover(&self, book: &Book) -> Option<Vec<u8>> {
        let cache_path = self.cover_cache_path(&book.id);

        // Try cache first
        if cache_path.exists()
            && let Ok(data) = std::fs::read(&cache_path)
        {
            return Some(data);
        }

        // Extract from file
        let handler = formats::get_handler(book.format);
        let cover_data = handler.extract_cover(&book.path).ok()??;

        // Save to cache
        if let Some(parent) = cache_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&cache_path, &cover_data);

        Some(cover_data)
    }

    /// Check if cover is cached.
    pub fn has_cached_cover(&self, book_id: &str) -> bool {
        self.cover_cache_path(book_id).exists()
    }
}
