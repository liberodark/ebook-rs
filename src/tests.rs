use crate::auth::AuthService;
use crate::config::{BookFormat, Config};
use crate::db::{
    Bookmark, Database, Highlight, Library, ReadingProgress, SdrBackup, StoredBook, User,
    now_timestamp,
};

fn test_db() -> Database {
    Database::open_memory().unwrap()
}

fn create_user(db: &Database, id: &str, username: &str) {
    let user = User {
        id: id.to_string(),
        username: username.to_string(),
        password_hash: "hash".to_string(),
        display_name: None,
        role: "user".to_string(),
        created_at: now_timestamp(),
        last_login: None,
    };
    db.create_user(&user).unwrap();
}

fn create_library(db: &Database) {
    let lib = Library {
        id: "lib-1".to_string(),
        name: "Test".to_string(),
        path: "/test".to_string(),
        is_public: true,
        owner_id: None,
        created_at: now_timestamp(),
    };
    db.create_library(&lib).unwrap();
}

fn create_book(db: &Database, id: &str, title: &str) {
    let book = StoredBook {
        id: id.to_string(),
        library_id: "lib-1".to_string(),
        file_hash: None,
        title: title.to_string(),
        author: None,
        authors_json: None,
        description: None,
        publisher: None,
        published: None,
        language: None,
        isbn: None,
        series: None,
        series_index: None,
        tags_json: None,
        path: format!("/test/{}.pdf", id),
        format: "pdf".to_string(),
        file_size: 1000,
        mtime: now_timestamp(),
        page_count: None,
        cover_cached: false,
        created_at: now_timestamp(),
        updated_at: now_timestamp(),
    };
    db.save_book(&book).unwrap();
}

fn setup_user_and_book(db: &Database) {
    create_user(db, "user-1", "testuser");
    create_library(db);
    create_book(db, "book-1", "Test Book");
}

#[test]
fn db_create_and_get_user() {
    let db = test_db();
    let user = User {
        id: "user-1".to_string(),
        username: "alice".to_string(),
        password_hash: "hash".to_string(),
        display_name: Some("Alice".to_string()),
        role: "user".to_string(),
        created_at: now_timestamp(),
        last_login: None,
    };

    db.create_user(&user).unwrap();

    let found = db.get_user_by_username("alice").unwrap().unwrap();
    assert_eq!(found.id, "user-1");
    assert_eq!(found.username, "alice");

    let found_by_id = db.get_user_by_id("user-1").unwrap().unwrap();
    assert_eq!(found_by_id.username, "alice");
}

#[test]
fn db_duplicate_username_fails() {
    let db = test_db();
    let user1 = User {
        id: "user-1".to_string(),
        username: "alice".to_string(),
        password_hash: "hash".to_string(),
        display_name: None,
        role: "user".to_string(),
        created_at: now_timestamp(),
        last_login: None,
    };
    let user2 = User {
        id: "user-2".to_string(),
        username: "alice".to_string(),
        password_hash: "hash2".to_string(),
        display_name: None,
        role: "user".to_string(),
        created_at: now_timestamp(),
        last_login: None,
    };

    db.create_user(&user1).unwrap();
    assert!(db.create_user(&user2).is_err());
}

#[test]
fn db_delete_user() {
    let db = test_db();
    let user = User {
        id: "user-1".to_string(),
        username: "bob".to_string(),
        password_hash: "hash".to_string(),
        display_name: None,
        role: "user".to_string(),
        created_at: now_timestamp(),
        last_login: None,
    };

    db.create_user(&user).unwrap();
    assert!(db.delete_user("bob").unwrap());
    assert!(db.get_user_by_username("bob").unwrap().is_none());
}

#[test]
fn db_create_and_get_session() {
    let db = test_db();
    create_user(&db, "user-1", "testuser");

    let session = crate::db::Session {
        token: "token123".to_string(),
        user_id: "user-1".to_string(),
        device_id: Some("device-1".to_string()),
        expires_at: now_timestamp() + 3600,
    };

    db.create_session(&session).unwrap();

    let found = db.get_session("token123").unwrap().unwrap();
    assert_eq!(found.user_id, "user-1");
    assert_eq!(found.device_id, Some("device-1".to_string()));
}

#[test]
fn db_delete_session() {
    let db = test_db();
    create_user(&db, "user-1", "testuser");

    let session = crate::db::Session {
        token: "token456".to_string(),
        user_id: "user-1".to_string(),
        device_id: None,
        expires_at: now_timestamp() + 3600,
    };

    db.create_session(&session).unwrap();
    db.delete_session("token456").unwrap();
    assert!(db.get_session("token456").unwrap().is_none());
}

#[test]
fn db_create_and_list_libraries() {
    let db = test_db();
    let lib = Library {
        id: "lib-1".to_string(),
        name: "Books".to_string(),
        path: "/path/to/books".to_string(),
        is_public: true,
        owner_id: None,
        created_at: now_timestamp(),
    };

    db.create_library(&lib).unwrap();

    let libs = db.list_libraries().unwrap();
    assert_eq!(libs.len(), 1);
    assert_eq!(libs[0].name, "Books");
}

#[test]
fn db_get_library_by_name() {
    let db = test_db();
    create_user(&db, "user-1", "owner");

    let lib = Library {
        id: "lib-2".to_string(),
        name: "Comics".to_string(),
        path: "/path/to/comics".to_string(),
        is_public: false,
        owner_id: Some("user-1".to_string()),
        created_at: now_timestamp(),
    };

    db.create_library(&lib).unwrap();

    let found = db.get_library_by_name("Comics").unwrap().unwrap();
    assert_eq!(found.id, "lib-2");
    assert!(!found.is_public);
}

#[test]
fn db_save_and_get_book() {
    let db = test_db();
    create_library(&db);

    let book = StoredBook {
        id: "book-1".to_string(),
        library_id: "lib-1".to_string(),
        file_hash: None,
        title: "Test Book".to_string(),
        author: Some("Author".to_string()),
        authors_json: Some(r#"["Author"]"#.to_string()),
        description: None,
        publisher: None,
        published: None,
        language: None,
        isbn: None,
        series: None,
        series_index: None,
        tags_json: None,
        path: "/test/book.epub".to_string(),
        format: "epub".to_string(),
        file_size: 1024,
        mtime: now_timestamp(),
        page_count: Some(100),
        cover_cached: true,
        created_at: now_timestamp(),
        updated_at: now_timestamp(),
    };

    db.save_book(&book).unwrap();

    let found = db.get_book("book-1").unwrap().unwrap();
    assert_eq!(found.title, "Test Book");
    assert_eq!(found.format, "epub");
}

#[test]
fn db_get_library_books() {
    let db = test_db();
    create_library(&db);

    for i in 1..=3 {
        create_book(&db, &format!("book-{}", i), &format!("Book {}", i));
    }

    let books = db.get_library_books("lib-1").unwrap();
    assert_eq!(books.len(), 3);
}

#[test]
fn db_delete_book() {
    let db = test_db();
    create_library(&db);
    create_book(&db, "book-del", "To Delete");

    assert!(db.delete_book("book-del").unwrap());
    assert!(db.get_book("book-del").unwrap().is_none());
}

#[test]
fn db_save_and_get_progress() {
    let db = test_db();
    setup_user_and_book(&db);

    let progress = ReadingProgress {
        id: 0,
        user_id: "user-1".to_string(),
        book_id: "book-1".to_string(),
        device_id: None,
        current_page: Some(50),
        total_pages: Some(200),
        percentage: Some(25.0),
        current_chapter: Some("Chapter 5".to_string()),
        position_data: None,
        status: "reading".to_string(),
        started_at: Some(now_timestamp()),
        finished_at: None,
        updated_at: now_timestamp(),
    };

    db.save_progress(&progress).unwrap();

    let found = db.get_progress("user-1", "book-1").unwrap().unwrap();
    assert_eq!(found.current_page, Some(50));
    assert_eq!(found.percentage, Some(25.0));
}

#[test]
fn db_update_progress() {
    let db = test_db();
    setup_user_and_book(&db);

    let ts = now_timestamp();

    let progress1 = ReadingProgress {
        id: 0,
        user_id: "user-1".to_string(),
        book_id: "book-1".to_string(),
        device_id: Some("device-1".to_string()),
        current_page: Some(10),
        total_pages: Some(100),
        percentage: Some(10.0),
        current_chapter: None,
        position_data: None,
        status: "reading".to_string(),
        started_at: Some(ts),
        finished_at: None,
        updated_at: ts,
    };
    db.save_progress(&progress1).unwrap();

    let progress2 = ReadingProgress {
        id: 0,
        user_id: "user-1".to_string(),
        book_id: "book-1".to_string(),
        device_id: Some("device-1".to_string()),
        current_page: Some(80),
        total_pages: Some(100),
        percentage: Some(80.0),
        current_chapter: None,
        position_data: None,
        status: "reading".to_string(),
        started_at: Some(ts),
        finished_at: None,
        updated_at: ts + 1,
    };
    db.save_progress(&progress2).unwrap();

    let found = db.get_progress("user-1", "book-1").unwrap().unwrap();
    assert_eq!(found.current_page, Some(80));
}

#[test]
fn db_save_and_get_highlights() {
    let db = test_db();
    setup_user_and_book(&db);

    let highlight = Highlight {
        id: "hl-1".to_string(),
        user_id: "user-1".to_string(),
        book_id: "book-1".to_string(),
        device_id: None,
        page: Some(42),
        chapter: Some("Ch 3".to_string()),
        text: "Important text".to_string(),
        note: Some("My note".to_string()),
        color: "yellow".to_string(),
        pos0: None,
        pos1: None,
        created_at: now_timestamp(),
        updated_at: now_timestamp(),
    };

    db.save_highlight(&highlight).unwrap();

    let highlights = db.get_highlights("user-1", "book-1").unwrap();
    assert_eq!(highlights.len(), 1);
    assert_eq!(highlights[0].text, "Important text");
}

#[test]
fn db_delete_highlight() {
    let db = test_db();
    setup_user_and_book(&db);

    let highlight = Highlight {
        id: "hl-del".to_string(),
        user_id: "user-1".to_string(),
        book_id: "book-1".to_string(),
        device_id: None,
        page: None,
        chapter: None,
        text: "Delete me".to_string(),
        note: None,
        color: "red".to_string(),
        pos0: None,
        pos1: None,
        created_at: now_timestamp(),
        updated_at: now_timestamp(),
    };

    db.save_highlight(&highlight).unwrap();
    db.delete_highlight("hl-del", "user-1").unwrap();

    let highlights = db.get_highlights("user-1", "book-1").unwrap();
    assert!(highlights.is_empty());
}

#[test]
fn db_save_and_get_bookmarks() {
    let db = test_db();
    setup_user_and_book(&db);

    let bookmark = Bookmark {
        id: "bm-1".to_string(),
        user_id: "user-1".to_string(),
        book_id: "book-1".to_string(),
        page: Some(100),
        position_data: None,
        name: Some("Important part".to_string()),
        created_at: now_timestamp(),
    };

    db.save_bookmark(&bookmark).unwrap();

    let bookmarks = db.get_bookmarks("user-1", "book-1").unwrap();
    assert_eq!(bookmarks.len(), 1);
    assert_eq!(bookmarks[0].name, Some("Important part".to_string()));
}

#[test]
fn db_save_and_get_sdr() {
    let db = test_db();
    setup_user_and_book(&db);

    let sdr = SdrBackup {
        user_id: "user-1".to_string(),
        book_id: "book-1".to_string(),
        data: vec![1, 2, 3, 4, 5],
        last_page: Some(50),
        percent_finished: Some(0.25),
        updated_at: now_timestamp(),
    };

    db.save_sdr(&sdr).unwrap();

    let found = db.get_sdr("user-1", "book-1").unwrap().unwrap();
    assert_eq!(found.data, vec![1, 2, 3, 4, 5]);
    assert_eq!(found.last_page, Some(50));
}

#[test]
fn db_get_sdr_list() {
    let db = test_db();
    create_user(&db, "user-1", "testuser");
    create_library(&db);

    for i in 1..=3 {
        create_book(&db, &format!("book-{}", i), &format!("Book {}", i));
        let sdr = SdrBackup {
            user_id: "user-1".to_string(),
            book_id: format!("book-{}", i),
            data: vec![i as u8],
            last_page: Some(i * 10),
            percent_finished: Some(i as f64 * 0.1),
            updated_at: now_timestamp(),
        };
        db.save_sdr(&sdr).unwrap();
    }

    let list = db.get_user_sdr_list("user-1").unwrap();
    assert_eq!(list.len(), 3);
}

#[test]
fn auth_create_user_and_login() {
    let db = test_db();
    let auth = AuthService::new(db, 30, true);

    let user = auth.create_user("testuser", "password123", "user").unwrap();
    assert_eq!(user.username, "testuser");
    assert_eq!(user.role, "user");

    let (logged_in, token) = auth.login("testuser", "password123", None).unwrap();
    assert_eq!(logged_in.username, "testuser");
    assert!(!token.is_empty());
}

#[test]
fn auth_validate_token() {
    let db = test_db();
    let auth = AuthService::new(db, 30, true);

    auth.create_user("alice", "pass1234", "admin").unwrap();
    let (_, token) = auth.login("alice", "pass1234", None).unwrap();

    let user = auth.validate_token(&token).unwrap().unwrap();
    assert_eq!(user.username, "alice");

    assert!(auth.validate_token("invalid_token").unwrap().is_none());
}

#[test]
fn auth_logout() {
    let db = test_db();
    let auth = AuthService::new(db, 30, true);

    auth.create_user("bob", "password", "user").unwrap();
    let (_, token) = auth.login("bob", "password", None).unwrap();

    auth.logout(&token).unwrap();
    assert!(auth.validate_token(&token).unwrap().is_none());
}

#[test]
fn auth_registration_disabled() {
    let db = test_db();
    let auth = AuthService::new(db, 30, false);

    let result = auth.register("newuser", "password");
    assert!(result.is_err());
}

#[test]
fn auth_invalid_password() {
    let db = test_db();
    let auth = AuthService::new(db, 30, true);

    auth.create_user("user", "correct", "user").unwrap();
    let result = auth.login("user", "wrong", None);
    assert!(result.is_err());
}

#[test]
fn auth_change_password() {
    let db = test_db();
    let auth = AuthService::new(db, 30, true);

    auth.create_user("user", "oldpass", "user").unwrap();
    auth.change_password("user", "newpass").unwrap();

    assert!(auth.login("user", "oldpass", None).is_err());
    assert!(auth.login("user", "newpass", None).is_ok());
}

#[test]
fn auth_short_password_rejected() {
    let db = test_db();
    let auth = AuthService::new(db, 30, true);

    let result = auth.create_user("user", "abc", "user");
    assert!(result.is_err());
}

#[test]
fn auth_invalid_username_rejected() {
    let db = test_db();
    let auth = AuthService::new(db, 30, true);

    assert!(auth.create_user("user@email", "password", "user").is_err());
    assert!(auth.create_user("user name", "password", "user").is_err());
    assert!(auth.create_user("", "password", "user").is_err());
}

#[test]
fn config_parse_toml() {
    let toml = r#"
[server]
bind = "127.0.0.1:9090"
title = "Test Library"

[database]
path = "/tmp/test.db"

[auth]
registration = "disabled"
session_days = 7

[scan]
interval_seconds = 600
workers = 2
"#;
    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.server.bind.port(), 9090);
    assert_eq!(config.server.title, "Test Library");
    assert!(!config.auth.registration_enabled());
    assert_eq!(config.auth.session_days, 7);
    assert_eq!(config.scan.interval_seconds, 600);
    assert_eq!(config.scan.workers, 2);
}

#[test]
fn config_default_values() {
    let config = Config::default();
    assert_eq!(config.server.bind.port(), 8080);
    assert!(config.auth.registration_enabled());
    assert_eq!(config.scan.workers, 1);
}

#[test]
fn book_format_from_extension() {
    assert_eq!(BookFormat::from_extension("epub"), Some(BookFormat::Epub));
    assert_eq!(BookFormat::from_extension("PDF"), Some(BookFormat::Pdf));
    assert_eq!(BookFormat::from_extension("cbz"), Some(BookFormat::Cbz));
    assert_eq!(BookFormat::from_extension("CBR"), Some(BookFormat::Cbr));
    assert_eq!(BookFormat::from_extension("mobi"), Some(BookFormat::Mobi));
    assert_eq!(BookFormat::from_extension("fb2"), Some(BookFormat::Fb2));
    assert_eq!(BookFormat::from_extension("unknown"), None);
}

#[test]
fn book_format_mime_type() {
    assert_eq!(BookFormat::Epub.mime_type(), "application/epub+zip");
    assert_eq!(BookFormat::Pdf.mime_type(), "application/pdf");
    assert_eq!(BookFormat::Cbz.mime_type(), "application/vnd.comicbook+zip");
    assert_eq!(BookFormat::Cbr.mime_type(), "application/vnd.comicbook-rar");
}

#[test]
fn db_expired_sessions_cleanup() {
    let db = test_db();
    create_user(&db, "user-1", "testuser");

    let expired = crate::db::Session {
        token: "expired".to_string(),
        user_id: "user-1".to_string(),
        device_id: None,
        expires_at: now_timestamp() - 3600,
    };
    let valid = crate::db::Session {
        token: "valid".to_string(),
        user_id: "user-1".to_string(),
        device_id: None,
        expires_at: now_timestamp() + 3600,
    };

    db.create_session(&expired).unwrap();
    db.create_session(&valid).unwrap();

    db.cleanup_expired_sessions().unwrap();

    assert!(db.get_session("expired").unwrap().is_none());
    assert!(db.get_session("valid").unwrap().is_some());
}

#[test]
fn db_sdr_update_replaces_data() {
    let db = test_db();
    setup_user_and_book(&db);

    let sdr1 = SdrBackup {
        user_id: "user-1".to_string(),
        book_id: "book-1".to_string(),
        data: vec![1, 2, 3],
        last_page: Some(10),
        percent_finished: Some(0.1),
        updated_at: now_timestamp(),
    };
    db.save_sdr(&sdr1).unwrap();

    let sdr2 = SdrBackup {
        user_id: "user-1".to_string(),
        book_id: "book-1".to_string(),
        data: vec![4, 5, 6, 7],
        last_page: Some(50),
        percent_finished: Some(0.5),
        updated_at: now_timestamp(),
    };
    db.save_sdr(&sdr2).unwrap();

    let found = db.get_sdr("user-1", "book-1").unwrap().unwrap();
    assert_eq!(found.data, vec![4, 5, 6, 7]);
    assert_eq!(found.last_page, Some(50));
}

#[test]
fn db_delete_sdr() {
    let db = test_db();
    setup_user_and_book(&db);

    let sdr = SdrBackup {
        user_id: "user-1".to_string(),
        book_id: "book-1".to_string(),
        data: vec![1],
        last_page: None,
        percent_finished: None,
        updated_at: now_timestamp(),
    };
    db.save_sdr(&sdr).unwrap();

    assert!(db.delete_sdr("user-1", "book-1").unwrap());
    assert!(db.get_sdr("user-1", "book-1").unwrap().is_none());
}

#[test]
fn db_get_all_books() {
    let db = test_db();
    create_library(&db);

    create_book(&db, "book-a", "Alpha");
    create_book(&db, "book-b", "Beta");

    let books = db.get_all_books().unwrap();
    assert_eq!(books.len(), 2);
}

#[test]
fn auth_is_admin() {
    let db = test_db();
    let auth = AuthService::new(db, 30, true);

    let admin = auth.create_user("admin", "password", "admin").unwrap();
    let user = auth.create_user("user", "password", "user").unwrap();

    assert!(auth.is_admin(&admin));
    assert!(!auth.is_admin(&user));
}
