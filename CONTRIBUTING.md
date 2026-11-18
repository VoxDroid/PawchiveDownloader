# Contributing

Thanks for helping. Bug reports, translations and pull requests are all
welcome. Please read the [Code of Conduct](CODE_OF_CONDUCT.md) first.

## Before you start

- For anything larger than a small fix, open an issue first so we can agree on
  the approach.
- Security problems go to [SECURITY.md](SECURITY.md), not to public issues.
- Pawchive's request limits (five downloads at once, one new request per
  second, cooldowns on 429 and 503) are not negotiable. Changes that work
  around them will not be merged.

## Setting up

You need Node.js 20 or newer, pnpm 10, Rust stable and the
[Tauri prerequisites](https://tauri.app/start/prerequisites/) for your system.

```bash
pnpm install
pnpm app:dev      # the app with hot reload
pnpm dev          # only the interface, in a browser, on demo data
```

The interface can run without the Rust side: in a plain browser it talks to a
demo backend (`src/lib/mock.ts`). That is the quickest way to work on the
interface, and what the Playwright tests use.

## Checks

```bash
pnpm check                                        # lint, format, types, translations, Vitest
pnpm test:rust                                    # Rust unit and integration tests
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --check
pnpm test:e2e                                     # Playwright (PW_CHANNEL=msedge to use Edge)
```

CI runs all of them on Windows, macOS and Linux.

## Style

- TypeScript is strict; Svelte 5 runes only. Prettier and ESLint decide the
  formatting; `cargo fmt` and Clippy for Rust.
- No emojis anywhere: not in the interface, the docs or commit messages.
- Interface text goes through `t('key')` and exists in every locale (see
  [docs/TRANSLATING.md](docs/TRANSLATING.md)). If you cannot translate a new
  string, add the English text to every source file and say so in the pull
  request.
- Icons come from the bundled JetBrains Mono Nerd Font. To add one, add its
  Nerd Font glyph name to `src/ui/icon-names.json` and run
  `python scripts/subset-font.py <folder with the Nerd Font TTFs>`.
- Commit messages are plain and descriptive: what changed and why, in the
  imperative ("Add a speed limit", not "added speed limit").

## Where things live

[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) explains how the Rust core and
the interface fit together. In short: everything that touches the network or
the disk is in `src-tauri/src`, everything you see is in `src`.

## Pull requests

- Keep them focused; one change per pull request.
- Add or update tests. Engine changes need an integration test in
  `src-tauri/tests/engine.rs`.
- Update `CHANGELOG.md` under Unreleased.
- Attach screenshots for visible changes (`pnpm screenshots` regenerates the
  README ones).

## Releases

Official builds are made in a separate, private repository and offered on
Ko-fi; the source stays free to build. Maintainers: bump the version in
`package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, update
the changelog, push a `vX.Y.Z` tag, then run the build workflow in the release
repository with that tag.
