# How Pawchive Downloader compares

A look at the other tools people use to download from Pawchive (and from
Kemono, the site Pawchive is based on), made in November 2026 from each
project's own documentation. It shows where Pawchive Downloader stands and
what it deliberately leaves out.

The tools:

- **Pawchive Downloader** (this app, 1.1.0): desktop app for Windows, macOS
  and Linux.
- **[Pawchive-Downloader](https://github.com/whyamihere773/Pawchive-Downloader)**
  by whyamihere773: Python desktop app for Kemono-style sites with many
  outside hosts.
- **[KToolBox](https://ktoolbox.readthedocs.io/)**: command line tool with a
  web interface, a Python API and an MCP service.
- **[Kemono-Downloader](https://github.com/Yuvi9587/Kemono-Downloader)** by
  Yuvi9587: Python desktop app.
- **[gallery-dl](https://github.com/mikf/gallery-dl)**: general command line
  downloader with a Pawchive extractor.
- **kumono**: Rust command line tool.
- **Pawchive File Ripper**: a userscript that zips one post in the browser.
- **[Pawchive Downloader for the browser](https://github.com/VoxDroid/PawchiveDownloader-BrowserExtension)**:
  this app's little sibling, an extension that works on the page you view.

Legend: **Yes**, **Partly**, a short note, or **-** when the project's
documentation does not mention it (it may still exist).

## Getting posts

| Feature                                                  | This app         | whyamihere773  | KToolBox        | Yuvi9587              | gallery-dl | kumono |
| -------------------------------------------------------- | ---------------- | -------------- | --------------- | --------------------- | ---------- | ------ |
| Single posts, creators, site search                      | Yes              | Yes            | Yes             | Yes                   | Yes        | Yes    |
| Paste, drop or import many links                         | Yes              | Yes            | Yes             | Yes                   | Yes        | Yes    |
| Search creators by name                                  | Yes              | -              | Yes             | -                     | -          | -      |
| Browse a post's files with thumbnails before downloading | Yes              | Yes            | Partly (web UI) | -                     | -          | -      |
| Image viewer while picking files                         | Yes              | Yes            | -               | -                     | -          | -      |
| Linked accounts of a creator                             | Shown            | -              | -               | -                     | -          | Yes    |
| Preview-only files (signed relay links)                  | Yes              | -              | -               | -                     | -          | -      |
| Comments                                                 | Yes              | -              | -               | Partly (filter scope) | Yes        | -      |
| Announcements                                            | Yes              | -              | -               | -                     | Yes        | -      |
| Outside links (Mega, Drive...) and passwords             | Saved and listed | Downloads them | -               | Lists, some download  | Metadata   | -      |
| Downloads from outside hosts                             | No (see below)   | Yes            | -               | Partly                | Some       | -      |

## Choosing what to download

| Feature                                         | This app               | whyamihere773 | KToolBox    | Yuvi9587 | gallery-dl        | kumono |
| ----------------------------------------------- | ---------------------- | ------------- | ----------- | -------- | ----------------- | ------ |
| File types by category                          | Yes                    | Yes           | Yes         | Yes      | Yes               | Yes    |
| Exact extensions to keep or skip                | Yes                    | Partly        | Yes         | Partly   | Yes               | Yes    |
| Keywords a post must have                       | Yes                    | Yes           | Yes (title) | Yes      | Yes (expressions) | -      |
| Words that skip a post or a file                | Yes                    | Yes           | Yes         | Yes      | Yes               | -      |
| Regular expressions in filters                  | Yes                    | -             | Yes         | -        | Yes               | -      |
| Date range                                      | Yes                    | Yes           | Yes         | -        | Yes               | -      |
| Newest N posts                                  | Yes                    | -             | Offset      | -        | Ranges            | -      |
| Skip files already downloaded (by content hash) | Yes                    | Yes           | Yes         | Yes      | Archive file      | Yes    |
| Remove words from names                         | Yes                    | -             | -           | Yes      | Partly            | -      |
| Name templates                                  | Yes, with live preview | Yes           | Yes         | Partly   | Yes               | -      |

## Downloading

| Feature                                        | This app | whyamihere773 | KToolBox   | Yuvi9587 | gallery-dl     | kumono |
| ---------------------------------------------- | -------- | ------------- | ---------- | -------- | -------------- | ------ |
| Resume after pause, network loss or restart    | Yes      | Yes           | Partly     | Yes      | Yes            | Yes    |
| SHA-256 check of every file                    | Yes      | -             | -          | -        | -              | Yes    |
| One ZIP per post (ZIP64, split parts)          | Yes      | -             | -          | -        | Post-processor | -      |
| Polite limits and 429 cooldowns                | Yes      | Partly        | Yes        | Partly   | Yes            | Partly |
| Proxy and Tor                                  | Yes      | Yes           | Yes        | Yes      | Yes            | Yes    |
| Speed limit                                    | Yes      | -             | -          | -        | Yes            | -      |
| Waits for the network instead of failing       | Yes      | Partly        | -          | -        | -              | -      |
| Waits instead of failing when the disk is full | Yes      | Yes           | -          | -        | -              | -      |
| Download window (for example at night)         | Yes      | Yes           | Cron plans | -        | -              | -      |
| Keeps the computer awake                       | Yes      | Yes           | -          | -        | -              | -      |
| Quit, sleep or shut down when done             | Yes      | -             | -          | -        | -              | -      |
| Queue with pause, reorder and retry per file   | Yes      | Partly        | Yes        | Partly   | -              | -      |
| Retry all failed, export failed list           | Yes      | Yes           | Rerun      | Yes      | -              | -      |

## Keeping up and looking back

| Feature                                          | This app | whyamihere773 | KToolBox   | Yuvi9587          | gallery-dl        | kumono |
| ------------------------------------------------ | -------- | ------------- | ---------- | ----------------- | ----------------- | ------ |
| Watchlist with new-post badges and notifications | Yes      | Yes           | Sync plans | -                 | -                 | -      |
| Download new posts automatically                 | Yes      | Yes           | Yes        | -                 | -                 | -      |
| Per-creator folder, mode and filters             | Yes      | Yes           | Per plan   | -                 | -                 | -      |
| Favorites with notes                             | Yes      | Partly        | Yes        | Account favorites | Account favorites | -      |
| History with download again and CSV              | Yes      | Yes           | Yes        | Yes               | -                 | -      |
| Statistics                                       | Yes      | Yes           | Overview   | -                 | -                 | -      |
| Find deleted or moved files and get them back    | Yes      | Yes           | -          | -                 | -                 | -      |
| Crash-safe queue                                 | Yes      | Yes           | Yes        | Partly            | -                 | -      |

## The app itself

| Feature                                  | This app      | whyamihere773 | KToolBox  | Yuvi9587 | gallery-dl | kumono   |
| ---------------------------------------- | ------------- | ------------- | --------- | -------- | ---------- | -------- |
| Native app for Windows, macOS and Linux  | Yes           | Python        | Python    | Python   | Python     | Rust CLI |
| Tray, start at sign-in, taskbar progress | Yes           | Partly        | -         | -        | -          | -        |
| Languages                                | 9             | 14            | 7         | Some     | English    | English  |
| Themes                                   | 6 and accents | Yes           | -         | Yes      | n/a        | n/a      |
| Searchable help inside the app           | Yes           | Yes           | Docs site | -        | Docs site  | -        |

## Left out on purpose

- **Downloading from Mega, Google Drive and other hosts.** Each host has its
  own rules, accounts and limits, and some forbid automated downloads. The
  app saves those links and passwords (`links.txt`, and the Links tab of a
  post) so they can be opened in the browser or a host's own app.
- **Extracting archives, converting images, tagging audio, AI character
  recognition.** These change your files; dedicated tools do them better.
- **Signing in to Pawchive.** Everything the app does works without an
  account, and no password or cookie ever has to be handed to it.
- **Discord servers.** Pawchive does not archive Discord.
