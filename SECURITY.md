# Security Policy

## Supported versions

Security fixes go into the latest version only. Please keep the app up to
date; the About page tells you when a newer version exists.

| Version        | Supported |
| -------------- | --------- |
| Latest version | Yes       |
| Older versions | No        |

## Reporting a vulnerability

Please report vulnerabilities privately, never in a public issue.

- **GitHub**: on the repository's
  [Security tab](https://github.com/VoxDroid/PawchiveDownloader/security/advisories/new),
  choose **Report a vulnerability**. Only the maintainers can see it.
- **Email**: [izeno.contact@gmail.com](mailto:izeno.contact@gmail.com).

Include the app version (About page), your operating system, steps to
reproduce, and what an attacker could achieve. You will get an answer within a
week, and credit in the changelog if you want it.

## What the app does to stay safe

- **Contained file names.** Every name that comes from the site is cleaned:
  `..`, absolute paths, reserved Windows names, control and direction
  characters and shortcut extensions are removed or neutralised, so a file can
  never be written outside the download folder or disguise its type.
- **Verified files.** Downloaded files are checked against the SHA-256 checksum
  in their Pawchive address before they get their real name.
- **No HTML rendering.** Post bodies are shown as plain text; the interface
  never injects HTML from the site.
- **Narrow network access.** The Rust side only talks to `pawchive.pw` and its
  subdomains (and, for the update check, GitHub's API). Media server addresses
  from the API are accepted only on Pawchive's own hosts over HTTPS.
- **Tight interface permissions.** The webview has a strict Content Security
  Policy and only the Tauri permissions it needs (dialogs, opening links and
  folders, notifications).
- **Nothing installed behind your back.** The update check only reports a new
  version; it never downloads or runs anything.

## Scope

In scope: the desktop app and its build files in this repository. Out of
scope: Pawchive itself, the browser extension (report those in
[its repository](https://github.com/VoxDroid/PawchiveDownloader-BrowserExtension/security)), and issues that need an already
compromised computer. Problems with the bridge between the two belong here.
