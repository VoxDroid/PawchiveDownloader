# Extension bridge (design)

Status: **planned, not implemented.** This describes how the
[browser extension](https://github.com/VoxDroid/PawchiveDownloader-BrowserExtension)
will be able to hand downloads to the desktop app, so the work can be picked up
without redesign.

## Goal

On a Pawchive page, the extension's panel offers **Send to app** next to
**Download**. The app then downloads with its own engine (resumable, no size
limits, keeps going when the browser closes). If the app is not running or not
paired, the extension falls back to downloading itself.

## Transport: a loopback HTTP server

The app listens on `127.0.0.1` only, on a fixed port with fallbacks
(`49517`, then `49518` and `49519`). Native messaging was considered and
rejected for the first version: it needs a manifest installed per browser and
per platform, and does not work for portable builds. A loopback server works
the same everywhere.

- The server is **off by default** and turned on in Settings > Browser
  extension.
- It accepts connections only from `127.0.0.1` and only requests whose `Origin`
  is a browser-extension origin (`chrome-extension://`, `moz-extension://`,
  `extension://`).
- Every request needs `Authorization: Bearer <token>`.

## Pairing

1. Settings > Browser extension > **Pair** shows a six-digit code for two
   minutes.
2. The extension's Settings page asks for the code and calls
   `POST /v1/pair { code, name }`.
3. The app answers with a random 256-bit token, stores its hash with the
   extension's name, and lists the pairing in Settings, where it can be revoked.

## API (v1)

All bodies are JSON. Errors use HTTP status codes with `{ "error": "<code>" }`.

| Method and path               | Body                                                                               | Answer                                                                        |
| ----------------------------- | ---------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `GET /v1/hello`               |                                                                                    | `{ app, version, paired }` (no token needed; lets the extension find the app) |
| `POST /v1/pair`               | `{ code, name }`                                                                   | `{ token }`                                                                   |
| `POST /v1/jobs`               | a `JobRequest` (`posts`, `refs` or `creator`, as in `src-tauri/src/jobs/model.rs`) | `{ jobId }`                                                                   |
| `GET /v1/jobs`                |                                                                                    | `JobSummary[]`                                                                |
| `POST /v1/jobs/{id}/{action}` |                                                                                    | `204` (`pause`, `resume`, `cancel`, `retry`)                                  |
| `POST /v1/watch`              | `{ service, creatorId, name }`                                                     | `{ added }`                                                                   |

`JobRequest` already exists on both sides with the same shape, so the
extension can send exactly what its own engine would have run.

## Implementation notes

- Rust: a small `axum` (or `hyper`) server in `src-tauri/src/bridge.rs`,
  started from `core.rs` when enabled, calling the same `Engine` and `Watcher`
  methods as the Tauri commands.
- Rate limits are unaffected: jobs from the extension join the same queue and
  limiter.
- The extension keeps a "send to app" preference and shows the app's
  progress in its popup through `GET /v1/jobs`.
