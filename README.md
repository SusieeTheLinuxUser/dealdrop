# DealDrop

**Free, open source desktop app** that notifies you when Steam sales start, when free games drop on Epic/Steam/GOG, and when games on your Steam wishlist go on sale.

No accounts. No telemetry. No secrets. Runs in your system tray.

---

## Features

- **Steam sale alerts** — notified the moment a major seasonal sale (Summer, Winter, Autumn, Spring) goes live
- **Free game alerts** — Epic, Steam, and GOG giveaways detected automatically
- **Free weekend alerts** — Steam free-to-play weekends
- **Wishlist sync** — sign in via Steam OpenID, get notified when your wishlist games drop in price
- **Hot deals tab** — top discounts from IsThereAnyDeal (optional, free API key)
- **Sales calendar** — full 2026 schedule with countdown to next sale
- **Open in Steam or browser** — every game card has both options
- **System tray** — minimizes to tray, polls in the background on your chosen interval

---

## Install

### AppImage (Linux — recommended)

Download the latest `.AppImage` from [Releases](../../releases), then:

```bash
chmod +x DealDrop-*.AppImage
./DealDrop-*.AppImage
```

No installation needed. Move it wherever you like.

### Build from source

Requires [Node.js 18+](https://nodejs.org/).

```bash
git clone https://github.com/your-username/dealdrop
cd dealdrop
npm install
npm start              # run in dev mode
npm run dist           # build AppImage
npm run dist:win       # build Windows installer
npm run dist:mac       # build macOS DMG
```

---

## Security & privacy

DealDrop is designed so **no secrets ever live in the source code**.

- All user-provided API keys are stored locally in the OS user data dir, never in the repo
- Steam login uses OpenID — DealDrop never sees your password, only your public Steam ID
- All external API calls happen in the main process (Node.js), not the renderer
- No telemetry, no analytics, no data leaves your machine except to the APIs listed below
- Full details in [SECURITY.md](SECURITY.md)

**APIs DealDrop talks to:**

| Endpoint | Purpose | Auth |
|---|---|---|
| `store.steampowered.com/api/featured` | Detect free Steam games | None |
| `store.steampowered.com/api/featuredcategories` | Detect active sales | None |
| `store.steampowered.com/wishlist/profiles/{id}/wishlistdata` | Fetch wishlist | None (profile must be public) |
| `store.steampowered.com/api/appdetails` | Check wishlist prices | None |
| `api.steampowered.com/ISteamUser/GetPlayerSummaries` | Profile name & avatar | Steam Web API key (optional) |
| `store-site-backend-static.ak.epicgames.com/freeGamesPromotions` | Free Epic games | None |
| `api.isthereanydeal.com/deals/v2` | Hot deals | ITAD API key (optional) |
| `steamcommunity.com/openid/login` | Steam sign-in | Entered on Steam's own page |

---

## Optional API keys

Both are free and optional — core features work without them.

| Key | Where | Unlocks |
|---|---|---|
| Steam Web API key | [steamcommunity.com/dev/apikey](https://steamcommunity.com/dev/apikey) | Profile name & avatar in sidebar |
| IsThereAnyDeal key | [isthereanydeal.com/dev/app/](https://isthereanydeal.com/dev/app/) | Hot Deals tab (30+ stores) |

---

## Wishlist sync

1. Click **Sign in via Steam** in the sidebar
2. Log in on Steam's own page (DealDrop never handles your password)
3. Set your Steam wishlist to **Public** in Steam → Privacy Settings
4. DealDrop syncs and checks prices on your chosen interval

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). PRs welcome.

---

## License

[MIT](LICENSE) — free to use, modify, and distribute.
