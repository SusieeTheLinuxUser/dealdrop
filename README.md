# DealDrop

**Free, open source desktop app** that notifies you when Steam sales start, when free games drop on Epic/Steam/GOG, and when games on your Steam wishlist go on sale.

No accounts. No telemetry. No secrets. Runs in your system tray.

---

## Features

- **Steam sale alerts** — notified the moment a major seasonal sale goes live
- **Free game alerts** — Epic, Steam and GOG giveaways, de-duplicated across sources
- **Free weekend alerts** — Steam free-to-play weekends
- **Store sales** — current discounts from **Steam, GOG and Epic Games**, browsable per store
- **Wishlists from Steam, GOG and Epic** — sign in once, get notified when a wishlist game drops ≥20%
- **Hot deals** — top discounts across 30+ stores. Works out of the box (CheapShark); add a free IsThereAnyDeal key to switch to ITAD. If a key is rejected the app tells you why
- **Wishlist links go to the cheapest store** — like deals.gg, a wishlist game that is cheaper elsewhere shows the best offer and opens it
- **Themes** — 6 built-in themes (Midnight, Lavender, Spooktober, Forest, Graphite, Daylight), optional rounded window corners, plus a **seasonal theme with falling leaves / snow / petals / lanterns that unlocks and switches on automatically while each Steam sale is live** (your own pick is remembered)
- **Open in your launcher** — Steam, Epic Games Launcher and GOG (GOG Galaxy / Heroic / Lutris) buttons appear when installed; pick your preferred one per store in Settings, ready for GOG's native Linux client
- **Sales calendar** — Steam seasonal sales with countdown; click one to open it in Steam
- **System tray** — minimizes to tray, polls in the background on your chosen interval
- Native Rust backend (Tauri) — small AppImage, low memory

---

## Install

Every release ships all three: grab the file for your OS from [Releases](../../releases).

### Windows (.msi)

Run `DealDrop_*.msi`. Windows SmartScreen may warn because the build is unsigned — **More info → Run anyway**.

### macOS (.dmg, Intel + Apple Silicon)

Open `DealDrop_*.dmg` and drag DealDrop to Applications. Unsigned build: on first launch right-click → **Open**, or run `xattr -dr com.apple.quarantine /Applications/DealDrop.app`.

### Linux (AppImage)

Download the latest `.AppImage` from [Releases](../../releases), then:

```bash
chmod +x DealDrop_*.AppImage
./DealDrop_*.AppImage
```

No installation needed. Needs `webkit2gtk-4.1` on the host (present on most desktop distros). Existing settings and Steam sign-in from the Electron versions are picked up automatically.

### Build from source

Requires Rust, `webkit2gtk-4.1`/GTK dev packages (Linux only), and `cargo install tauri-cli --version "^2"`.

```bash
git clone https://github.com/SusieeTheLinuxUser/dealdrop
cd dealdrop/src-tauri
cargo tauri dev                             # run in dev mode
cargo tauri build                           # build for your current OS
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
| `store.steampowered.com/api/featured`, `/featuredcategories` | Free Steam games, sale & free-weekend detection | None |
| `store.steampowered.com/search/results?specials=1` | Steam sales list | None |
| `api.steampowered.com/IWishlistService/GetWishlist` | Steam wishlist (app ids) | None (profile must be public) |
| `api.steampowered.com/IStoreBrowseService/GetItems` | Names & prices for many apps in one request | None |
| `api.steampowered.com/ISteamUser/GetPlayerSummaries` | Profile name & avatar | Steam Web API key (optional) |
| `steamcommunity.com/openid/login`, `/profiles/{id}/?xml=1` | Steam sign-in, profile fallback, privacy check | Entered on Steam's own page |
| `catalog.gog.com/v1/catalog` | GOG sales | None |
| `auth.gog.com`, `embed.gog.com/user/wishlist.json`, `api.gog.com/products` | GOG sign-in, wishlist, prices | Entered on GOG's own page |
| `store-site-backend-static.ak.epicgames.com/freeGamesPromotions` | Free Epic games | None |
| `store.epicgames.com/graphql` | Epic sales | None |
| `account-public-service-prod03.ol.epicgames.com`, `launcher.store.epicgames.com/graphql` | Epic sign-in & wishlist | Entered on Epic's own page |
| `www.gamerpower.com/api/giveaways` | PC giveaway aggregation | None |
| `www.cheapshark.com/api/1.0` | Hot deals fallback, cheapest store for a wishlist game | None |
| `api.isthereanydeal.com/deals/v2` | Hot deals (preferred when a key is set) | ITAD API key (optional) |

---

## Optional API keys

Both are free and optional — everything works without them.

| Key | Where | Unlocks |
|---|---|---|
| Steam Web API key | [steamcommunity.com/dev/apikey](https://steamcommunity.com/dev/apikey) | Profile name & avatar in sidebar |
| IsThereAnyDeal key | [isthereanydeal.com/apps/my](https://isthereanydeal.com/apps/my) | Hot Deals from ITAD instead of CheapShark |

**Getting the ITAD key:** register an app there, choose **“This app will NOT store OAuth secret securely”**, and copy the key listed under **API Keys** (not the Client ID / Secret). Always use the official link above.

---

## Wishlists

- **Steam** — click *Sign in* next to Steam, log in on Steam's page, and set your wishlist to **Public** in Steam → Privacy Settings.
- **GOG** / **Epic Games** — click *Sign in* next to the store and log in on its own page. The wishlist then appears in the same tab, filterable per store.

> GOG and Epic wishlist sync use the launchers' public OAuth flows. If a store changes its API the wishlist tab shows the error instead of staying silently empty.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). PRs welcome.

---

## License

[MIT](LICENSE) — free to use, modify, and distribute.
