# Changelog

All notable changes to Pawchive Downloader. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **Library: an offline gallery of your downloads**: a new page shows the
  images and videos the app downloaded, by creator and post, straight from
  your disk with no site and no network. Search creators and posts, show
  only images or videos, sort, and pick the tile size; video tiles play muted
  when you point at them. Thumbnails are made once (480 px, cached in the
  app's data folder). Files deleted or moved since are left out and counted.
  The window reads the files through a private `pdl-media` protocol that only
  serves files the library recorded, by content hash and never by path, and
  only images and videos, with byte ranges so long videos start at once and
  seek instantly.
- **Library viewer**: arrow keys, a slideshow (Space), a strip of the post's
  files, videos with seeking, and buttons for Open in Browse, Show in folder
  and Open with the default app.
- **Video previews in Browse**: video tiles get a play button and the viewer
  plays them straight from the site's media server; when the server is down
  it says so instead of waiting forever.
- **Zoom and full screen in both viewers**: pictures fit the window and can
  be zoomed with the mouse wheel (toward the pointer), + and -, or the bar
  at the bottom (Fit, actual size 1:1), moved by dragging, and switched with
  a double click. Full screen with the button or F; closing the viewer
  leaves full screen.
- **Back and forward in Browse**: a creator, then a post, then Back returns
  to the creator list exactly as it was, at the same spot, without loading it
  again. A trail under the link bar shows where you came from (creator names
  and post titles) and jumps back to any step; Alt+Left and Alt+Right and the
  mouse's back and forward buttons work too. Up to eight pages stay loaded.
- **Download again**: a download whose files were all there already says so
  ("Nothing new: all 6 files are already in your download folder") and offers
  to fetch them again, replacing them instead of adding numbered copies.
  Job cards count "already downloaded" files apart from other skipped ones.
- **Image viewer previews first**: the viewer shows the thumbnail at once and
  swaps in the full image when it arrives. When a media server is down (as
  Kemono's and Coomer's often are) the preview stays, with a note saying so,
  instead of an endless spinner and then an error.

### Changed

- **Icons sit in the middle of their badges**: the icons were drawn as Nerd
  Font text, whose glyphs are wider than their monospace cell, placed
  differently each and snapped to pixel rows, so the icons in empty pages,
  help topics, settings sections and other badges looked unevenly placed
  (up to 2 px across and 1.5 px down). They are now drawn as SVG from the
  same font's outlines, each centered on its own shape, with the same look
  and size; every badge is centered to within half a pixel
  (`scripts/icon-paths.py`).
- **Creator pages show the whole banner**: the name and the service, id and
  date line moved below the banner instead of covering it; only the avatar
  overlaps its edge, and the banner is a little taller.
- **Browse keeps its place**: going to Downloads, Settings or any other page
  and back to Browse shows the same post or creator, scrolled where you left
  it, instead of starting over.
- **Pairing with the browser extension, redesigned**: Settings > Browser
  extension now says plainly whether a browser is connected ("Connected to
  Microsoft Edge", with a green light) and lists each paired browser by name
  with Connected or when it was last seen. Pairing is three numbered steps
  with a large code, a copy button and a countdown, and the page notices the
  new browser by itself and confirms it. Codes can be cancelled or renewed.
  Unpairing in the extension removes the browser here too.
- **Existing files are skipped by default**: "If a file already exists" now
  defaults to Skip instead of Keep both, so downloading a post again no longer
  fills folders with `name (1).png` copies. Settings still on the old default
  move over once; a choice you made yourself is kept. Skip never drops a
  different file that only shares a name with another in the same download:
  that one still gets a number.
- The settings search box has room below it, so results no longer sit right
  against it.

### Fixed

- Large images and videos in the viewer were cut off instead of fitting the
  window.
- **Previews did not show**: the window's content security policy only let in
  images from `img.pawchive.pw`, so full-size images in the viewer
  (`n1.pawchive.pw` and the other media servers) and every Kemono and Coomer
  thumbnail, icon and banner were blocked. All three sites' hosts are allowed
  now, and a test keeps every supported site in the policy.
- **Deleted files were never downloaded again**: the library remembered every
  file it had saved and skipped it forever, even after it was deleted from the
  folder (the job then said "0 of 999 done" and the only way out was clearing
  the history). Now a file counts as downloaded only while it is still where
  it was saved; deleted or moved files are fetched again by themselves. The
  library keeps its records, so a download drive that is only unplugged does
  not lose them.
- A tooltip could break the page for a moment when the element under it was
  removed (Svelte "state_unsafe_mutation").

### Security

- **Safer ZIP unpacking**: the ZIP library is updated from 2.6.1 to 9.7.3,
  which fixes three problems a crafted archive in a post could exploit while
  being unpacked: headers that disagree could bypass checks and overwrite
  files (critical), symlinks with endless targets could crash the app or use up
  its memory (high), and a truncated encrypted entry could skip its
  authentication check (medium). Building from source now needs Rust 1.88 or
  newer.
- **Safer post text**: HTML entities in post text, file names and Google
  Drive folders are decoded in one pass, so text that shows `&amp;lt;` is no
  longer turned into `<`, and tags are removed until none is left, so pieces
  like `<<b>script>` cannot form a new one. An entity with an impossible
  number (such as `&#99999999;`) no longer breaks the post view.

## [2.0.0] - 2026-11-19

The first public release, and the successor to
[KemonoDownloader](https://github.com/VoxDroid/KemonoDownloader): Pawchive,
Kemono and Coomer in one app, outside hosts and a lot of convenience. Versions
1.0.0 to 1.1.1 were private test builds; the changes since then are below.

### Added

- **Kemono and Coomer** next to Pawchive: links (old kemono.su and
  coomer.su ones too), creator search, recent posts, the watchlist and every
  download work on all three. A new `{site}` name token; the default folder
  template is now `{site}/{creator} ({service})/...` (the old default moves
  over by itself).
- **Outside hosts**: files posts share on Mega (files and folders,
  decrypted on this computer), Google Drive (files and folders), Dropbox,
  Pixeldrain, GoFile, MediaFire and direct links are downloaded with the
  post, or one link at a time from a post's Links tab.
- **Unpacking**: downloaded ZIP archives can be unpacked next to
  themselves, trying the passwords the post mentions.
- **Dead media servers**: before a Kemono or Coomer download starts, one
  tiny request checks the site's media server; when it does not answer, a
  dialog offers to cancel, download thumbnails instead, or queue the full
  files to retry later. During downloads such a server is noticed within
  seconds instead of waiting for the network, and skipped for a while.
- **Automatic retries** of failed files after a chosen time, up to three
  times.
- **Browser bridge**: a paired browser extension (coming soon) can hand
  downloads to the app (loopback only, off by default).
- **Portable mode**: with a `data` folder next to the program everything
  stays there; the portable zip ships with one.
- **Backup and restore** of settings, watchlist, favorites and history.
- **Read markers** for posts opened before.
- **Copied links**: optionally, links copied anywhere are offered for
  opening.
- **Privacy**: blur previews until pointed at, and notifications without
  titles or names.
- **Compact sidebar** (Ctrl+B) and tooltips that are never cut off.
- **Open in Browse** on every download, history entry, favorite and
  watched creator, to see where it came from again.
- **Settings search** (Ctrl+F).
- Help topics for all of the above.

### Fixed

- Recent posts never finished loading: the list restarted itself after
  every page and kept sending requests.
- The link bar's focus ring was cut off at the top of the window.

## [1.1.1] - 2026-11-18

### Fixed

- The link bar on the Browse page stuck flush against the top of the
  window while scrolling, which cut off its focus ring. It now floats a
  little below the edge, and what scrolls behind it fades out.

## [1.1.0] - 2026-11-18

### Added

- Creator search: type a name in the link bar to search every creator on
  Pawchive (fetched at most once a day), filtered by service and sorted by
  match, favorites, update date or name. Creator pages show linked accounts.
- Filters: only posts with chosen words, skip posts or files with others
  (title and tags, or also the text; plain words or `/regex/`), the newest
  N posts, and file types to keep or skip. Defaults in Settings > Filters,
  changeable per download with the new Filters button.
- Remove words such as `[HD]` from titles and file names.
- Links to Mega, Google Drive, Dropbox and other file hosts, and archive
  passwords, are listed on a post's new Links tab and saved to `links.txt`.
- Save comments (with nested replies) and a creator's announcements.
- Download window: start downloads only between two times.
- Downloads wait when the drive is nearly full instead of failing.
- The computer is kept awake while downloads run.
- When done: quit, sleep or shut down after the queue finishes, after a
  cancellable 60 second countdown.
- Retry all failed files at once and export the failed list.
- Statistics page with lifetime totals, monthly chart, top creators and
  files by kind, plus a check that finds deleted or moved files and
  downloads only those again.
- Per-creator watchlist options: folder, format and filters.
- Image viewer in the post view, with arrow keys and selection.
- Help topics for all of the above, and docs/COMPARISON.md.

### Fixed

- Typing in the link bar right after returning to Browse could be replaced
  by the previous link once its lookup finished.

## [1.0.2] - 2026-11-18

### Fixed

- On Linux and macOS, posts with long names in Japanese, Chinese or Korean
  failed with "File name too long": names were limited to 120 characters,
  but these systems allow 255 bytes and such characters take 3 bytes each.
  Names now stay within 200 bytes as well.

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

The first version of the desktop app, a private test build.

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

[Unreleased]: https://github.com/VoxDroid/PawchiveDownloader/compare/v2.0.0...HEAD
[2.0.0]: https://github.com/VoxDroid/PawchiveDownloader/compare/v1.1.1...v2.0.0
[1.1.1]: https://github.com/VoxDroid/PawchiveDownloader/compare/v1.1.0...v1.1.1
[1.1.0]: https://github.com/VoxDroid/PawchiveDownloader/compare/v1.0.2...v1.1.0
[1.0.2]: https://github.com/VoxDroid/PawchiveDownloader/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/VoxDroid/PawchiveDownloader/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/VoxDroid/PawchiveDownloader/releases/tag/v1.0.0
