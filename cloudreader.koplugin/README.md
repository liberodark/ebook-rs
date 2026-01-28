# CloudReader Plugin for KOReader

Sync your ebook-rs library to KOReader with automatic placeholders and reading data backup.

## Features

- **Library sync** — Browse your full library with PDF placeholders
- **On-demand download** — Books are downloaded when you open them
- **SDR sync** — Backup reading position, highlights, and notes to server
- **Auto-sync** — Syncs on startup and every 30 minutes

## Installation

Copy `cloudreader.koplugin` to your KOReader plugins directory:

- **Kobo**: `/.adds/koreader/plugins/`
- **Kindle**: `/koreader/plugins/`
- **Android**: `/sdcard/koreader/plugins/`
- **Linux**: `~/.config/koreader/plugins/`

Restart KOReader.

## Usage

### Login

1. Menu → Tools → **CloudReader**
2. Tap **Login / Register**
3. Enter server URL: `http://192.168.x.x:8080`
4. Enter username and password
5. Tap **Login** or **Register**

### Browse Library

After login, tap **CloudReader** to open the synced library folder.
- Books appear as PDF placeholders with covers
- Tap a book to download and open it
- Long-press for settings menu

### Settings Menu

Long-press **CloudReader** menu item:
- **Open library folder** — Browse synced books
- **Sync library now** — Manual sync
- **Sync current book progress** — Upload reading position
- **Auto-sync: ON/OFF** — Toggle automatic sync
- **Logout**

## Server

Requires ebook-rs server:

```bash
ebook-rs library add "Books" --path /path/to/books
ebook-rs user add myuser --password mypass
ebook-rs serve
```

## How It Works

1. Plugin syncs library metadata from server
2. Creates lightweight PDF placeholders with covers
3. When you open a placeholder, the full book is downloaded
4. Reading data (.sdr folders) are synced to server
