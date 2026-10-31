# AGENTS.md

DealDrop is a Tauri 2 desktop app: Rust backend (`src-tauri/`) + plain HTML/CSS/JS UI (`renderer/`, no bundler).
It polls store APIs, shows free games / store sales / hot deals / wishlists, and sends desktop notifications from the tray.

## Layout

| Path | What |
|---|---|
| `src-tauri/src/main.rs` | App state, persistence (`config.json` in the OS config dir: `~/.config/dealdrop` / `%APPDATA%\dealdrop` / `~/Library/Application Support/dealdrop`), poll loop, notifications, tray, Tauri commands (see Backend notes) |
| `src-tauri/src/sources.rs` | Every no-account HTTP source (Steam, GOG, Epic free, GamerPower giveaways + loot, CheapShark) + offline parser fixtures + live smoke tests |
| `src-tauri/src/stores.rs` | Sign-in window + Steam/GOG/Epic auth, GOG & Epic wishlists, Epic sales |
| `src-tauri/src/autostart.rs` | "Start when I log in": per-OS entry (XDG autostart file / `Run` registry value / LaunchAgent) that runs the app with `--hidden` (tray only); the OS entry is the only record of the setting. In the Flatpak the Linux entry is `Exec=flatpak run com.dealdrop.desktop --hidden`. `refresh()` (Linux, release builds only, on a thread at startup) repoints an entry whose quoted app path is gone; the Flatpak always takes over a quoted (AppImage-era) entry; everything else in the file (e.g. `Hidden=true`) is kept |
| `src-tauri/src/update.rs` | Flatpak detection (`/.flatpak-info`), `use_host_dirs` (points `XDG_CONFIG_HOME`/`XDG_DATA_HOME` back at the host's folders so config.json and webview storage are the AppImage-era files) and the self-update: `flatpak-spawn --host flatpak install --user/--system --reinstall --bundle`, then a host helper waits for this instance (`flatpak ps`) to exit and runs `flatpak run` |
| `flatpak/` | `com.dealdrop.desktop.yml` (manifest: GNOME 51 runtime, builds with the Rust SDK extension and network; libayatana-appindicator from the `shared-modules` git submodule for the tray), the desktop file and the AppStream metainfo (`@VERSION@`/`@DATE@` filled in at build time from `Cargo.toml`) |
| `src-tauri/src/launchers.rs` | Open-in-launcher table, per-OS install detection, deep links |
| `src-tauri/src/tui.rs` | Optional read-only terminal UI (`dealdrop --tui`) for public free games, hot deals, and store sales |
| `.github/workflows/ci.yml` | Every push to `dev`/`main` and every PR → `cargo test` + `node tools/preview/selfcheck.mjs` on Linux, `cargo test` on Windows and macOS |
| `.github/workflows/release.yml` | Manual dispatch → builds Windows .msi + macOS .dmg and attaches them to an existing release |
| `scripts/release-linux.sh` | Builds the Flatpak bundle `DealDrop_<ver>_x86_64.flatpak` (in `src-tauri/target/flatpak/`) and publishes it to the release (`NO_UPLOAD=1`: build only) |
| `renderer/api.js` | `window.api` → Tauri `invoke` bridge (the only place the UI calls backend commands; `app.js` also calls two Tauri window/app APIs directly) |
| `renderer/app.js` | All UI rendering. `renderer/index.html`, `renderer/style.css`, `renderer/onboarding.js` / `onboarding.css` (first-run guide + tour) |
| `tools/preview/` | Dev-only browser preview harness with a local mock Tauri bridge; never bundled. Mock flags: `?stress=1&accounts=1&bigwishlist=1&sale=<key>` (also `errors`, `empty`, `stale`, `onboarded`, `update`; see its README). `selfcheck.mjs` asserts the pure helpers in `app.js` |
| `tools/e2e/` | Dev-only headless checks of the real release app: `run.sh <check.mjs>` isolates it (temp home, private D-Bus, `kwin_wayland --virtual`) and the check drives the webview through WebKitWebDriver (see its README). `DEALDROP_BIN=tools/e2e/flatpak.sh … flatpak.mjs` drives the installed Flatpak |

## Backend notes

- **Persistence:** `load_db` moves an unreadable `config.json` aside to `config.bad.json` (`config.bad2.json`, … never overwriting an earlier one) and starts from defaults. If it can't be moved, `CONFIG_STUCK` is set: nothing is saved that run, and **Copy error** says why. A config write that fails later (permissions, full disk) is kept in `AppState.save_error` and shown the same way until a write succeeds.
- **Seen/notify bookkeeping** (`seen_free_ids`, `seen_sale_ids`, `seen_deal_ids` in `Db`): `remember_*` records hidden items as seen too, so unhiding one never notifies it as new. Hidden games are filtered out of notifications (`is_hidden`), not out of bookkeeping.
- **Price targets:** `Settings.price_targets` (`"store:id"` → USD, `normalize_targets`) and `Db.fired_targets` (key → the target that fired). `target_hits` fires once per target; it re-arms when the price rises above it or the target changes. Reset settings keeps hidden games and targets.
- **CheapShark:** every request goes through `cheapshark_get`, which shares one 429 cooldown (`CS_COOLDOWN`: Retry-After, else 15 min doubling to 2 h) and fails without touching the network while blocked; the poll keeps last good data, and `price_failure` drops the wishlist copy of the cooldown error so it shows once. Wishlist offers' stores come from the batched `games?ids=` lookup (never one `deals?id=` per item). `attach_cheapest` returns the items it got no answer for, and `carry_cheapest` keeps their last offer while the store's own price is unchanged, so a missed lookup doesn't re-arm a price target; any item with an unchanged price also keeps its last lowest-ever when none came back (a failed `games?ids=` batch); `carry_deal_lowest` does the same for hot deals (same dealID and price).

- **ProtonDB (Linux only):** `proton_tiers` in `main.rs` looks up `protondb_tier` (`sources.rs`, the public `reports/summaries/<appid>.json`; 404 = no rating) for every Steam appid listed, at most `PROTON_PER_POLL` per poll, 4 at a time, and caches the answer in `Db.proton` for a week (dropped after 30 days unlisted). `Snapshot.proton` (appid → tier) feeds the 🐧 tags in `app.js` (`protonTag`; only the five known tiers render) and single-game notifications (`proton_suffix`). A failed lookup keeps the old entry and only logs: it's not worth an error box.

## Commands

```bash
cd src-tauri
cargo check                                          # fast compile check
cargo test -- --ignored --nocapture --test-threads=1 # live API smoke tests (needs network)
cargo tauri dev                                      # run the app (needs `cargo install tauri-cli`)
NO_UPLOAD=1 ../scripts/release-linux.sh             # Flatpak bundle → src-tauri/target/flatpak/
./target/release/dealdrop --tui                      # terminal UI (Windows release builds have no console)
```

## Branches

- `main` = what has been released (the fallback; every release is also a `vX.Y.Z` tag). Don't commit work-in-progress to it.
- `dev` = day-to-day work. Commit and push here; to release, fast-forward `main` to `dev` (`git switch main && git merge --ff-only dev && git push`), then follow Releasing below and switch back to `dev`.
- GitHub rulesets: `main`/`dev` can't be deleted or force-pushed and need linear history (PRs merge by rebase or squash; merge commits are off). `v*` tags can't be moved or deleted.

## Releasing — ALWAYS all three platforms

Every release ships **Linux Flatpak + Windows .msi + macOS .dmg (universal: Intel + Apple Silicon)**. The file names matter: the in-app update check (`asset()` in `sources.rs`) only offers a release once this platform's `DealDrop_<ver>_x86_64.flatpak` / `DealDrop_<ver>_x64_en-US.msi` / `DealDrop_<ver>_universal.dmg` is attached. Never release just one.

1. On `dev`: bump the version in all three places (see Rules); update the docs; commit; then fast-forward `main` to `dev` and push both.
2. **Linux first:** `RELEASE_NOTES="…" scripts/release-linux.sh` builds the Flatpak bundle inside the GNOME SDK and publishes it to the release. Needs `flatpak` and `flatpak-builder` (or the `org.flatpak.Builder` Flatpak); the SDK and Rust extension are installed per-user from Flathub. The maintainer tests it. Installed Flatpaks see it on their next check (start, then every 6 h) and offer **Update to vX.Y.Z**.
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

- **Errors:** every source that fails lands in `Snapshot.source_errors` (built by `failure()` in `main.rs`, text capped at 400 chars; hot deals use `deals_error`, wishlists `wishlist_errors`) and the sidebar shows only a red "Something went wrong" box with a **Copy error** button (`renderErrors` / `copyError` in `app.js`), below accounts and above navigation. Never print raw error text in the sidebar; healthy state shows nothing.
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
- Version lives in three places: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, the User-Agent in `sources.rs`. The update check compares the `releases/latest` redirect's `vX.Y.Z` tag with `CARGO_PKG_VERSION`.
- **Updates:** checked through the `github.com/…/releases/latest` redirect, never the API (60 requests/hour per IP, easily used up behind a shared address). Only the Flatpak installs updates itself, and only from this repository's `releases/download/v<ver>/` URL; the UI never passes a URL to `install_update` (it re-checks). Other builds open the release page. Never add `--noninteractive` to `update::install`'s `flatpak install`: it makes polkit refuse a system-wide install (`install-bundle` is `auth_admin_keep`) instead of asking for the password; `-y` is enough.
- **Flatpak sandbox:** anything that must reach the host goes through `flatpak-spawn --host` (`update::install`, `has_scheme`'s `xdg-mime`); the tray icon file goes in `$XDG_RUNTIME_DIR/app/<id>` (the only runtime folder the host can read). A new host path needs a `--filesystem=` line in the manifest.

## Known issues

See [TODO.md](TODO.md) (e.g. the KDE minimize animation flying to the pointer instead of the taskbar icon: a Plasma-side problem, not fixable from the app).

## Known unverified paths

- Windows and macOS builds are CI-built only (not yet run by the maintainer on those OSes); their `has_scheme` detection and the unsigned-install flow are untested.
- Start at login on Windows (`reg add` of a `Run` value) and macOS (LaunchAgent plist with `RunAtLoad`) has not been run on a real machine (only the plist text is unit-tested; see [TESTING.md](TESTING.md)). Linux was checked by unit test plus the written `.desktop` file.
- GOG/Epic wishlist sign-in works for the maintainer's accounts but follows the launchers' unofficial OAuth flows; if one breaks, the UI shows the error above the wishlist.
- Epic's `store/p/<slug>` deep link versus documented `store/product/<slug>` route is unverified. [Heroic issue #3342](https://github.com/Heroic-Games-Launcher/HeroicGamesLauncher/issues/3342) reports `/p/` works in the official Windows client; Linux without the official launcher falls back to the browser. See [TESTING.md](TESTING.md) for manual checks.
- The Windows/macOS tray and tooltip (Linux AppIndicator has no tooltips). The tray on a real Plasma 6 panel was seen working on 2026-10-31 (v0.10.0 Flatpak: icon, menu labels and counts). Registration, the icon file, menu labels/counts and the "Wishlist" shortcut are checked headless against a fake StatusNotifierWatcher (`tools/e2e/tray.mjs`, release binary and Flatpak), as is sending a new-free-game notification; how Plasma draws them is not.
- Autostart at a real login from the Flatpak, and its `refresh()` taking over an AppImage entry; only `repaired_entry` is unit-tested.
- The Flatpak updating itself from a real release (download, then restart into the new version), including a system-wide install (polkit prompt); see TESTING.md §8 **Updating itself**. Checked by hand: `update::install`'s `flatpak-spawn --host flatpak install --user --reinstall --bundle` from inside a running instance succeeds and leaves that instance's `/app` readable, and the restart helper outlives the sandbox and starts after it exits.
- Notifications as the desktop shows them. Sending is checked headless against a fake notification server: price targets with a public Steam wishlist (`STEAM_ID=… tools/e2e/run.sh tools/e2e/price-target.mjs`: once, again on a changed target) and the "sale starts tomorrow" heads-up with the wall clock moved to the day before the next sale (`tools/e2e/sale-soon.mjs`, an `LD_PRELOAD` clock shim; release binary only).
- The Free DLC & loot section on a real desktop; the real app with live GamerPower data is checked headless (`tools/e2e/loot.mjs`: LOOT cards, no repeat of a Free to keep game, kept out of the notification and the tray count, hidden when switched off).
- `visibilitychange` on closing to the tray: checked headless (`tools/e2e/close-to-tray.mjs`, WebKitGTK 2.54 under `kwin_wayland --virtual`, release binary and Flatpak: hiding the window sets `document.hidden` and marks the open page seen). A never-shown `--hidden` window reports `document.hidden` and pauses `requestAnimationFrame` (what `state.pageShown` relies on; `tools/e2e/hidden-start.mjs`). Not yet seen on a real Plasma session.
- Sale-theme switching is unit-tested (`nextTheme` in `app.js`, asserted by `tools/preview/selfcheck.mjs`) and preview-checkable with `?sale=<key>`, but Steam spotlight banner detection (`detect_steam_sale`) during a real sale is still unverified.
