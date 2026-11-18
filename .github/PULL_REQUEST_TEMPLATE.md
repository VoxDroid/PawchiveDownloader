## What and why

<!-- What does this change, and why? Link the issue: Closes #123 -->

## How it was tested

- [ ] `pnpm check` passes
- [ ] `pnpm test:rust` passes, and `cargo clippy` and `cargo fmt --check` are clean
- [ ] `pnpm test:e2e` passes (for interface changes)
- [ ] Tried in the running app (`pnpm app:dev`) on Windows, macOS or Linux

## Checklist

- [ ] New interface text is in every locale (English placeholders are noted below)
- [ ] Screenshots attached for visible changes
- [ ] `CHANGELOG.md` updated under Unreleased
- [ ] Request limits and the `SiteLimiter` in `src-tauri/src/net/limiter.rs` are untouched or still respected
