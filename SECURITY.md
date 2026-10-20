# Security

## No secrets in this repo

DealDrop is designed so that **no secrets, API keys, or credentials ever live in the source code or repository**.

DealDrop needs no API keys. The GOG / Epic sign-in tokens (and your Steam ID) are:
- Obtained at runtime through the stores' own sign-in pages
- Stored locally in the OS config directory (file mode `0600`)
- Never committed to or read from the project folder
- Stored as plain JSON (not in the OS keyring): anyone who can read your user files can read them. The file is created `0600` on Linux/macOS and lives in your user profile on Windows. Tokens are sent in request headers, never in URLs, so they can't leak through error messages.

**Storage locations (OS config dir, never the repo):**
| Platform | Path |
|---|---|
| Linux   | `~/.config/dealdrop/config.json` |
| Windows | `%APPDATA%\dealdrop\config.json` |
| macOS   | `~/Library/Application Support/dealdrop/config.json` |

## Store sign-in

- **Steam** uses **OpenID**. DealDrop opens the real `steamcommunity.com` login page, Steam redirects back with a signed assertion containing only your **public Steam ID**, and DealDrop asks Steam to confirm the signature (`check_authentication`) before trusting it. Your password is never seen.
- **GOG** and **Epic Games** use the same OAuth flow as their official launchers (public launcher client ids, as used by Heroic/Lutris/legendary). You type your password on gog.com / epicgames.com in a sign-in window; DealDrop only receives an access/refresh token, stored locally and used only to read your wishlist and show your account name (and GOG avatar). Sign out in the sidebar deletes it.

## API calls

All API calls are made by the **Rust backend**, never the webview. The UI talks to it through a fixed set of Tauri commands, and `open_url` / `open_in_app` only accept plain `https` links with a real hostname (no credentials, IPs, `localhost` or other schemes) and open them in your default browser (`open_in_app` uses the matching installed store launcher when there is one). Launcher deep links (`steam://`, `goggalaxy://`, …) are built only from the store's own domain and plain slugs/numeric ids.

## Content Security Policy

Set in `src-tauri/tauri.conf.json`: scripts only from the app itself (no inline scripts, no `eval`), network access only through the Tauri IPC bridge, images from `https:` and fonts only from the app itself (`font-src 'self'`, bundled in `renderer/fonts/`). The main window can't navigate away from the bundled UI, only it has access to the Tauri API, and the sign-in windows are limited to `https`.

## Releases & dependencies

The release workflow pins every GitHub Action to a commit SHA. `cargo audit` reports no known vulnerabilities (two transitive "unmaintained/unsound" notices from Tauri's Linux GTK stack). Windows and macOS builds are unsigned.

## Reporting a vulnerability

Please open a GitHub issue tagged `[security]`. For sensitive disclosures, you can reach the maintainers via the email in the GitHub profile before going public.
