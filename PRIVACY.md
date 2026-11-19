# Privacy

Pawchive Downloader has no accounts, no analytics, no tracking and no
advertising. It does not send anything about you anywhere.

## What stays on your computer

Everything the app remembers is stored in its data folder (Settings > Data >
App data folder):

| What                                | Where                                             | Why                                             |
| ----------------------------------- | ------------------------------------------------- | ----------------------------------------------- |
| Settings                            | `library.db`                                      | Your choices                                    |
| Download queue                      | `library.db`                                      | So downloads survive a restart                  |
| History                             | `library.db`                                      | The History page; can be turned off and cleared |
| Downloaded file hashes and post ids | `library.db`                                      | Skipping files you already have; can be cleared |
| Watchlist and favorites             | `library.db`                                      | The pages of the same name                      |
| Activity log                        | `pawchive-downloader.log` in the app's log folder | Troubleshooting; at most about 10 MB            |
| Recently opened links               | the interface's local storage                     | The start page                                  |
| Posts you opened                    | `library.db`                                      | Read markers in creator and listing views       |
| Creator lists of each site          | `creators*.json`                                  | Creator search; fetched at most once a day      |
| Browser extension pairings          | `library.db` (only a hash of each token)          | The browser extension bridge; can be removed    |

Your downloads go to the folder you choose.

## Who the app talks to

- **Pawchive, Kemono and Coomer** (`pawchive.pw`, `kemono.cr`, `coomer.st`
  and their subdomains), only the ones you use: to read posts and creators
  and to download files. The app identifies itself as
  `PawchiveDownloader/<version>` with a contact address, as the sites ask of
  programs. Requests go through the proxy or Tor if you set one.
- **File hosts** (Mega, Google Drive, Dropbox, Pixeldrain, GoFile,
  MediaFire), only when you download a post's outside links. GoFile is used
  through a guest account it creates for every visitor. Mega keys stay on
  your computer: files are decrypted locally.
- **Your browser extension**, only if you turn on Settings > Browser
  extension: the app then listens on `127.0.0.1` (this computer only) for a
  paired extension. Web pages cannot use it.
- **The clipboard** is read only if you turn on copied links, and only on
  this computer.
- **GitHub** (`api.github.com`): only if "Check for updates" is on, once at
  start, to read the newest version number of the source repository.
- **Ko-fi and GitHub pages** open in your browser only when you click a link
  to them.

## Removing your data

Settings > Data lets you forget remembered files, and History lets you clear
the history. Uninstalling the app and deleting its data folder removes
everything it stored.
