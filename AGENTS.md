# AGENTS.md

DealDrop is a Tauri 2 desktop app: Rust backend (`src-tauri/`) + plain HTML/CSS/JS UI (`renderer/`, no bundler).
It polls store APIs, shows free games / store sales / hot deals / wishlists, and sends desktop notifications from the tray.

## Layout

| Path | What |
|---|---|
| `src-tauri/src/main.rs` | App state, persistence (`config.json` in the OS config dir: `~/.config/dealdrop` / `%APPDATA%\dealdrop` / `~/Library/Application Support/dealdrop`), poll loop, notifications, tray, Tauri commands |
| `src-tauri/src/sources.rs` | Every no-account HTTP source (Steam, GOG, Epic free, GamerPower, CheapShark) + live smoke tests |
| `src-tauri/src/stores.rs` | Sign-in window + Steam/GOG/Epic auth, GOG & Epic wishlists, Epic sales |
| `src-tauri/src/autostart.rs` | "Start when I log in": per-OS entry (XDG autostart file / `Run` registry value / LaunchAgent) that runs the app with `--hidden` (tray only); the OS entry is the only record of the setting |
| `src-tauri/src/launchers.rs` | Open-in-launcher table, per-OS install detection, deep links |
| `.github/workflows/ci.yml` | Every push to `dev`/`main` and every PR → `cargo test` + `node tools/preview/selfcheck.mjs` |
| `.github/workflows/release.yml` | Manual dispatch → builds Windows .msi + macOS .dmg and attaches them to an existing release |
| `renderer/api.js` | `window.api` → Tauri `invoke` bridge (the only place the UI touches the backend) |
| `renderer/app.js` | All UI rendering. `renderer/index.html`, `renderer/style.css` |
| `tools/preview/` | Dev-only browser preview harness with a local mock Tauri bridge; never bundled |

## Commands

```bash
cd src-tauri
cargo check                                          # fast compile check
cargo test -- --ignored --nocapture --test-threads=1 # live API smoke tests (needs network)
cargo tauri dev                                      # run the app (needs `cargo install tauri-cli`)
cargo tauri build --bundles appimage                 # → src-tauri/target/release/bundle/appimage/
```

## Branches

- `main` = what has been released (the fallback; every release is also a `vX.Y.Z` tag). Don't commit work-in-progress to it.
- `dev` = day-to-day work. Commit and push here; to release, fast-forward `main` to `dev` (`git switch main && git merge --ff-only dev && git push`), then follow Releasing below and switch back to `dev`.
- GitHub rulesets: `main`/`dev` can't be deleted or force-pushed and need linear history (PRs merge by rebase or squash; merge commits are off). `v*` tags can't be moved or deleted.

## Releasing — ALWAYS all three platforms

Every release ships **Linux AppImage + Windows .msi + macOS .dmg (universal: Intel + Apple Silicon)**. Never release just one.

1. On `dev`: bump the version in all four places (see Rules); update the docs; commit; then fast-forward `main` to `dev` and push both.
2. **Linux first:** `RELEASE_NOTES="…" scripts/release-linux.sh` builds the AppImage with embedded update info (Gear Lever / AppImageUpdate read it: `gh-releases-zsync|…|DealDrop_*_amd64.AppImage.zsync`), makes the `.zsync` and publishes both to the release. Needs `zsyncmake` (package `zsync`). The maintainer tests it.
3. **Only after the Linux build is confirmed working:** `gh workflow run release.yml -f tag=vX.Y.Z` — `.github/workflows/release.yml` builds the Windows .msi and macOS .dmg on GitHub runners and attaches them to that same release. (Windows/macOS can't be built locally on Linux; don't ship them for a build that's known broken.)
4. Check the release has all three assets before announcing it.

Builds are unsigned: Windows SmartScreen ("More info → Run anyway") and macOS Gatekeeper (right-click → Open, or `xattr -dr com.apple.quarantine /Applications/DealDrop.app`) will warn on first launch.

Platform code: launcher detection (`has_scheme` in `launchers.rs`) is per-OS via `cfg` — `xdg-mime` on Linux, a registry query on Windows, an app-bundle check on macOS. Keep any new OS-specific code behind `cfg` so all three keep compiling.

## Themes & sale themes

- Themes are CSS variable sets keyed by `html[data-theme="…"]` in `renderer/style.css`; the JS lists live in `renderer/app.js` (`BASE_THEMES`, `SALE_THEMES`). Add a theme in both places.
- `live_sale` (from `detect_steam_sale`, the Steam spotlight banner) drives sale themes: first sight of a sale sets `settings.theme` + `lastSaleTheme`; a user's own pick afterwards sticks across restarts; when the sale ends the theme falls back to `baseTheme`.
- Falling-glyph effects live in `startFx` (canvas `#fx`); they respect `prefers-reduced-motion` and the `effects` setting.
- CSP forbids inline handlers (`onerror=`, `onclick=`): use listeners (see the global `error` handler for images, `data-href` for links).

## UI notes

- **Errors:** every source that fails lands in `Snapshot.source_errors` (full text, built by `failure()` in `main.rs`) and the sidebar shows only a red "Something went wrong" box with a **Copy error** button (`renderErrors` / `copyError` in `app.js`), below accounts and above navigation. Never print raw error text in the sidebar; healthy state shows nothing.
- **Window:** it has no OS frame, so edges/corners are our own `.rz` handles calling `startResizeDragging` (permission `core:window:allow-start-resize-dragging`); corners are bigger than edges so they win. Min size is 570×420; below the content's size the main area scrolls **vertically only**. Never add horizontal scrolling: shrink the sidebar (`clamp()` width) instead.
- **Accounts:** sidebar rows show the store avatar (Steam, GOG) or the store logo (`logo()` / `LOGO_PATH` in `app.js`, simple-icons, CC0); clicking the name/avatar opens `Acct.profile_url` through `open_in_app` (Steam client if installed, else browser). Epic has no public profile page, so it links to the account page.
- The titlebar logo is `renderer/logo.png` (a copy of `src-tauri/icons/64x64.png`, same art as the tray icon); keep them in sync if the icon changes.

## Launchers

`src-tauri/src/launchers.rs` is the single table of launchers per store (`options()`), their URL schemes, and the deep links (`link()`, unit-tested). `has_scheme()` asks the OS which are installed (`xdg-mime` / Windows registry / macOS app bundles); "Auto" prefers `native`, then the browser, and the user can force one (or the browser) in Settings → Launchers (`launcherGog` / `launcherEpic`). GOG offers only GOG Galaxy; saved Heroic/Lutris preferences fall back to the first installed option.

GOG/Epic CheapShark deal titles are looked up on click only; exact matches open through the configured launcher and misses keep the CheapShark browser redirect.

- **GOG native Linux client (expected ~2027):** assumed to keep the `goggalaxy://` scheme. If it uses another, change the `native` row in `GOG` and the `("gog", "native")` arm in `link()` — nothing else needs touching.

## Rules

- **External URLs shown to the user must be clickable.** Any external service URL presented in the UI (hints, errors, docs, sign-up pages) must be an actionable hyperlink/button using a hard-coded, reviewed HTTPS URL where possible. Never make the user type or copy a URL by hand (`<a data-href="https://…">`, handled by the delegated click listener in `renderer/app.js` → `open_url`).
- **Opening links is centralised and validated.** All links open in the user's default browser via `open_url` / `open_in_app` (`main.rs`), which accept only plain `https` URLs with a real hostname (`safe_https`: no `file:`/`javascript:`/custom schemes, no embedded credentials, no IPs/localhost). Never open a URL any other way, and never render remote content in an in-app webview.
- **The main webview is locked to the bundled UI** (`on_navigation` in `main.rs`) and only the `main` window has IPC permissions (`capabilities/default.json`); the Steam/GOG/Epic sign-in windows load the stores' own pages with no access to Tauri APIs.
- **Security-sensitive destinations** (API-key, sign-in and account pages) use only the service's canonical official domain, hard-coded in the source. Never build such a URL from remotely fetched content, and never put it in a plain-text message that could be mistyped (typosquatted sites have served malware this way).
- No secrets in the repo. The app has no API keys; sign-in tokens live only in the user's config file.
- Hot deals come from CheapShark only (IsThereAnyDeal was removed: its key setup kept failing for users). Avatars: Steam (public profile XML) and GOG (`userData.json`); Epic has no public avatar API.
- GamerPower attribution in the UI must remain an active link to [GamerPower](https://www.gamerpower.com). CheapShark offer links must use its redirect URLs, and requests need a descriptive User-Agent.
- **Secrets never go in URLs.** Send tokens/keys in headers, and strip URLs from `reqwest` errors that could carry one (`e.without_url()`). Deep links (`launchers::link`) only accept the store's own domain and plain slugs. Escape every remote string put into `innerHTML` (including attribute values).
- **Rounded window = transparent window.** `main.rs` builds the window `.transparent(true)`; `html`/`body` stay transparent and `#app` paints the background (and gets the radius when `data-round="on"`). Don't give `body` a background colour: with a transparent `<html>` it spreads over the whole canvas and squares the corners off again. macOS needs `macos-private-api` (enabled in `Cargo.toml` / `tauri.conf.json`).
- **Fonts are bundled** (`renderer/fonts/`, DM Sans + DM Mono, OFL licences alongside). Never link a font CDN: the CSP is `font-src 'self'` and the webview must not contact third parties. Latin subset only.
- **First-run tour:** `settings.onboarded` (Rust `Settings`) stays false until the tour is finished or skipped; keep any new setting the tour touches in both `Settings` and the JS defaults so `saveSettings` never drops it.
- No telemetry. All external calls go through `sources.rs` / `stores.rs`; the webview never calls external APIs.
- JSON sent to the UI is camelCase (`#[serde(rename_all = "camelCase")]`); keep it in sync with `renderer/app.js`.
- Config keys in `Db` deliberately match the old Electron `electron-store` names so upgrades keep settings.
- Each source returns `Result`; the poll keeps the last good data when one fails and surfaces errors in the UI where it matters (hot deals, wishlists).
- When an upstream API changes, fix it and extend the matching `#[ignore]` live test in `sources.rs`.
- Wishlist items get `cheapest` from CheapShark (`attach_cheapest`) when another store is strictly cheaper; the UI opens that link first.
- Version lives in four places: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, the User-Agent in `sources.rs`, and `X-AppImage-Version` in `src-tauri/dealdrop.desktop` (how Gear Lever shows the version).

## Known issues

See [TODO.md](TODO.md) (e.g. the KDE minimize animation flying to the pointer instead of the taskbar icon: a Plasma-side problem, not fixable from the app).

## Known unverified paths

- Windows and macOS builds are CI-built only (not yet run by the maintainer on those OSes); their `has_scheme` detection and the unsigned-install flow are untested.
- Start at login on Windows (`reg add` of a `Run` value) and macOS (LaunchAgent plist with `RunAtLoad`) has not been run on a real machine (only the plist text is unit-tested; see [TESTING.md](TESTING.md)). Linux was checked by unit test plus the written `.desktop` file.
- GOG/Epic wishlist sign-in works for the maintainer's accounts but follows the launchers' unofficial OAuth flows; if one breaks, the UI shows the error above the wishlist.
- Epic's `store/p/<slug>` deep link versus documented `store/product/<slug>` route is unverified. [Heroic issue #3342](https://github.com/Heroic-Games-Launcher/HeroicGamesLauncher/issues/3342) reports `/p/` works in the official Windows client; Linux without the official launcher falls back to the browser. See [TESTING.md](TESTING.md) for manual checks.
- Sale themes during a live sale have not been seen end to end.
