# DealDrop

**Free, open source desktop app** that notifies you when Steam sales start, when free games drop on Epic/Steam/GOG, and when games on your Steam wishlist go on sale.

Optional store sign-in enables wishlist alerts. No API keys or telemetry; DealDrop runs in your system tray.

---

## Features

- **Steam sale alerts** — notified the moment a major seasonal sale goes live
- **Free game alerts** — Epic, Steam and GOG giveaways, de-duplicated across sources
- **Steam free-to-keep games** — includes qualifying DLC, clearly tagged as DLC
- **Free weekend alerts** — Steam free-to-play weekends
- **Store sales** — current discounts from **Steam, GOG and Epic Games**, browsable per store
- **Search and sort** — find deals, store sales and wishlist games by title, discount or price; discount chips and wishlist sale-only filters narrow results
- **Remembered list choices** — last page, sort order and discount toggles stay on this computer; search text is cleared when the app closes
- **Wishlists from Steam, GOG and Epic** — sign in once for alerts when games drop 20% or more; the empty Wishlist page offers sign-in buttons when no store is connected
- **Hot deals** — top discounts across 30+ stores (via CheapShark, no key needed)
- **Wishlist links go to the cheapest store** — like deals.gg, a wishlist game that is cheaper elsewhere shows the best offer and opens it
- **Themes** — 6 built-in themes (Midnight, Lavender, Spooktober, Forest, Graphite, Daylight), optional rounded window corners, plus a **seasonal theme with falling leaves / snow / petals / lanterns that unlocks and switches on automatically while each Steam sale is live** (your own pick is remembered)
- **Your accounts in the sidebar** — avatar (or store logo), store tag, click to open your profile; a red "Something went wrong" box with a **Copy error** button appears only when a source fails
- **Open in your launcher** — Steam and Epic, plus GOG Galaxy, Heroic or Lutris for GOG games; choose a preference in Settings
- **Sales calendar** — Steam seasonal sales with countdown; click one to open it in Steam
- **System tray** — minimizes to tray, polls in the background on your chosen interval
- **Welcome guide and keyboard shortcuts** — quick setup on first run, with shortcuts for common actions
- **Accessible updates and loading** — live result/refresh feedback and lightweight loading skeletons while the first snapshot arrives
- **Keyboard-friendly cards** — press Enter on a free game, store sale or hot deal to open its store page; launcher and browser buttons remain available
- **Responsive layouts** — card grids fill wide windows while keeping rows and text at a readable width
- **Bundled fonts** — DM Sans and DM Mono load locally, with no third-party font requests
- Native Rust backend (Tauri) — small AppImage, low memory

---

## Install

v0.7.9 ships all three builds. Download the file for your OS from [Releases](../../releases): Linux AppImage, Windows `.msi`, or universal macOS `.dmg` (Intel and Apple Silicon).

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

No installation needed. Linux requires `webkit2gtk-4.1`. Existing settings and Steam sign-in from the earlier Electron app are picked up automatically.

### Build from source

Requires Rust, `webkit2gtk-4.1`/GTK dev packages (Linux only), and `cargo install tauri-cli --version "^2"`.

```bash
git clone https://github.com/SusieeTheLinuxUser/dealdrop
cd dealdrop/src-tauri
cargo tauri dev                             # run in dev mode
cargo tauri build --bundles appimage        # Linux AppImage
cargo tauri build                           # Windows MSI or macOS DMG on that OS
```

Run the dependency-free renderer helper self-check from the repo root with `node tools/preview/selfcheck.mjs`.

---

## Security & privacy

DealDrop is designed so **no secrets ever live in the source code**.

- Sign-in tokens are stored locally in the OS user data dir, never in the repo
- Steam login uses OpenID — DealDrop never sees your password, only your public Steam ID
- External API calls run in the Rust/Tauri backend; the renderer does not call external APIs
- No telemetry or analytics; network requests go to Steam, GOG, Epic, GamerPower and CheapShark
- Full details in [SECURITY.md](SECURITY.md)

**APIs DealDrop talks to:**

| Endpoint | Purpose | Auth |
|---|---|---|
| `store.steampowered.com/api/featured`, `/featuredcategories` | Free Steam games, sale & free-weekend detection | None |
| `store.steampowered.com/search/results?specials=1` | Steam sales list | None |
| `api.steampowered.com/IWishlistService/GetWishlist` | Steam wishlist (app ids) | None (profile must be public) |
| `api.steampowered.com/IStoreBrowseService/GetItems` | Names & prices for many apps in one request | None |
| `steamcommunity.com/profiles/<id>?xml=1` | Profile name & avatar | None |
| `steamcommunity.com/openid/login`, `/profiles/{id}/?xml=1` | Steam sign-in, profile fallback, privacy check | Entered on Steam's own page |
| `catalog.gog.com/v1/catalog` | GOG sales | None |
| `auth.gog.com`, `embed.gog.com/user/wishlist.json`, `api.gog.com/products` | GOG sign-in, wishlist, prices | Entered on GOG's own page |
| `store-site-backend-static.ak.epicgames.com/freeGamesPromotions` | Free Epic games | None |
| `store.epicgames.com/graphql` | Epic sales | None |
| `account-public-service-prod03.ol.epicgames.com`, `launcher.store.epicgames.com/graphql` | Epic sign-in & wishlist | Entered on Epic's own page |
| `www.gamerpower.com/api/giveaways` | PC giveaway aggregation | None |
| `www.cheapshark.com/api/1.0` | Hot deals, cheapest store for a wishlist game | None |

---

## API keys

None needed — everything works without any key. Sign in to Steam, GOG and Epic from the sidebar; your name and avatar (Steam, GOG) show up there.

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
