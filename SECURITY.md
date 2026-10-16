# Security

## No secrets in this repo

DealDrop is designed so that **no secrets, API keys, or credentials ever live in the source code or repository**.

All user-provided keys (Steam Web API key, IsThereAnyDeal API key) and the GOG / Epic sign-in tokens are:
- Entered by the user at runtime through the Settings UI
- Stored locally in the OS config directory (file mode `0600`)
- Never committed to or read from the project folder

**Storage locations (OS user data dir, never the repo):**
| Platform | Path |
|---|---|
| Linux   | `~/.config/dealdrop/config.json` |
| Windows | `%APPDATA%\dealdrop\config.json` |
| macOS   | `~/Library/Application Support/dealdrop/config.json` |

## Store sign-in

- **Steam** uses **OpenID**. DealDrop opens the real `steamcommunity.com` login page, Steam redirects back with a signed assertion containing only your **public Steam ID**, and DealDrop asks Steam to confirm the signature (`check_authentication`) before trusting it. Your password is never seen.
- **GOG** and **Epic Games** use the same OAuth flow as their official launchers (public launcher client ids, as used by Heroic/Lutris/legendary). You type your password on gog.com / epicgames.com in a sign-in window; DealDrop only receives an access/refresh token, stored locally and used solely to read your wishlist. Sign out in the sidebar deletes it.

## API calls

All API calls are made by the **Rust backend**, never the webview. The UI talks to it through a fixed set of Tauri commands, and `open_url` only accepts `http`, `https` and `steam` links.

## Content Security Policy

Set in `src-tauri/tauri.conf.json`: scripts only from the app itself (no inline scripts, no `eval`), network access only through the Tauri IPC bridge, images from `https:` and fonts from Google Fonts.

## Reporting a vulnerability

Please open a GitHub issue tagged `[security]`. For sensitive disclosures, you can reach the maintainers via the email in the GitHub profile before going public.
