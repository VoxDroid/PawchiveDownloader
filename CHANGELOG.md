# Changelog

All notable changes to Pawchive Downloader. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.0.1] - 2026-11-18

### Fixed

- The app closed when a download that has to look up posts first was
  started: a whole creator, a selection from a creator or listing, a batch,
  "Download again" from history, favorites, new watchlist posts, and Pause
  all or Resume all from the tray. Background work now always runs on the
  app's own runtime, whatever thread starts it.
- Two transfers could write the same file at once when "Replace it" was
  chosen; a file name another running transfer is using is never reused.
- Files were matched case-sensitively when choosing names, so `A.png` and
  `a.png` could collide on Windows and macOS.
- A server answering with a web page (an error page, a login wall or a
  captive portal) could be saved as the file when no checksum was known.
- Byte ranges are always requested uncompressed, so resumed files cannot be
  corrupted by a compressing proxy.
- With a folder template that puts every post in one folder, each post's
  `content.txt` overwrote the previous one; text files now carry the post id
  in that case.
- Removing a running job kept its file names reserved until the app was
  restarted.
- A job removed, cancelled or cleared just before the app was closed hard
  could come back on the next start; queue changes are now saved at once.

### Changed

- A bug in a single download can no longer close the app: it fails that
  item with a short message instead. A crash report is written next to the
  log file, and the app points to it on the next start.
- Lost connections are waited out (for up to an hour) instead of using up
  the retries and failing the queue; the log says when the app is waiting.
- The download folder is checked before anything is queued, so a missing or
  read-only folder gives one clear message instead of a failure per file.
- Queuing the same download twice (a double click, the same batch again)
  points to the job already in the queue instead of downloading twice.
- A file another job has finished in the meantime is skipped when its turn
  comes, not downloaded again.
- One notification for a batch of downloads instead of one per job, sent
  without holding up the downloads.
- Large queues are saved incrementally: only jobs that changed are written,
  instead of the whole queue every few seconds.
- Progress shows while a creator is still being scanned, and archives show
  "Building archive" while they are written.
- Errors are explained in words (for example "Not on the server (HTTP 404)").
- Ctrl+V anywhere on the Browse page opens a pasted Pawchive link.
- Interface commands run off the window's thread, keeping the window smooth
  during database and file work.
- Opening paths from the interface is limited to folders, and the text
  import and export commands to text file types.

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

[Unreleased]: https://github.com/VoxDroid/PawchiveDownloader/compare/v1.0.1...HEAD
[1.0.1]: https://github.com/VoxDroid/PawchiveDownloader/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/VoxDroid/PawchiveDownloader/releases/tag/v1.0.0
