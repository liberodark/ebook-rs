use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

/// OPDS server for ebooks and comics with reading sync.
#[derive(Parser, Debug, Clone)]
#[command(name = "ebook-rs")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Path to config file.
    #[arg(short, long, env = "EBOOK_CONFIG", global = true)]
    pub config: Option<PathBuf>,

    /// Subcommand to run.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// CLI subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Start the server (default if no command given).
    Serve {
        /// Address to bind the server to.
        #[arg(short, long)]
        bind: Option<SocketAddr>,

        /// Path to library directory (legacy mode without config file).
        #[arg(short, long)]
        library: Option<PathBuf>,
    },

    /// User management commands.
    User {
        /// User subcommand action.
        #[command(subcommand)]
        action: UserCommand,
    },

    /// Library management commands.
    Library {
        /// Library subcommand action.
        #[command(subcommand)]
        action: LibraryCommand,
    },

    /// Initialize database and create default config.
    Init {
        /// Force overwrite existing config.
        #[arg(short, long)]
        force: bool,
    },
}

/// User management subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum UserCommand {
    /// Add a new user.
    Add {
        /// Username.
        username: String,
        /// Password (will prompt if not provided).
        #[arg(short, long)]
        password: Option<String>,
        /// User role (admin or user).
        #[arg(short, long, default_value = "user")]
        role: String,
    },

    /// Delete a user.
    Del {
        /// Username to delete.
        username: String,
    },

    /// List all users.
    List,

    /// Change user password.
    Passwd {
        /// Username.
        username: String,
        /// New password (will prompt if not provided).
        #[arg(short, long)]
        password: Option<String>,
    },
}

/// Library management subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum LibraryCommand {
    /// Add a new library.
    Add {
        /// Library name.
        name: String,
        /// Path to library directory.
        #[arg(short, long)]
        path: PathBuf,
        /// Make library public (accessible to all users).
        #[arg(long, default_value = "true")]
        public: bool,
    },

    /// Remove a library.
    Del {
        /// Library name.
        name: String,
    },

    /// List all libraries.
    List,

    /// Scan libraries for new books.
    Scan {
        /// Scan all libraries.
        #[arg(long)]
        all: bool,
        /// Specific library name.
        name: Option<String>,
    },
}

/// Main configuration from TOML file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Server configuration.
    #[serde(default)]
    pub server: ServerConfig,

    /// Database configuration.
    #[serde(default)]
    pub database: DatabaseConfig,

    /// Authentication configuration.
    #[serde(default)]
    pub auth: AuthConfig,

    /// Sync configuration.
    #[serde(default)]
    pub sync: SyncConfig,

    /// Scan configuration.
    #[serde(default)]
    pub scan: ScanConfig,

    /// Cache configuration.
    #[serde(default)]
    pub cache: CacheConfig,

    /// Libraries to serve.
    #[serde(default)]
    pub libraries: Vec<LibraryConfig>,
}

/// Library configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryConfig {
    /// Library name.
    pub name: String,

    /// Path to library directory.
    pub path: PathBuf,

    /// Whether library is public (accessible to all users).
    #[serde(default = "default_public")]
    pub public: bool,
}

fn default_public() -> bool {
    true
}

/// Server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Address to bind to.
    #[serde(default = "default_bind")]
    pub bind: SocketAddr,

    /// Catalog title.
    #[serde(default = "default_title")]
    pub title: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            title: default_title(),
        }
    }
}

fn default_bind() -> SocketAddr {
    SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)),
        8080,
    )
}

fn default_title() -> String {
    "My Library".to_string()
}

/// Database configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Path to SQLite database file.
    #[serde(default = "default_db_path")]
    pub path: PathBuf,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: default_db_path(),
        }
    }
}

fn default_db_path() -> PathBuf {
    PathBuf::from("data/library.db")
}

/// Authentication configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Registration mode: "open", "disabled".
    #[serde(default = "default_registration")]
    pub registration: String,

    /// Session token duration in days.
    #[serde(default = "default_session_days")]
    pub session_days: u32,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            registration: default_registration(),
            session_days: default_session_days(),
        }
    }
}

fn default_registration() -> String {
    "open".to_string()
}

fn default_session_days() -> u32 {
    30
}

impl AuthConfig {
    /// Check if registration is enabled.
    pub fn registration_enabled(&self) -> bool {
        self.registration == "open"
    }
}

/// Sync configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Merge strategy: "latest", "furthest", "per_device".
    #[serde(default = "default_merge_strategy")]
    pub merge_strategy: String,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            merge_strategy: default_merge_strategy(),
        }
    }
}

fn default_merge_strategy() -> String {
    "furthest".to_string()
}

/// Scan configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    /// Rescan interval in seconds (0 to disable).
    #[serde(default = "default_scan_interval")]
    pub interval_seconds: u64,

    /// Number of parallel workers for metadata extraction (1 = sequential).
    /// Keep low for NAS/network storage to avoid saturation.
    #[serde(default = "default_scan_workers")]
    pub workers: usize,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            interval_seconds: default_scan_interval(),
            workers: default_scan_workers(),
        }
    }
}

fn default_scan_interval() -> u64 {
    300
}

fn default_scan_workers() -> usize {
    1 // Sequential by default - safe for NAS/Raspberry Pi
}

/// Cache configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Directory for cached covers.
    #[serde(default = "default_cache_dir")]
    pub covers_dir: PathBuf,

    /// Thumbnail size in pixels.
    #[serde(default = "default_thumbnail_size")]
    pub thumbnail_size: u32,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            covers_dir: default_cache_dir(),
            thumbnail_size: default_thumbnail_size(),
        }
    }
}

fn default_cache_dir() -> PathBuf {
    PathBuf::from("data/covers")
}

fn default_thumbnail_size() -> u32 {
    200
}

impl Config {
    /// Load configuration from file.
    pub fn load(path: &PathBuf) -> crate::error::Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            crate::error::AppError::Config(format!("Failed to read config file: {}", e))
        })?;

        toml::from_str(&content).map_err(|e| {
            crate::error::AppError::Config(format!("Failed to parse config file: {}", e))
        })
    }

    /// Find config file in default locations.
    pub fn find_config_file() -> Option<PathBuf> {
        let candidates = [
            PathBuf::from("config.toml"),
            PathBuf::from("ebook-rs.toml"),
            dirs::config_dir()
                .map(|p| p.join("ebook-rs").join("config.toml"))
                .unwrap_or_default(),
            PathBuf::from("/etc/ebook-rs/config.toml"),
        ];

        candidates.into_iter().find(|p| p.exists())
    }

    /// Generate default config file content.
    pub fn generate_default() -> String {
        r#"# ebook-rs configuration

[server]
bind = "0.0.0.0:8080"
title = "My Library"

[database]
# path = "/var/lib/ebook-rs/library.db"

[auth]
# Registration mode: "open" or "disabled"
registration = "open"
# Session duration in days
session_days = 30

[sync]
# Merge strategy: "latest", "furthest", "per_device"
merge_strategy = "furthest"

[scan]
# Rescan interval in seconds (0 to disable)
interval_seconds = 300

[cache]
# covers_dir = "/var/lib/ebook-rs/covers"
thumbnail_size = 200

# Libraries to serve (optional - can also use CLI)
# [[libraries]]
# name = "Mangas"
# path = "/mnt/nas/Ebook/Mangas"
# public = true

# [[libraries]]
# name = "Romans"
# path = "/mnt/nas/Ebook/Romans"
"#
        .to_string()
    }
}

/// Supported book formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BookFormat {
    /// EPUB format (Electronic Publication).
    Epub,
    /// PDF format (Portable Document Format).
    Pdf,
    /// CBZ format (Comic Book ZIP archive).
    Cbz,
    /// CBR format (Comic Book RAR archive).
    Cbr,
    /// CB7 format (Comic Book 7-Zip archive).
    Cb7,
    /// MOBI format (Mobipocket eBook).
    Mobi,
    /// FB2 format (FictionBook).
    Fb2,
    /// Plain text format.
    Txt,
    /// HTML format.
    Html,
    /// Markdown format.
    Md,
}

impl BookFormat {
    /// Get the MIME type for this format.
    pub fn mime_type(&self) -> &'static str {
        match self {
            BookFormat::Epub => "application/epub+zip",
            BookFormat::Pdf => "application/pdf",
            BookFormat::Cbz => "application/vnd.comicbook+zip",
            BookFormat::Cbr => "application/vnd.comicbook-rar",
            BookFormat::Cb7 => "application/x-cb7",
            BookFormat::Mobi => "application/x-mobipocket-ebook",
            BookFormat::Fb2 => "application/x-fictionbook+xml",
            BookFormat::Txt => "text/plain",
            BookFormat::Html => "text/html",
            BookFormat::Md => "text/markdown",
        }
    }

    /// Try to detect format from file extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "epub" => Some(BookFormat::Epub),
            "pdf" => Some(BookFormat::Pdf),
            "cbz" => Some(BookFormat::Cbz),
            "cbr" => Some(BookFormat::Cbr),
            "cb7" => Some(BookFormat::Cb7),
            "mobi" | "azw" | "azw3" => Some(BookFormat::Mobi),
            "fb2" => Some(BookFormat::Fb2),
            "txt" => Some(BookFormat::Txt),
            "html" | "htm" => Some(BookFormat::Html),
            "md" | "markdown" => Some(BookFormat::Md),
            _ => None,
        }
    }

    /// Check if this format is a comic book archive.
    pub fn is_comic(&self) -> bool {
        matches!(self, BookFormat::Cbz | BookFormat::Cbr | BookFormat::Cb7)
    }
}
