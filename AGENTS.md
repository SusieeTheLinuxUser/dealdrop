# AGENTS.md

DealDrop is a Tauri 2 desktop app: Rust backend (`src-tauri/`) + plain HTML/CSS/JS UI (`renderer/`, no bundler).
It polls store APIs, shows free games / store sales / hot deals / wishlists, and sends desktop notifications from the tray.

## Layout

| Path | What |
|---|---|
| `src-tauri/src/main.rs` | App state, persistence (`~/.config/dealdrop/config.json`), poll loop, notifications, tray, Tauri commands |
| `src-tauri/src/sources.rs` | Every no-account HTTP source (Steam, GOG, Epic free, GamerPower, ITAD, CheapShark) + live smoke tests |
| `src-tauri/src/stores.rs` | Sign-in window + Steam/GOG/Epic auth, GOG & Epic wishlists, Epic sales |
| `renderer/api.js` | `window.api` → Tauri `invoke` bridge (the only place the UI touches the backend) |
| `renderer/app.js` | All UI rendering. `renderer/index.html`, `renderer/style.css` |

## Commands

```bash
cd src-tauri
cargo check                                          # fast compile check
cargo test -- --ignored --nocapture --test-threads=1 # live API smoke tests (needs network)
cargo tauri dev                                      # run the app (needs `cargo install tauri-cli`)
cargo tauri build --bundles appimage                 # → src-tauri/target/release/bundle/appimage/
```

## Rules

- No secrets in the repo. API keys / tokens are user-supplied and live only in the user's config file.
- No telemetry. All external calls go through `sources.rs` / `stores.rs`; the webview never calls external APIs.
- JSON sent to the UI is camelCase (`#[serde(rename_all = "camelCase")]`); keep it in sync with `renderer/app.js`.
- Config keys in `Db` deliberately match the old Electron `electron-store` names so upgrades keep settings.
- Each source returns `Result`; the poll keeps the last good data when one fails and surfaces errors in the UI where it matters (hot deals, wishlists).
- When an upstream API changes, fix it and extend the matching `#[ignore]` live test in `sources.rs`.
- Version lives in two places: `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`.

## Known unverified paths

GOG and Epic **wishlist** sign-in can't be tested without real accounts. They follow the launchers' public OAuth flow; if one breaks, the UI shows the error above the wishlist.
