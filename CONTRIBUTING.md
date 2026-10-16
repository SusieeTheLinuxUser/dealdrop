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

- **No secrets in the code.** API keys are always user-supplied at runtime. See [SECURITY.md](SECURITY.md).
- **No telemetry.** DealDrop doesn't phone home. Don't add any.
- **All API calls go through the Rust backend.** The webview never calls external APIs directly.
- **Keep it dependency-light.** Think carefully before adding a crate.
- **Three platforms.** Releases ship AppImage + .msi + .dmg (AppImage built locally; .msi/.dmg by `.github/workflows/release.yml`, run manually after the Linux build is tested). Keep OS-specific code behind `cfg` so all three compile. Test locally with `cargo tauri build --bundles appimage` on Linux.

## Building the AppImage (Linux)

```bash
cd src-tauri && cargo tauri build --bundles appimage
# src-tauri/target/release/bundle/appimage/DealDrop_x.x.x_amd64.AppImage
```

## Opening a PR

- Keep PRs focused — one feature or fix per PR
- No need for a formal issue first for small things, just open the PR
- Large features: open an issue to discuss first so work doesn't get duplicated

## Code style

`cargo fmt`-style Rust; in `renderer/` match the surrounding style — 2-space indents, single quotes, `const`/`let` only.
