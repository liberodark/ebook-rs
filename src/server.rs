//! HTTP server and routes.

mod handlers;
mod state;

pub use state::AppState;

use axum::{
    Router,
    routing::{delete, get, post, put},
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

/// Create the application router.
pub fn create_router(state: AppState) -> Router {
    let catalog_routes = Router::new()
        .route("/", get(handlers::catalog_root))
        .route("/recent", get(handlers::catalog_recent))
        .route("/all", get(handlers::catalog_all))
        .route("/search", get(handlers::catalog_search));

    let book_routes = Router::new()
        .route("/{id}", get(handlers::book_metadata))
        .route("/{id}/download", get(handlers::book_download))
        .route(
            "/{id}/download.{ext}",
            get(handlers::book_download_with_ext),
        )
        .route("/{id}/cover", get(handlers::book_cover))
        .route("/{id}/thumbnail", get(handlers::book_thumbnail))
        .route("/{id}/placeholder", get(handlers::book_placeholder));

    let auth_routes = Router::new()
        .route("/login", post(handlers::auth_login))
        .route("/register", post(handlers::auth_register))
        .route("/logout", post(handlers::auth_logout))
        .route("/me", get(handlers::auth_me));

    let sync_routes = Router::new()
        // Progress by book
        .route("/progress/{book_id}", get(handlers::sync_get_progress))
        .route("/progress/{book_id}", put(handlers::sync_update_progress))
        // Highlights by book
        .route(
            "/book/{book_id}/highlights",
            get(handlers::sync_get_highlights),
        )
        .route(
            "/book/{book_id}/highlights",
            post(handlers::sync_add_highlight),
        )
        // Highlight by ID
        .route("/highlight/{id}", delete(handlers::sync_delete_highlight))
        // Bookmarks by book
        .route(
            "/book/{book_id}/bookmarks",
            get(handlers::sync_get_bookmarks),
        )
        .route(
            "/book/{book_id}/bookmarks",
            post(handlers::sync_add_bookmark),
        )
        // Bookmark by ID
        .route("/bookmark/{id}", delete(handlers::sync_delete_bookmark))
        // SDR backups (KOReader .sdr folders)
        .route("/sdr", get(handlers::sync_get_sdr_list))
        .route("/sdr/{book_id}", get(handlers::sync_download_sdr))
        .route("/sdr/{book_id}", put(handlers::sync_upload_sdr))
        .route("/sdr/{book_id}/info", get(handlers::sync_get_sdr_info));

    let api_routes = Router::new()
        .route("/scan", post(handlers::api_scan))
        .route("/stats", get(handlers::api_stats))
        .route("/library", get(handlers::api_library));

    Router::new()
        .route("/", get(handlers::index))
        .route("/opensearch.xml", get(handlers::opensearch))
        .nest("/catalog", catalog_routes)
        .nest("/books", book_routes)
        .nest("/api/auth", auth_routes)
        .nest("/api/sync", sync_routes)
        .nest("/api", api_routes)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
