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

Your downloads go to the folder you choose.

## Who the app talks to

- **Pawchive** (`pawchive.pw` and its subdomains): to read posts and creators
  and to download files. The app identifies itself as
  `PawchiveDownloader/<version>` with a contact address, as Pawchive asks of
  programs. Requests go through the proxy or Tor if you set one.
- **GitHub** (`api.github.com`): only if "Check for updates" is on, once at
  start, to read the newest version number of the source repository.
- **Ko-fi and GitHub pages** open in your browser only when you click a link
  to them.

## Removing your data

Settings > Data lets you forget remembered files, and History lets you clear
the history. Uninstalling the app and deleting its data folder removes
everything it stored.
