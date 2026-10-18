# Contributing to DealDrop

Thanks for wanting to help! DealDrop is free and open source — contributions of all kinds are welcome.

## Getting started

Requires Rust (stable), `webkit2gtk-4.1` + GTK dev packages on Linux, and `cargo install tauri-cli --version "^2"`.

```bash
git clone https://github.com/SusieeTheLinuxUser/dealdrop
cd dealdrop/src-tauri
cargo tauri dev
```

The UI is plain HTML/CSS/JS in `renderer/` — no build step.

## Project layout

See [AGENTS.md](AGENTS.md) for the file map and commands.

## Adding a new data source

1. Write an `async fn` returning `R<Vec<…>>` in `src-tauri/src/sources.rs` (see `gog_sales` as a pattern) and add an `#[ignore]` live test next to it
2. Call it from `run_poll` in `main.rs` and add the result to `Snapshot`
3. Render it in `renderer/app.js`

## Golden rules

- **No secrets in the code.** The app has no API keys; sign-in tokens live only in the user's config file. Secrets never go in URLs. See [SECURITY.md](SECURITY.md).
- **No telemetry.** DealDrop doesn't phone home. Don't add any.
- **All API calls go through the Rust backend.** The webview never calls external APIs directly.
- **Keep it dependency-light.** Think carefully before adding a crate.
- **Three platforms.** Releases ship AppImage + .msi + .dmg (AppImage built locally; .msi/.dmg by `.github/workflows/release.yml`, run manually after the Linux build is tested). Keep OS-specific code behind `cfg` so all three compile. Test locally with `cargo tauri build --bundles appimage` on Linux.

## Running checks

Run these inside `src-tauri/`:

```bash
cargo check                                          # fast compile check
cargo test                                           # unit tests
cargo test -- --ignored --nocapture --test-threads=1 # live API smoke tests (needs network)
```

`wishlist_cheapest_live` needs a `STEAM_ID` environment variable (`STEAM_ID=... cargo test wishlist_cheapest_live -- --ignored --nocapture`). When an upstream API changes, fix it and extend the matching `#[ignore]` live test in `sources.rs`. For manual Windows/macOS testing see [TESTING.md](TESTING.md).

## UI rules

- Links open only through `open_url` / `open_in_app` (`safe_https`); external UI links are `<a data-href="https://…">`. Never render remote content in an in-app webview.
- The CSP forbids inline handlers (`onclick=`, `onerror=`): use listeners.
- Escape every remote string put into `innerHTML`, including attribute values.
- Never add horizontal scrolling; shrink the sidebar (`clamp()`) instead.
- Fonts are bundled in `renderer/fonts/`; never link a font CDN (`font-src 'self'`).
- JSON sent to the UI is camelCase (`#[serde(rename_all = "camelCase")]`); keep it in sync with `renderer/app.js`.
- Add a theme in both `renderer/style.css` (`html[data-theme="…"]`) and `renderer/app.js` (`BASE_THEMES` / `SALE_THEMES`).

## Building the AppImage (Linux)

```bash
cd src-tauri && cargo tauri build --bundles appimage
# src-tauri/target/release/bundle/appimage/DealDrop_x.x.x_amd64.AppImage
```

## Opening a PR

- Branch from `dev` (work happens there; `main` only holds released code)
- Keep PRs focused — one feature or fix per PR
- No need for a formal issue first for small things, just open the PR
- Large features: open an issue to discuss first so work doesn't get duplicated

## Code style

`cargo fmt`-style Rust; in `renderer/` match the surrounding style — 2-space indents, single quotes, `const`/`let` only.
