# Changelog

All notable changes to Pawchive Downloader. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.0.0] - 2026-11-18

The first version of the desktop app, the big sibling of the Pawchive
Downloader browser extension.

### Added

- Browse: paste or drop links to posts, creators and searches; pick files with
  thumbnails, type filters, Shift-click ranges and "hide what I have"; load a
  creator's posts page by page with site search, date range and title filters.
- Downloads as separate files, one ZIP archive per post (ZIP64, optional
  parts), or thumbnails only.
- A Rust download engine: at most five transfers and one new request per
  second, shared site cooldowns on 429 and 503, retries with growing pauses,
  resumable `.part` files with HTTP Range, SHA-256 verification, duplicate
  skipping by content hash, relay links for preview-only files, thumbnail
  fallback, post text and metadata.
- Creator scans that start downloading while still scanning.
- Batch queue and URL import from text files; links passed on the command line
  or by a second launch open in the running app.
- Queue with pause, resume, cancel, retry, per-file retry and reordering,
  saved continuously so it survives crashes.
- History with CSV and JSON export, show in folder and download again.
- Watchlist with scheduled checks, notifications and automatic downloads.
- Favorites with notes, import and export.
- Tray icon, background running, taskbar progress, start at sign-in.
- Proxy support (system, none, custom HTTP/HTTPS/SOCKS5, Tor) and a speed limit.
- Update check that points to the official build on Ko-fi.
- Searchable help guide, live logs with a rotating log file.
- Nine languages, six themes plus Match system, accent colors, reduced motion,
  JetBrains Mono Nerd Font with icons from the font.
- Tests: Rust unit and integration tests against a mock Pawchive, Vitest unit
  and component tests, Playwright interface tests; CI on Windows, macOS and
  Linux.

[Unreleased]: https://github.com/VoxDroid/PawchiveDownloader/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/VoxDroid/PawchiveDownloader/releases/tag/v1.0.0
