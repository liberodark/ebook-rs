//! ebook-rs: A lightweight OPDS server for ebooks and comics with reading sync.
//!
//! This crate provides an OPDS 1.2 compatible server that can serve
//! ebooks (EPUB, PDF, MOBI) and comics (CBZ, CBR) to e-readers like
//! KOReader, with reading progress synchronization.
//!
//! # Features
//!
//! - OPDS 1.2 catalog support
//! - User accounts and authentication
//! - Reading progress synchronization
//! - Highlights and bookmarks sync
//! - Automatic metadata extraction
//! - Cover image extraction and thumbnails
//! - JPEG XL support for comic images
//! - Search functionality
//! - Category navigation based on directory structure

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Authentication and user management.
pub mod auth;
/// Configuration and CLI.
pub mod config;
/// Database operations.
pub mod db;
/// Error types.
pub mod error;
/// Book format handlers.
pub mod formats;
/// Library and book models.
pub mod library;
/// OPDS feed generation.
pub mod opds;
/// HTTP server.
pub mod server;

#[cfg(test)]
mod tests;

pub use config::{Cli, Command, Config};
pub use db::Database;
pub use error::{AppError, Result};
pub use server::AppState;
