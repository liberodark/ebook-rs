# ebook-rs

A lightweight OPDS server for ebooks and comics with reading synchronization.

## Features

- **OPDS 1.2 catalog** — Compatible with KOReader, Calibre, and other readers
- **CloudReader sync** — KOReader plugin for library sync with placeholders
- **SDR backup** — Sync KOReader reading data (.sdr folders) across devices
- **Reading progress** — Synchronize progress, highlights, and bookmarks
- **Multi-user support** — Each user has their own reading data
- **Multiple formats** — EPUB, PDF, CBZ, CBR, MOBI, FB2, JPEG XL
- **Incremental scanning** — Fast startup with SQLite cache, background updates
- **SQLite storage** — No external database required

## Quick Start

```bash
# Initialize config and database
ebook-rs init

# Add a library
ebook-rs library add "My Books" --path /path/to/books

# Create admin user
ebook-rs user add admin --password secret --role admin

# Start server
ebook-rs serve
```

## Installation

```bash
cargo build --release
cargo install --path .
```

## Configuration

```toml
[server]
bind = "0.0.0.0:8080"
title = "My Library"

[database]
path = "data/library.db"

[auth]
registration = "open"  # or "disabled"
session_days = 30

[scan]
interval_seconds = 300  # 0 to disable auto-scan
workers = 1             # parallel workers (1 = sequential, safe for NAS)

[cache]
thumbnail_size = 200
```

## CLI Commands

```bash
# Server
ebook-rs serve
ebook-rs serve --bind 0.0.0.0:80

# User management
ebook-rs user add <username> --password <pass> --role <user|admin>
ebook-rs user del <username>
ebook-rs user list
ebook-rs user passwd <username>

# Library management
ebook-rs library add <n> --path /path/to/books [--public]
ebook-rs library del <n>
ebook-rs library list
```

## API Endpoints

### OPDS Catalog

```
GET  /catalog                 # Root catalog
GET  /catalog/recent          # Recent books
GET  /catalog/all             # All books
GET  /catalog/search?q=...    # Search
GET  /books/{id}/download     # Download book
GET  /books/{id}/cover        # Cover image
GET  /books/{id}/placeholder  # PDF placeholder (for CloudReader)
```

### Authentication

```
POST /api/auth/login          # Login
POST /api/auth/register       # Register (if enabled)
POST /api/auth/logout         # Logout
```

### CloudReader Sync

```
GET  /api/library             # Full library listing with paths

GET  /api/sync/sdr            # List user's SDR backups
GET  /api/sync/sdr/{book_id}  # Download SDR (tar.gz)
PUT  /api/sync/sdr/{book_id}  # Upload SDR (tar.gz)

GET  /api/sync/progress/{book_id}  # Get reading progress
PUT  /api/sync/progress/{book_id}  # Update progress
```

## KOReader Setup

### OPDS Catalog

1. File Browser → Search (🔍) → OPDS Catalog
2. Press **+** to add catalog
3. URL: `http://<server-ip>:8080/catalog`

### CloudReader Plugin

Install `cloudreader.koplugin` for full sync:
- Automatic library sync with PDF placeholders
- Download books on demand
- SDR folder sync (reading position, highlights, notes)
- Auto-sync on startup

## Supported Formats

| Format | Metadata | Cover |
|--------|----------|-------|
| EPUB   | ✅       | ✅    |
| PDF    | ✅       | ✅    |
| CBZ    | ✅       | ✅    |
| CBR    | ⚠️       | ⚠️    |
| MOBI   | ⚠️       | ⚠️    |
| FB2    | ⚠️       | ⚠️    |
