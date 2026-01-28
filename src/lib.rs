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

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod formats;
pub mod library;
pub mod opds;
pub mod server;

#[cfg(test)]
mod tests;

pub use config::{Cli, Command, Config};
pub use db::Database;
pub use error::{AppError, Result};
pub use server::AppState;
