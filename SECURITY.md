# Security

## No secrets in this repo

DealDrop is designed so that **no secrets, API keys, or credentials ever live in the source code or repository**.

All user-provided keys (Steam Web API key, IsThereAnyDeal API key) are:
- Entered by the user at runtime through the Settings UI
- Stored locally by `electron-store` in the OS user data directory
- Never committed to or read from the project folder

**Storage locations (OS user data dir, never the repo):**
| Platform | Path |
|---|---|
| Linux   | `~/.config/dealdrop/config.json` |
| Windows | `%APPDATA%\dealdrop\config.json` |
| macOS   | `~/Library/Application Support/dealdrop/config.json` |

## Steam login

Authentication uses **Steam OpenID** — the same open standard used by thousands of websites. The flow:

1. DealDrop opens a real `steamcommunity.com` login page in a child window
2. The user logs in directly on Steam's servers
3. Steam redirects back with a signed assertion containing only the user's **public Steam ID**
4. DealDrop never sees, handles, or stores the user's Steam password

The Steam ID is public information (visible on any public profile URL). DealDrop uses it solely to fetch the public wishlist endpoint.

## API calls

All API calls are made from the **main (Node.js) process**, not the renderer. This means:
- No CORS issues
- The renderer never directly touches external APIs
- All data flows through IPC with `contextIsolation: true` and `nodeIntegration: false`

## Content Security Policy

The renderer enforces a strict CSP (see `renderer/index.html`):
- No inline scripts (except `unsafe-inline` for the bundled app.js — this should be moved to a proper bundler in future)
- External resources restricted to Google Fonts and `https:` image sources
- No `eval`, no remote code execution

## Reporting a vulnerability

Please open a GitHub issue tagged `[security]`. For sensitive disclosures, you can reach the maintainers via the email in the GitHub profile before going public.
