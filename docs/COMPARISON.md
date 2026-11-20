# How Pawchive Downloader compares

A look at the other tools people use to download from Pawchive (and from
Kemono, the site Pawchive is based on), made in November 2026 from each
project's own documentation and GitHub releases. It shows where Pawchive Downloader stands and
what it deliberately leaves out.

The tools:

- **Pawchive Downloader** (this app, 2.0.0): desktop app for Windows, macOS
  and Linux.
- **[Pawchive-Downloader](https://github.com/whyamihere773/Pawchive-Downloader)**
  by whyamihere773: Python desktop app for Pawchive, Kemono, Coomer and
  similar sites, with Mega, Google Drive, Dropbox and GoFile downloads.
- **[KToolBox](https://ktoolbox.readthedocs.io/)**: command line tool with a
  web interface, a Python API and an MCP service.
- **[Kemono-Downloader](https://github.com/Yuvi9587/Kemono-Downloader)** by
  Yuvi9587: Python desktop app.
- **[gallery-dl](https://github.com/mikf/gallery-dl)**: general command line
  downloader with a Pawchive extractor.
- **[kumono](https://github.com/APT37/kumono)**: Rust command line tool.
- **Pawchive File Ripper**: a userscript that zips one post in the browser.
- **Pawchive Downloader for the browser** (coming soon): this app's little
  sibling, an extension that will work on the page you view.

Legend: **Yes**, **Partly**, a short note, or **-** when the project's
documentation does not mention it (it may still exist).

## At a glance

Sizes are the Windows download of each project's latest release on
GitHub, checked on 2026-11-19; stars show how many people follow each
project there.

|                  | This app                                      | whyamihere773                       | KToolBox              | Yuvi9587                                  | gallery-dl            | kumono                |
| ---------------- | --------------------------------------------- | ----------------------------------- | --------------------- | ----------------------------------------- | --------------------- | --------------------- |
| Kind             | Desktop app                                   | Desktop app                         | CLI and web UI        | Desktop app                               | CLI                   | CLI                   |
| Windows download | **6.4 MB** zip (16 MB exe)                    | 332 MB zip                          | 43 MB zip             | 15 MB exe + 120 MB `_internal`            | pip or standalone exe | 8 MB exe              |
| Installers       | Setup, MSI, portable, DMG, AppImage, deb, rpm | Zip, tar.gz                         | Zip per system        | Exe and folder                            | pip, exe              | Single binary         |
| Systems          | Windows, macOS (universal), Linux             | Windows, Linux                      | Windows, macOS, Linux | Windows                                   | Any with Python       | Windows, macOS, Linux |
| Built with       | Rust and Svelte (Tauri)                       | Python and QML                      | Python and TypeScript | Python (PyQt5)                            | Python                | Rust                  |
| Sites            | Pawchive, Kemono, Coomer                      | Pawchive, Kemono, Coomer and others | Pawchive              | Kemono, Coomer, Bunkr, Discord and others | Hundreds              | Kemono, Coomer        |
| Latest release   | 1.1.1, 2026-11-18                             | 1.4.1, 2026-11-17                   | 1.0.0, 2026-08-23     | 2.0.0, 2026-11-12                         | 1.32.20, 2026-11-14   | 0.77.1, 2026-09-12    |
| GitHub stars     | private                                       | 91                                  | 597                   | 675                                       | 20,562                | 71                    |
| License          | MIT (builds on Ko-fi)                         | not stated                          | BSD-3-Clause          | MIT                                       | GPL-2.0               | MIT-0                 |
| Automated tests  | About 160 (Rust, unit, UI) on 3 systems       | -                                   | Yes                   | -                                         | Yes                   | -                     |

## Getting posts

| Feature                                                  | This app                                                                 | whyamihere773  | KToolBox        | Yuvi9587              | gallery-dl | kumono |
| -------------------------------------------------------- | ------------------------------------------------------------------------ | -------------- | --------------- | --------------------- | ---------- | ------ |
| Single posts, creators, site search                      | Yes                                                                      | Yes            | Yes             | Yes                   | Yes        | Yes    |
| Paste, drop or import many links                         | Yes                                                                      | Yes            | Yes             | Yes                   | Yes        | Yes    |
| Search creators by name                                  | Yes                                                                      | -              | Yes             | -                     | -          | -      |
| Browse a post's files with thumbnails before downloading | Yes                                                                      | Yes            | Partly (web UI) | -                     | -          | -      |
| Image viewer while picking files                         | Yes                                                                      | Yes            | -               | -                     | -          | -      |
| Linked accounts of a creator                             | Shown                                                                    | -              | -               | -                     | -          | Yes    |
| Preview-only files (signed relay links)                  | Yes                                                                      | -              | -               | -                     | -          | -      |
| Comments                                                 | Yes                                                                      | -              | -               | Partly (filter scope) | Yes        | -      |
| Announcements                                            | Yes                                                                      | -              | -               | -                     | Yes        | -      |
| Outside links (Mega, Drive...) and passwords             | Saved, listed, downloaded                                                | Downloads them | -               | Lists, some download  | Metadata   | -      |
| Downloads from outside hosts                             | **Yes: Mega (decrypted), Drive, Dropbox, Pixeldrain, GoFile, MediaFire** | Yes            | -               | Partly                | Some       | -      |

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

| Feature                                              | This app | whyamihere773      | KToolBox   | Yuvi9587 | gallery-dl     | kumono |
| ---------------------------------------------------- | -------- | ------------------ | ---------- | -------- | -------------- | ------ |
| Resume after pause, network loss or restart          | Yes      | Yes                | Partly     | Yes      | Yes            | Yes    |
| SHA-256 check of every file                          | Yes      | -                  | -          | -        | -              | Yes    |
| One ZIP per post (ZIP64, split parts)                | Yes      | -                  | -          | -        | Post-processor | -      |
| Polite limits and 429 cooldowns                      | Yes      | Partly             | Yes        | Partly   | Yes            | Partly |
| Proxy and Tor                                        | Yes      | Yes                | Yes        | Yes      | Yes            | Yes    |
| Speed limit                                          | Yes      | -                  | -          | -        | Yes            | -      |
| Waits for the network instead of failing             | Yes      | Partly             | -          | -        | -              | -      |
| Waits instead of failing when the disk is full       | Yes      | Yes                | -          | -        | -              | -      |
| Download window (for example at night)               | Yes      | Yes                | Cron plans | -        | -              | -      |
| Keeps the computer awake                             | Yes      | Yes                | -          | -        | -              | -      |
| Quit, sleep or shut down when done                   | Yes      | -                  | -          | -        | -              | -      |
| Queue with pause, reorder and retry per file         | Yes      | Partly             | Yes        | Partly   | -              | -      |
| Retry all failed, export failed list                 | Yes      | Yes                | Rerun      | Yes      | -              | -      |
| Notices dead media servers, falls back to thumbnails | **Yes**  | -                  | -          | -        | -              | -      |
| Unpacks ZIP archives with the post's passwords       | **Yes**  | Yes (zip, rar, 7z) | -          | -        | -              | -      |
| Retries failed files later by itself                 | **Yes**  | -                  | -          | -        | -              | -      |

## Keeping up and looking back

| Feature                                          | This app                         | whyamihere773 | KToolBox   | Yuvi9587          | gallery-dl        | kumono |
| ------------------------------------------------ | -------------------------------- | ------------- | ---------- | ----------------- | ----------------- | ------ |
| Watchlist with new-post badges and notifications | Yes                              | Yes           | Sync plans | -                 | -                 | -      |
| Download new posts automatically                 | Yes                              | Yes           | Yes        | -                 | -                 | -      |
| Per-creator folder, mode and filters             | Yes                              | Yes           | Per plan   | -                 | -                 | -      |
| Favorites with notes                             | Yes                              | Partly        | Yes        | Account favorites | Account favorites | -      |
| History with download again and CSV              | Yes                              | Yes           | Yes        | Yes               | -                 | -      |
| Statistics                                       | Yes                              | Yes           | Overview   | -                 | -                 | -      |
| Find deleted or moved files and get them back    | Yes                              | Yes           | -          | -                 | -                 | -      |
| Crash-safe queue                                 | Yes                              | Yes           | Yes        | Partly            | -                 | -      |
| Read markers on posts                            | **Yes**                          | -             | -          | -                 | -                 | -      |
| Portable mode, full backup and restore           | **Yes**                          | -             | -          | -                 | -                 | -      |
| Browser extension hands downloads to the app     | **Ready, extension coming soon** | -             | -          | -                 | -                 | -      |

## The app itself

| Feature                                  | This app      | whyamihere773 | KToolBox  | Yuvi9587 | gallery-dl | kumono   |
| ---------------------------------------- | ------------- | ------------- | --------- | -------- | ---------- | -------- |
| Native app for Windows, macOS and Linux  | Yes           | Python        | Python    | Python   | Python     | Rust CLI |
| Tray, start at sign-in, taskbar progress | Yes           | Partly        | -         | -        | -          | -        |
| Languages                                | 9             | 14            | 7         | Some     | English    | English  |
| Themes                                   | 6 and accents | Yes           | -         | Yes      | n/a        | n/a      |
| Searchable help inside the app           | Yes           | Yes           | Docs site | -        | Docs site  | -        |

## Left out on purpose

- **RAR and 7z archives, converting images, tagging audio, AI character
  recognition.** These change your files or need large extra components;
  dedicated tools do them better. ZIP archives are unpacked.
- **Hosts that need an account or a captcha** (Bunkr and similar). Their
  links are still listed and saved to `links.txt`.
- **Signing in.** Everything the app does works without an account, and no
  password or cookie ever has to be handed to it.
- **Discord servers on Kemono.** They are a chat archive rather than posts;
  they may come later.
