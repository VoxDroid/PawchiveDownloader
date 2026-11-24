# Architecture

Pawchive Downloader is a [Tauri 2](https://tauri.app) app. A Rust core does
everything that touches the network or the disk; a Svelte 5 interface runs in
the system webview (WebView2 on Windows, WKWebView on macOS, WebKitGTK on
Linux) and talks to the core through typed commands and events.

```
 Svelte interface (src/)                   Rust core (src-tauri/src/)
 ------------------------                  --------------------------
 features/* pages      --- invoke ---->    commands.rs
 lib/api.ts            <--- events ----    app.rs (live updates, tray, notifications)
                                           core.rs  wires everything together
                                             |-- jobs/engine.rs   queue and scheduler
                                             |     |-- jobs/plan.rs      posts -> items
                                             |     |-- jobs/transfer.rs  resumable fetches
                                             |     `-- jobs/archive.rs   ZIP64 writer
                                             |-- pawchive/        API client, normalizing, relay links
                                             |-- net/             HTTP client, limiter, speed limit
                                             |-- naming/          templates, safe file names
                                             |-- store/           SQLite library
                                             |-- watch.rs         watchlist scheduler
                                             `-- updates.rs       version check
```

## The Rust core

**`pawchive/`** knows the site. `urls.rs` parses links and builds API, page,
file and thumbnail URLs; `client.rs` fetches JSON (Pawchive serves it only to
requests that send `Accept: text/css`) with retries; `normalize.rs` turns API
posts into one model, de-duplicating the main file, attachments and inline
images by content hash; `relay.rs` scrapes the temporary links that post pages
offer for preview-only files.

Facts this relies on, verified against the live site:

- Each file has its own media server (`n1` to `n4.pawchive.pw`); the full URL
  is `server + "/data" + path + "?f=" + name`.
- The file name in the path is the SHA-256 of the content, so downloads can be
  verified.
- Media servers support HTTP Range requests, so downloads can be resumed.
- Listings (`/posts?o=`) page by 50 and leave out some attachments (marked
  `deferred`); the full post has them, so creator scans fetch full posts when
  the listing is incomplete or post text is wanted.

**`net/limiter.rs`** is the politeness policy, shared by every request: at most
five transfers at once, at least one second between new requests, and a site
cooldown when Pawchive answers 429 (or 503 with Retry-After). Turns are handed
out first come, first served. It runs on tokio's clock, so its tests use paused
time instead of waiting.

**`jobs/`** is the download engine. A _job_ is what the user asked for (posts,
a list of post references, or a creator with filters); _items_ are the files,
texts and archives that make it up.

- `plan.rs` turns a post into items according to the job's options (mode,
  file types, templates, preview-only handling, duplicates).
- `engine.rs` owns the queue. A scheduler loop starts pending items in queue
  order up to the concurrency setting; each transfer runs in its own task and
  reports an outcome that `settle` applies: done, skipped, retry later (with a
  growing pause), fall back to the thumbnail, or failed. Creator jobs are
  _resolved_ in the background page by page, and their items are planned as
  each page arrives, so downloads start before the scan ends.
- `transfer.rs` downloads into `<target>.part`, sends `Range` when a part file
  exists, verifies the checksum and renames the file into place. The target
  name is decided once and stored with the item, so a resumed download writes
  to the same file.
- `archive.rs` writes ZIP64 archives from files fetched into a staging folder
  (which also makes archive entries resumable).

Progress is read from atomic counters, so transfers never lock the queue for
every chunk. The queue is saved to SQLite every two seconds while it changes,
and on exit; anything that was transferring when the app stopped goes back to
pending and resumes from its part file.

**`store/`** is one SQLite database, `library.db`: settings, the queue, the
history, the hashes of downloaded files and posts, the watchlist and
favorites.

**`gallery.rs`** is the Library: it groups the recorded files that are still
on disk into posts and creators (names from the history, else from the folder
names), and adds the images and videos a scan of the download folders finds
without a record, placed by the folder names the app writes
(`Site/Creator (service)/date Title [post id]/`) and matched to known
creators by name. It serves them to the window through the `pdl-media`
protocol (`app.rs` registers it). The protocol takes a content hash (recorded
files) or an id the last scan gave a found file (from its path, size and
time), never a path, so it can only hand out those files, and only images and
videos.
Videos are served in byte ranges (at most 4 MB per answer), so they start and
seek without being read whole. Thumbnails are made on first request with the
`image` crate, at most three at a time, and cached in `thumbs/`. The content
security policy allows `pdl-media:` for `img-src` and `media-src`, next to the
three sites' hosts; a test keeps every site in it.

**`watch.rs`** checks followed creators on the interval from Settings; the
first check of a creator only records a baseline.

**`app.rs`** is the desktop shell: the tray menu, close-to-tray, notifications
(translated into the nine languages on the Rust side), taskbar progress and a
loop that pushes job summaries to the interface about four times a second
while anything changes.

## The interface

`src/lib/api.ts` is the only place that talks to Rust. In a plain browser it
loads `src/lib/mock.ts` instead, a demo backend with sample data, which is how
the Playwright tests and the README screenshots run.

`src/ui/` is the design system shared with the browser extension: theme
tokens, components and the icons: SVG outlines taken from the Nerd Font
(`scripts/icon-paths.py`), each centered on its own shape, so they sit evenly
in badges at any size, which the font glyphs did not.
`src/features/` holds one folder per page. State lives in small rune stores in
`src/ui/state/`.

Translations are runtime dictionaries in `src/lib/i18n/locales/`, generated
from compact sources in `scripts/i18n-src/` (see
[TRANSLATING.md](TRANSLATING.md)).

## Testing

| Layer                          | Tool                           | Where                                   |
| ------------------------------ | ------------------------------ | --------------------------------------- |
| Rust units                     | `cargo test`                   | `#[cfg(test)]` modules next to the code |
| Engine end to end              | `cargo test` + wiremock        | `src-tauri/tests/engine.rs`             |
| Interface logic and components | Vitest + Testing Library       | `tests/unit/`                           |
| Interface flows                | Playwright on the demo backend | `tests/e2e/`                            |

The integration tests run the real engine against a mock Pawchive: whole
creators, Range resume, rate limits, checksum failures, relay links, ZIP output,
pause, resume and cancel, and the watchlist.

## Builds and releases

CI (`.github/workflows/ci.yml`) lints, type-checks and tests on Windows, macOS
and Linux. Installers are built in a separate private repository that checks
out a version tag of this one; those official builds are offered on Ko-fi and
report themselves as official (`PAWCHIVE_BUILD_CHANNEL=official` at build
time). Anyone can build the app from this repository.
