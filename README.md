<h1 align="center">Pawchive Downloader</h1>

<p align="center">
  <img src="assets/icon-512.png" alt="Pawchive Downloader" width="120" />
</p>

<p align="center">
  <strong>The complete desktop downloader for <a href="https://pawchive.pw">Pawchive</a>, <a href="https://kemono.cr">Kemono</a> and <a href="https://coomer.st">Coomer</a>.</strong><br />
  Posts, attachments, whole creators, and the Mega, Google Drive and Dropbox links they share, on Windows, macOS and Linux.
</p>

<p align="center">
  <a href="https://github.com/VoxDroid/PawchiveDownloader/actions/workflows/ci.yml"><img src="https://github.com/VoxDroid/PawchiveDownloader/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="License" /></a>
  <img src="https://img.shields.io/badge/Windows%20%7C%20macOS%20%7C%20Linux-desktop-5aa9ff" alt="Platforms" />
  <img src="https://img.shields.io/badge/Tauri%202%20%2B%20Rust%20%2B%20Svelte%205-8b7cf6" alt="Stack" />
  <img src="https://img.shields.io/badge/sites-Pawchive%20%7C%20Kemono%20%7C%20Coomer-f5a524" alt="Sites" />
  <img src="https://img.shields.io/badge/languages-9-3ccf91" alt="Languages" />
</p>

<p align="center">
  <a href="https://ko-fi.com/izeno"><img src="https://ko-fi.com/img/githubbutton_sm.svg" alt="Get the official build on Ko-fi" /></a>
</p>

<p align="center">
  <img src="docs/screenshots/creator.png" alt="A creator's posts in Pawchive Downloader" width="900" />
</p>

---

Paste a link, or just type a creator's name. Pick exactly what you want, and a polite, resumable, checksum-verified download engine takes it from there, in the background, from the tray. Links to Mega, Google Drive, Dropbox, Pixeldrain, GoFile and MediaFire in a post come along too, decrypted and unpacked on your computer with the passwords the post mentions.

It is the big sibling of the [Pawchive Downloader browser extension](https://github.com/VoxDroid/PawchiveDownloader-BrowserExtension), and the next generation of [KemonoDownloader](https://github.com/VoxDroid/KemonoDownloader).

**Free to build, official builds on Ko-fi.** The source code is free and MIT licensed: anyone can [build the app](#building-it-yourself). Ready-made, tested installers for Windows, macOS and Linux are offered on [Ko-fi](https://ko-fi.com/izeno); buying one is the easiest way to get the app and keeps the project going.

## At a glance

|                         |                                                                                              |
| ----------------------- | -------------------------------------------------------------------------------------------- |
| **3 sites**             | Pawchive, Kemono and Coomer, with old kemono.su and coomer.su links still opening            |
| **7 outside hosts**     | Mega (decrypted locally), Google Drive, Dropbox, Pixeldrain, GoFile, MediaFire, direct links |
| **6.4 MB**              | the Windows download; native installers for Windows, macOS (universal) and Linux             |
| **Every file verified** | SHA-256 checked against the site's own hash, resumed after any interruption                  |
| **Never stuck**         | dead media servers detected in seconds, thumbnails as a fallback, failed files retried later |
| **9 languages**         | English, Japanese, Korean, Chinese, Spanish, Portuguese, Russian, German, French             |
| **About 160 tests**     | Rust engine, interface and end-to-end, run on Windows, macOS and Linux for every change      |

How it compares with every other Pawchive, Kemono and Coomer downloader: [docs/COMPARISON.md](docs/COMPARISON.md).

## Contents

- [Features](#features)
- [Screenshots](#screenshots)
- [Getting the app](#getting-the-app)
- [Using it](#using-it)
- [File names and folders](#file-names-and-folders)
- [Settings](#settings)
- [Privacy and politeness](#privacy-and-politeness)
- [Building it yourself](#building-it-yourself)
- [Development](#development)
- [Project structure](#project-structure)
- [Support the project](#support-the-project)
- [Disclaimer and ethical use](#disclaimer-and-ethical-use)
- [Credits](#credits)
- [License](#license)

## Features

**Finding things**

| Feature        | What it does                                                                                                                                                                          |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Three sites    | Links from pawchive.pw, kemono.cr and coomer.st (and their old domains) all work the same way: posts, creators, searches and recent posts.                                            |
| Creator search | Type a name instead of a link to search every creator on a site (about 100,000 on Pawchive alone), by service, popularity, update date or name. Lists are fetched at most once a day. |
| Browse         | Posts show every file with a thumbnail; creators show their posts with banner, avatar and their other accounts; recent posts can be searched per site.                                |
| Image viewer   | Look at a post's images full size with the arrow keys and pick them while you look.                                                                                                   |
| Read markers   | Posts you opened before are marked, so you can see where you left off.                                                                                                                |
| Clipboard      | Optionally, links you copy anywhere are noticed and offered for opening.                                                                                                              |
| Browser bridge | The browser extension can hand a download to the app with one click, after pairing with a six-digit code.                                                                             |

**Downloading**

| Feature                    | What it does                                                                                                                                                                                                                     |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| File picking               | Select files or posts by click and Shift-click, filter by type (images, videos, audio, archives, documents, text), invert, hide what you already have.                                                                           |
| Whole creators             | Download everything, a date range, a site search, or just the posts you tick. Large creators are scanned page by page while downloads already run.                                                                               |
| Filters                    | Only posts with certain words, skip posts or files with others (plain or `/regex/`), the newest N posts, file types to keep or skip. Defaults in Settings, changeable for each download.                                         |
| Outside hosts              | Files a post shares on Mega (files and folders, decrypted on your computer), Google Drive (files and folders), Dropbox, Pixeldrain, GoFile and MediaFire are downloaded with the post.                                           |
| Unpacking                  | Downloaded ZIP archives can be unpacked next to themselves, trying the passwords the post mentions; nothing can be written outside the folder.                                                                                   |
| Batch queue and URL import | Paste any number of links, import a `.txt` file or drag links onto the window. Each creator becomes one download; posts of the same creator are grouped.                                                                         |
| Three formats              | Separate files in a folder per post, one ZIP archive per post (ZIP64, no size limit, optional parts), or thumbnails only.                                                                                                        |
| Resumable downloads        | Files are written as `.part` files and continue with HTTP Range requests after a pause, a dropped connection or a restart of the app.                                                                                            |
| Checksums                  | Every finished file is checked against the SHA-256 in its address and downloaded again if it does not match.                                                                                                                     |
| Preview-only files         | When Pawchive only keeps a preview, the full file is fetched through the temporary link the post page offers.                                                                                                                    |
| Dead media servers         | Before a Kemono or Coomer download, the app checks the site's media server; when it is down you choose: cancel, thumbnails instead, or queue the full files for later. During downloads a dead server is noticed within seconds. |
| Duplicate skipping         | Files are recognised by content hash, across posts, creators and sites, and skipped when you already have them.                                                                                                                  |
| Post text and metadata     | Save the description as `content.txt` and/or `content.html`, a `post.json`, `links.txt` with outside links and passwords, comments with their replies, and a creator's announcements.                                            |
| Clean names                | Remove words such as `[HD]` from titles and file names.                                                                                                                                                                          |
| Fast mode                  | One switch: every file type, your saved options, no per-download choices.                                                                                                                                                        |

**Managing**

| Feature             | What it does                                                                                                                                                            |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Download queue      | Pause, resume, cancel, retry and reorder downloads; follow speed, progress and every item, retry single files, open folders.                                            |
| Automatic retries   | Failed files go back in the queue by themselves after a chosen time, up to three times. Retry everything at once or export the failed list.                             |
| Download window     | Start downloads only at chosen times (for example at night); the computer is kept awake while they run.                                                                 |
| Disk space guard    | New downloads wait instead of failing when the drive is nearly full, and go on by themselves.                                                                           |
| When done           | Quit, sleep or shut down after the queue finishes, with a cancellable countdown.                                                                                        |
| Crash-safe          | The queue is saved continuously to a local SQLite library and picks up where it stopped.                                                                                |
| History             | Every finished download with its files, size, duration and errors. Show it in the folder, download it again, export CSV or JSON.                                        |
| Watchlist           | Follow creators on any site. They are checked on a schedule; new posts raise a badge and a notification and can be downloaded automatically, each with its own options. |
| Favorites           | Bookmark posts and creators with notes, download them again in one click, import and export the list.                                                                   |
| Statistics          | Lifetime totals, the last twelve months, most downloaded creators, files by kind, and a check that finds deleted or moved files and downloads just those again.         |
| Portable mode       | With a `data` folder next to the program, everything stays there, so the app runs from a USB stick.                                                                     |
| Backup              | Settings, watchlist, favorites and history in one file, restored on another computer.                                                                                   |
| Tray and background | Closing the window keeps the app in the tray; progress shows on the tray tooltip and the taskbar button. Optional start at sign-in.                                     |
| Notifications       | Finished downloads, problems and new posts, in your language, optionally without titles or names.                                                                       |
| Logs and help       | A live, searchable activity log, also written to a rotating file, and a searchable guide inside the app.                                                                |
| Update check        | Looks for a newer version and points to Ko-fi (or the source). It never installs anything by itself.                                                                    |

**Interface**

| Feature         | What it does                                                                                                               |
| --------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Nine languages  | English, Japanese, Korean, Simplified Chinese, Spanish, Brazilian Portuguese, Russian, German and French, switchable live. |
| Six themes      | Midnight, Graphite, Plum, Forest, Ember and Light, or Match system, with six accent colors or your own.                    |
| Compact mode    | The sidebar folds to icons (Ctrl+B); tooltips float above everything and never get cut off.                                |
| Settings search | Find any setting by typing (Ctrl+F).                                                                                       |
| Privacy blur    | Thumbnails, avatars and banners stay blurred until pointed at, for sharing the screen or working in public.                |
| Typography      | JetBrains Mono Nerd Font throughout; every icon comes from the font itself.                                                |
| Keyboard        | Ctrl+L link bar, Ctrl+Shift+B batch, Ctrl+B sidebar, Ctrl+1 to Ctrl+9 pages, Ctrl+, settings, Ctrl+F settings search.      |
| Network         | System proxy, no proxy, a custom HTTP, HTTPS or SOCKS5 proxy, or Tor; a total speed limit.                                 |

## Screenshots

|                                                        |                                                               |
| ------------------------------------------------------ | ------------------------------------------------------------- |
| ![Start page](docs/screenshots/browse.png)             | ![A post with its files](docs/screenshots/post.png)           |
| ![Outside links of a post](docs/screenshots/links.png) | ![Creator search](docs/screenshots/search.png)                |
| ![Downloads](docs/screenshots/downloads.png)           | ![Compact sidebar](docs/screenshots/compact.png)              |
| ![Watchlist](docs/screenshots/watchlist.png)           | ![Statistics](docs/screenshots/statistics.png)                |
| ![Favorites](docs/screenshots/favorites.png)           | ![History](docs/screenshots/history.png)                      |
| ![Settings](docs/screenshots/settings.png)             | ![Batch queue, light theme](docs/screenshots/batch-light.png) |

The screenshots use the app's demo data.

## Getting the app

- **Official build:** installers for Windows (setup, MSI and portable), macOS (universal DMG for Apple Silicon and Intel) and Linux (AppImage, deb, rpm) are on [Ko-fi](https://ko-fi.com/izeno).
- **Build it yourself:** free, see [below](#building-it-yourself).

The installers are not code-signed yet: Windows SmartScreen and macOS Gatekeeper ask for confirmation the first time. On Windows choose **More info > Run anyway**; on macOS right-click the app and choose **Open**.

On Windows the app uses the Microsoft Edge WebView2 Runtime, which Windows 10 and 11 already include. On Linux it needs WebKitGTK 4.1, which the packages pull in.

## Using it

1. Copy a link from pawchive.pw, kemono.cr or coomer.st and paste it into the bar at the top of **Browse** (or press Ctrl+L, or drag the link onto the window). Or type a creator's name to search.
2. A post opens with its files, a creator with their posts. Pick what you want and filter by type, title or date.
3. In the bar at the bottom choose **Separate files**, **ZIP archive** or **Thumbnails**, whether to save the post text, skip files you have and follow outside links, and where to save. Press **Download**.
4. Follow it in **Downloads**, or close the window: the app keeps working in the tray and tells you when it is done.

To queue many links at once, open **Batch** (Ctrl+Shift+B). To follow a creator, press **Watch** on their page.

## File names and folders

Files go into your download folder, arranged by templates you can change in **Settings > File names** with a live preview:

| Template    | Default                                                                   |
| ----------- | ------------------------------------------------------------------------- |
| Folder      | `{site}/{creator} ({service})/{published:YYYY-MM-DD} {title} [{post_id}]` |
| File        | `{index:2}_{filename}`                                                    |
| ZIP archive | `{creator} - {published:YYYY-MM-DD} {title} [{post_id}]`                  |

Tokens: `{site}` `{service}` `{creator}` `{creator_id}` `{post_id}` `{title}` `{published}` `{added}` `{date}` `{index}` `{count}` `{name}` `{filename}` `{ext}` `{hash}` `{role}`. Dates take a format (`{published:YYYY/MM}`), `{index}` a width (`{index:3}` gives `007`) and `{hash}` a length (`{hash:8}`).

Every value is cleaned so names are valid on Windows, macOS and Linux, cannot hide their real extension and can never point outside the download folder.

## Settings

| Group             | Settings                                                                                                                                                                                                                                                                                                                                                                                            |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Appearance        | Language, theme, accent color, animations, blur previews                                                                                                                                                                                                                                                                                                                                            |
| Downloads         | Download folder, default format, fast mode, simultaneous downloads (up to 5), time between requests (at least 1 s), speed limit, retries, automatic retries, download window, keep awake, free space to keep, existing files, duplicate skipping, checksums, preview-only files, thumbnail fallback, post text, metadata, outside links, links file, comments, announcements, unpacking, file types |
| File names        | Folder, file and ZIP templates, words to remove                                                                                                                                                                                                                                                                                                                                                     |
| Filters           | Keywords, skip words, where to look, newest posts, file types and names to keep or skip                                                                                                                                                                                                                                                                                                             |
| ZIP archives      | Part size or one archive per post, videos beside the archive                                                                                                                                                                                                                                                                                                                                        |
| Network           | Proxy (system, none, custom, Tor), proxy address                                                                                                                                                                                                                                                                                                                                                    |
| Background        | Keep running in the tray, start at sign-in, start in the tray, copied links, notifications (and private notifications), watchlist interval, update check, history, log detail                                                                                                                                                                                                                       |
| Browser extension | Accept downloads from the browser, pairing, paired browsers                                                                                                                                                                                                                                                                                                                                         |
| Data              | Settings export and import, full backup and restore, open the data folder, forget remembered files, reset                                                                                                                                                                                                                                                                                           |

## Privacy and politeness

No accounts, analytics or tracking. Settings, history, the watchlist, favorites and logs stay in the app's folder on your computer (`library.db`). The app connects only to the sites you use, to the file hosts of links you choose to download, and, if you leave the update check on, to GitHub. Mega keys are used on your computer and never sent anywhere. The browser bridge listens on 127.0.0.1 only and only for a paired extension. See [PRIVACY.md](PRIVACY.md).

Each site asks clients for at most five downloads at once and one new request per second. The app enforces both per site, identifies itself with its own user agent and a contact address, and when a site answers 429 (or 503 with Retry-After) every request to it waits as long as the site says.

## Building it yourself

You need [Node.js](https://nodejs.org) 20 or newer, [pnpm](https://pnpm.io) 10, and [Rust](https://rustup.rs) (stable). Follow the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your system (on Linux: WebKitGTK 4.1 and a few libraries).

```bash
git clone https://github.com/VoxDroid/PawchiveDownloader.git
cd PawchiveDownloader
pnpm install
pnpm tauri build
```

The installers land in `src-tauri/target/release/bundle/`, the program itself in `src-tauri/target/release/`. Builds made from source call themselves **Built from source** on the About page.

## Development

```bash
pnpm install
pnpm app:dev        # run the app with hot reload
pnpm dev            # only the interface, in a browser, on demo data
pnpm check          # lint, format, types, translations, interface tests
pnpm test:rust      # Rust unit and integration tests
pnpm test:e2e       # interface tests in a browser (Playwright)
pnpm screenshots    # README screenshots from the demo data
pnpm icons          # render the icon with Blender
```

See [CONTRIBUTING.md](CONTRIBUTING.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [docs/TRANSLATING.md](docs/TRANSLATING.md).

## Project structure

```
src-tauri/            Rust: the app and its download engine
  src/pawchive/       sites (Pawchive, Kemono, Coomer), API client, creator lists, links, relay
  src/hosts/          Mega (with decryption), Drive, Dropbox, Pixeldrain, GoFile, MediaFire
  src/net/            HTTP client, rate limiter, speed limit
  src/naming/         templates and safe file names
  src/jobs/           job model, filters, planning, engine, transfers, ZIP, unpacking
  src/store/          SQLite library: queue, history, hashes, watchlist, favorites, read marks
  src/library.rs      statistics and the missing-file check
  src/bridge.rs       the browser extension's local server
  src/power.rs        keep-awake, free space, sleep and shut down
  src/watch.rs        scheduled watchlist checks
  src/app.rs          window, tray, notifications, clipboard, live updates
  src/commands.rs     what the interface can call
  tests/              integration tests against a mock site, and live checks
src/                  Svelte 5 interface
  features/           Browse, Downloads, History, Watchlist, Favorites, Statistics, Logs, Settings, Help, About
  ui/                 components, layout, theme, state
  lib/                bridge to Rust, demo backend, translations, helpers
tests/                Vitest unit and component tests, Playwright UI tests
scripts/              font subsetting, icon rendering, translations, screenshots
docs/                 architecture, extension bridge, translating, screenshots
```

## Support the project

Pawchive Downloader is free to build. If it saves you time, the [official build on Ko-fi](https://ko-fi.com/izeno) is the simplest way to get it and to support its development. Starring the repository helps too.

## Disclaimer and ethical use

This project is not affiliated with Pawchive. Creators rely on their supporters: download only what you have the right to, keep it for personal use, and support the creators you enjoy on their own pages. You are responsible for how you use the app and for following the laws where you live.

## Credits

- [JetBrains Mono](https://www.jetbrains.com/lp/mono/) and [Nerd Fonts](https://www.nerdfonts.com) (SIL Open Font License 1.1)
- Built with [Tauri](https://tauri.app), [Rust](https://www.rust-lang.org) and [Svelte](https://svelte.dev)
- The download policy follows [KemonoDownloader](https://github.com/VoxDroid/KemonoDownloader)
- Hand-drawn icon rendered with [Blender](https://www.blender.org)

## License

[MIT](LICENSE) © VoxDroid
