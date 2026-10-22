# DealDrop

[![CI](https://github.com/SusieeTheLinuxUser/dealdrop/actions/workflows/ci.yml/badge.svg?branch=dev)](https://github.com/SusieeTheLinuxUser/dealdrop/actions/workflows/ci.yml)

**Free, open source desktop app** that notifies you when Steam sales start, when free games drop on Epic/Steam/GOG, and when games on your Steam wishlist go on sale.

Optional store sign-in enables wishlist alerts. No API keys or telemetry; DealDrop runs in your system tray.

---

## Features

- **Steam sale alerts** — notified the moment a major seasonal sale goes live, plus a heads-up the day before it starts
- **Free game alerts** — Epic, Steam and GOG giveaways, de-duplicated across sources; mark the ones you've claimed (dimmed, sorted last, left out of the count)
- **Coming next on Epic** — the next free Epic games and the date they turn free
- **Free DLC & loot** (optional) — in-game items and DLC giveaways from GamerPower, off by default; never notified or counted
- **Steam free-to-keep games** — includes qualifying DLC, clearly tagged as DLC
- **Free weekend alerts** — Steam free-to-play weekends
- **Store sales** — current discounts from **Steam, GOG and Epic Games**, browsable per store
- **Search everything** — Ctrl+K (⌘K on macOS) finds a game across free games, store sales, hot deals and wishlist and jumps to it
- **Search and sort** — find deals, store sales and wishlist games by title, discount or price; discount chips and wishlist sale-only filters narrow results
- **Remembered list choices** — last page, sort order and discount toggles stay on this computer; search text is cleared when the app closes
- **Wishlists from Steam, GOG and Epic** — sign in once for alerts when games drop 20% or more (you choose the discount); the empty Wishlist page offers sign-in buttons when no store is connected; long wishlists show 100 games at a time
- **Hot deals** — top discounts across 30+ stores (via CheapShark, no key needed) with their Steam rating, filterable by store, sortable by rating, with an optional alert above a discount you choose
- **"Lowest ever" badge** — hot deals and wishlist games at their all-time low price (CheapShark; USD prices only)
- **"New" tags** — hot deals, store sales and wishlist sales you haven't seen yet are tagged New, with a dot on the sidebar item
- **Hide games** — hide a title from hot deals, store sales or free games; it also drops out of notifications (hidden wishlist games stay listed but send no alerts); unhide in Settings
- **Wishlist price targets** — set "tell me at or below $X" on a wishlist game for one alert when it (or a cheaper store) gets there (USD prices)
- **Wishlist links go to the cheapest store** — like deals.gg, a wishlist game that is cheaper elsewhere shows the best offer and opens it
- **Themes** — 6 built-in themes (Midnight, Lavender, Spooktober, Forest, Graphite, Daylight), optional rounded window corners, plus a **seasonal theme with falling leaves / snow / petals / lanterns that unlocks and switches on automatically while each Steam sale is live** (your own pick is remembered)
- **Your accounts in the sidebar** — avatar (or store logo), store tag, click to open your profile; a red "Something went wrong" box with a **Copy error** button appears only when a source fails
- **Open in your launcher** — Steam and Epic, plus GOG Galaxy for GOG games; choose a preference in Settings
- **Sales calendar** — Steam seasonal sales with countdown; click one to open it in Steam
- **System tray** — minimizes to tray, polls in the background on your chosen interval; the tray menu jumps to Free now or Wishlist with their counts; optionally starts when you log in, hidden in the tray
- **Welcome guide and keyboard shortcuts** — quick setup on first run, with shortcuts for common actions (J / K to move between items in lists)
- **Accessible updates and loading** — live result/refresh feedback and lightweight loading skeletons while the first snapshot arrives
- **Keyboard-friendly cards** — press Enter on a free game, store sale or hot deal to open its store page; launcher and browser buttons remain available
- **Responsive layouts** — card grids fill wide windows while keeping rows and text at a readable width
- **Bundled fonts** — DM Sans and DM Mono load locally, with no third-party font requests
- **Safe settings** — an unreadable `config.json` is kept aside as `config.bad.json` instead of being overwritten
- Native Rust backend (Tauri) — small download, low memory
- **Updates itself** on Linux: when a new release is out, the sidebar shows **Update to vX.Y.Z**; one click installs it and restarts (Windows/macOS: opens the download page). Settings → About → **Check for updates** checks right away

---

## Install

Every release ships all three builds. Download the file for your OS from [Releases](../../releases): Linux Flatpak, Windows `.msi`, or universal macOS `.dmg` (Intel and Apple Silicon).

### Windows (.msi)

Run `DealDrop_*.msi`. Windows SmartScreen may warn because the build is unsigned — **More info → Run anyway**.

### macOS (.dmg, Intel + Apple Silicon)

Open `DealDrop_*.dmg` and drag DealDrop to Applications. Unsigned build: on first launch right-click → **Open**, or run `xattr -dr com.apple.quarantine /Applications/DealDrop.app`.

### Linux (Flatpak)

Download the latest `DealDrop_*_x86_64.flatpak` from [Releases](../../releases), then open it with your software center (Discover, GNOME Software) or run:

```bash
flatpak install --user DealDrop_*_x86_64.flatpak
flatpak run com.dealdrop.desktop
```

It needs Flatpak with the Flathub remote (the GNOME runtime is fetched from there on first install). After that DealDrop updates itself: click **Update to vX.Y.Z** in the sidebar when it appears.

Coming from the AppImage (v0.9.2 and older)? Install the Flatpak and delete the AppImage: settings, sign-ins and "Start when I log in" carry over (the Flatpak uses the same `~/.config/dealdrop/`). The AppImage gets no more updates.

The Flatpak is allowed to talk to the host's Flatpak (`org.freedesktop.Flatpak`) so it can install its own updates and see which game launchers you have; it can only read and write `~/.config/dealdrop`, `~/.local/share/com.dealdrop.desktop` and `~/.config/autostart`.

### Build from source

Requires Rust, `webkit2gtk-4.1`/GTK dev packages (Linux only), and `cargo install tauri-cli --version "^2"`.

```bash
git clone https://github.com/SusieeTheLinuxUser/dealdrop
cd dealdrop/src-tauri
cargo tauri dev                             # run in dev mode
../scripts/release-linux.sh                 # Linux Flatpak bundle (NO_UPLOAD=1 to only build it)
cargo tauri build                           # Windows MSI or macOS DMG on that OS
```

Run `./target/release/dealdrop --tui` for the read-only terminal view of free games, hot deals, and store sales. It needs an interactive terminal; Windows release builds have no console, so use a debug build there.

Run the dependency-free renderer helper self-check from the repo root with `node tools/preview/selfcheck.mjs`.

---

## Security & privacy

DealDrop is designed so **no secrets ever live in the source code**.

- Sign-in tokens are stored locally in the OS config dir (`~/.config/dealdrop/config.json` on Linux), never in the repo
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
