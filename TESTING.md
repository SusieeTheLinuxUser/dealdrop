# DealDrop Manual Testing Checklist

Short manual test checklist for Windows and macOS. Check each item as you verify it.

## 1. Installing Unsigned Builds
- [ ] **Windows (.msi)**: Run installer; on SmartScreen prompt click **More info -> Run anyway**.
- [ ] **macOS (.dmg)**: Open `.dmg`, drag DealDrop to `/Applications`. Unsigned build warning: right-click DealDrop -> **Open**, or run `xattr -dr com.apple.quarantine /Applications/DealDrop.app`.

## 2. First Launch & Window Management
- [ ] **Window launch**: App opens showing the main UI without native titlebar framing.
- [ ] **Tray icon**: DealDrop tray icon appears in system tray / menu bar.
- [ ] **Minimize / close**: Minimize button minimizes window; close button (`X`) closes to tray instead of quitting.
- [ ] **Tray interaction**: Clicking tray icon restores/toggles the window; right-click menu offers Open, Free now, Wishlist, Refresh all, Quit; hovering shows the free/wishlist counts (see §8 Tray).
- [ ] **Resize handles**: Hovering edges and corners shows resize cursors; dragging resizes window (min size 570x420).
- [ ] **Start at login**: In Settings -> Notifications, turn on **Start DealDrop when I log in**, sign out and back in: only the tray icon appears; opening DealDrop again shows the window. Turning it off removes the entry (Windows: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` value `DealDrop`; macOS: `~/Library/LaunchAgents/com.dealdrop.desktop.plist`).
- [ ] **Rounded corners**: In Settings -> Appearance, toggling **Rounded window** rounds the window corners.
- [ ] **First-run guide**: On a fresh install (no config file) the setup guide opens; Skip or Escape closes it and it does not come back on restart.
- [ ] **Guide again**: Settings -> About -> "Show welcome guide again" reopens it; choices made there show in Settings afterwards.
- [ ] **Tour**: "Show me around" on the last step highlights each item and fits in a 570x420 window.
- [ ] **Shortcuts**: `?` lists them; `r`, `/`, `1`-`5` and `,` work, and do nothing while typing in a search box.
- [ ] **Fonts offline**: With the network off, text still uses DM Sans / DM Mono (no fallback serif look).

## 3. Data Loading & Error Handling
- [ ] **Data feeds**: Verify Free games ("Free now"), Store sales, and Hot deals populate with items.
- [ ] **Refresh all**: Clicking "Refresh all" in the sidebar footer triggers a reload without crashing.
- [ ] **Updated label**: Next to Refresh all, "Updated N min ago" counts up and resets after a refresh.
- [ ] **Free to keep**: Steam games/DLC at 100% off appear under Free now with a Steam tag.
- [ ] **Failed source keeps rows**: If one source errors (error box shows), its earlier items stay listed instead of vanishing.
- [ ] **Error banner**: Note: healthy state shows nothing. If a source fails, verify the red **Something went wrong** box appears below the store accounts (above the navigation) with a **Copy error** button (clicking it copies error text to clipboard).

## 4. Store Launchers
> Windows detects launchers via registry (`HKCR\<scheme>`); macOS checks app bundles in `/Applications` and `~/Applications`.
- [ ] **Installed detection**: "Open in launcher" buttons (Steam / Epic / GOG) appear *only* when that launcher is installed.
- [ ] **Browser fallback**: When a launcher is not installed (or set to "Browser" in Settings), links open in default web browser.
- [ ] **Deep link test (Steam / GOG)**: Test opening at least one item in an installed launcher.
- [ ] **Epic deep link (UNVERIFIED)**: Test an Epic game link (`store/p/<slug>`). Record whether it opens the product page in Epic Games Launcher.

## 5. Desktop Notifications
> Note: There is no manual test notification button in the app.
- [ ] **No burst on first run**: A fresh install sends no free-game notifications on its first check.
- [ ] **Notification triggers**: Ensure notification toggles are enabled in Settings -> Notifications.
- [ ] **Receive notification**: Lower the check interval to 15 minutes in Settings (or wait/refresh during a new free game, a deal at the Big deal threshold (75% by default), or sale) and verify an OS desktop notification appears.
- [ ] **Sale heads-up**: The day before a Steam major sale (UTC date), one "Steam … Sale starts tomorrow" notification appears; restarting or refreshing that day does not repeat it.

## 6. Store Accounts & Wishlists (Optional)
- [ ] **Sign-in windows**: Click "Sign in" for Steam, GOG, or Epic in the sidebar; store authentication window opens.
- [ ] **Account display**: After signing in, username and avatar/logo display in the sidebar.
- [ ] **Wishlist sync**: Wishlist tab displays synced games (Steam wishlist must be set to Public in Steam Privacy Settings).

## 7. Sale Themes
- [ ] **Theme activation**: Sale themes (and seasonal effects) are only visible during a live Steam sale (see TODO.md). Check if a sale banner / theme unlocks if a Steam sale is active.

## 8. Unreleased features (manual checks)

One pass on Linux (KDE Plasma, Wayland) for everything in CHANGELOG.md [Unreleased], about 15 minutes in this order. Items marked *(Windows/macOS)* can't be checked on Linux.

**Before you start**
- Run `cargo tauri dev` from `src-tauri/` for everything except **Start at login**, which needs a built AppImage (`cargo tauri build --bundles appimage`).
- Dev builds have WebKit's inspector: right-click → **Inspect Element** → Console. It's used once below to force "New" tags.
- Config file: `~/.config/dealdrop/config.json`. Quit DealDrop from the tray (**Quit**) before editing it, or the app writes over your change. Keep a copy first.
- Sign in to at least Steam and Epic so the Wishlist has data.

### Hot deals: store filter (1 min)
- [ ] Hot deals → the **All stores** dropdown next to the sort lists every store alphabetically with a count, e.g. `GOG (12)`. Pick one: only that store's rows show and the "N of M" count appears.
- [ ] Go to another page and back, then restart: the store choice is still selected.
- [ ] Pick a store plus **90%+** until nothing matches: "No deals match." with **Clear filters**, which resets search, discount and store.

### "Lowest ever" (1 min)
- [ ] Hot deals: some rows show a **Lowest ever** tag after the discount (price equals CheapShark's historical low).
- [ ] Wishlist: discounted games at their all-time low show **Lowest ever** in both the "On sale" and the full list. Only USD prices are compared: Steam (up to 250 games, most discounted first) and Epic can show it, GOG only if your GOG account's currency is USD; Epic/GOG games are looked up by exact title, 30 at most, also most discounted first. A non-USD GOG price reads e.g. `5.00 EUR` and never gets the tag.

### Wishlist price targets (2 min)
- [ ] Wishlist: Tab to a USD game's **Target** button, press Enter: "Tell me at or below $" opens under the row with the cursor in the box. Enter `0`: "Enter a price from $0.01 to $10,000." Enter a price just above the game's current (or cheaper-elsewhere) price and press Enter: the row shows a green **Under target $X** tag in both lists and focus is back on Target.
- [ ] Within one poll (press **R**) one notification arrives: "Under your target: <title>", "<title> is $Y on <store> (target $X)". Press R again and restart the app: no repeat. Change the target to another price still above the current one: it notifies once more.
- [ ] Target, then Tab to **Clear** and press Enter: the tag goes away. Escape closes the editor without saving. A non-USD GOG game's editor says "Price targets work for USD prices only." The window at 570×420 has no horizontal scrollbar with the editor open.

### Hot deals: Steam rating and "Best rated" (½ min)
- [ ] Hot deals rows show the Steam rating, e.g. `92% · Very Positive` (just `92%` when the window is narrow); hovering shows "Steam reviews: 92% positive (Very Positive)". Deals without Steam reviews show nothing.
- [ ] Sort → **Best rated**: highest rating first, unrated deals last. The choice survives a restart.

### Free now: Mark claimed (1 min)
- [ ] Each giveaway card under Free now has **Mark claimed**. Click it: it reads **Claimed ✓**, the card dims and moves to the end, the Free now sidebar badge drops by one, and keyboard focus stays on the button.
- [ ] Click again to unclaim. Claim one and restart: it is still claimed.
- [ ] Free-weekend and "Coming next" cards have no claim button.

### Free now: "Coming next on Epic" (½ min)
- [ ] At the bottom of Free now, **Coming next on Epic** lists Epic's next free games with a **Free from <date>** tag instead of FREE, no claim button, and they are not counted in the sidebar badge. (The section only shows when Epic has announced upcoming free games.)

### "New" tags and sidebar dots (2 min)
- [ ] Force them: in the inspector Console run `localStorage.setItem('dealdrop-seen', JSON.stringify({deals:[],storesales:[],wishlist:[]})); location.reload()`. Small dots appear on **Hot deals**, **Store sales** and **Wishlist** (if any wishlist game is discounted; next to its count badge), and rows/cards show **New**.
- [ ] Open Hot deals: the tags stay while you're there. Go to another page: the Hot deals dot is gone; come back: no New tags. Same for Store sales (one dot covers all store tabs) and Wishlist.
- [ ] Force again, open Hot deals, close the window to the tray (X) and reopen from the tray: that page's tags are gone.
- [ ] A fresh install (or **Reset settings**) shows no New tags on its first load.

### Hide games and Settings → Hidden games (2 min)
- [ ] Hot deals: Tab to a row's **Hide** and press Enter. The row disappears and focus lands on the next row's Hide (the new last one if you hid the last row, the search box once the list is empty); the "N of M" count shows.
- [ ] Store sales: a card's **Hide** does the same. The hidden title (case and spacing ignored) also disappears from Free now and Store sales if it's listed there, and never appears in notifications. The Wishlist still lists it (its wishlist sale alert is suppressed).
- [ ] Free now: giveaway and free-weekend cards have **Hide** ("Coming next" cards don't). Hiding one moves focus to the next card's Hide, then to the other section's, and to the "Free now" heading once nothing is left; the sidebar badge drops by one.
- [ ] Settings → **Hidden games** shows "N hidden" and one row per title. **Unhide** brings the game back in Hot deals and focus moves to the Unhide now in its place; **Unhide all** empties the list, focus goes to the "Games you hide…" text.

### Ctrl+K search (2 min)
- [ ] Press **Ctrl+K** on any page, also while typing in a page's search box: a "Search free games, sales, deals and wishlist" box opens. Type a few letters: up to 20 results, titles starting with the text first, each with where it is (`Hot deals · GOG`, `Store sales · Steam`, `Wishlist · Epic`, `Coming next · Epic Games`…) and a price.
- [ ] Arrow keys move the selection; **Enter** goes to that page, puts the title in that page's search box and focuses the game's row or card. **Escape** or a click outside closes it and returns focus. `?` lists Ctrl+K.
- [ ] Filter reset: on Hot deals pick a store and **90%+**, then Ctrl+K a hot deal from another store below 90% → Enter: the dropdown is back on **All stores**, discount on **Any**, and the row shows. Pick a result the current filters already include: they are kept.
- [ ] Wishlist: select the GOG tab, Ctrl+K a Steam wishlist game → Enter switches to **All**. A Store sales result on another store switches to that store's tab.
- [ ] Hidden games don't appear in the results except Wishlist ones. Ctrl+K does nothing while the welcome guide or tour is open.

### Notification thresholds (1½ min)
- [ ] Settings → Notifications: **Wishlist game on sale** has a dropdown `at 10%+ off` … `75%` (default 20%); **Big deal** has 50/60/75/85/90 (default 75%). Each dropdown is greyed out while its toggle is off.
- [ ] Turn **Big deal** on at **50%+**, then **Refresh all**: one notification, "N deals at 50%+ off" (or "🔥 X% off: Title" for a single deal). A second refresh doesn't repeat it. Turn it back off.
- [ ] Set the wishlist threshold to **10%** and refresh: wishlist games 10–19% off you haven't been notified about produce a notification (nothing if there are none). Both choices survive a restart.

### "Steam sale starts tomorrow" (½ min)
- [ ] Settings → Notifications has **Steam sale starts tomorrow** ("A heads-up the day before a major sale"), on by default.
- [ ] It fires once, on the poll during the **UTC** day before a start date in `SALES` (`main.rs`, mirrors the calendar). The next is the Winter Sale on 2026-12-17, so it fires on 2026-12-16 UTC (01:00 Dec 16 to 01:00 Dec 17 in CET). It **can't be triggered tonight** without changing the system date. Once sent, config.json records the sale's start date as `"lastSaleSoonKey": "2026-12-17"` (`last_sale_soon_key` in the Rust `Db`), so restarts and refreshes that day don't repeat it.
- [ ] Optional force test: quit DealDrop, `sudo timedatectl set-ntp false && sudo timedatectl set-time '2026-12-16 12:00:00'`, start it and **Refresh all** → "Steam Winter Sale starts tomorrow" / "Dec 17 – Jan 4. Time to check your wishlist." Refresh again: nothing. Restore with `sudo timedatectl set-ntp true`; quit and delete `lastSaleSoonKey` to re-test. Also see §5 **Sale heads-up**.

### Tray: counts and shortcuts (1 min)
- [ ] Right-click the tray icon: **Open DealDrop**, **Free now (N)**, **Wishlist (N)**, **Refresh all**, **Quit**. N is free games plus free weekends (hidden games left out, claimed ones still counted: the tray can't see claims), and wishlist games at 20%+ off (hidden ones included, as on the Wishlist page); at zero the label has no "(0)". The counts update after Refresh all, and right away when you hide or unhide a free game.
- [ ] With the window closed to the tray, **Free now** / **Wishlist** opens it on that page.
- [ ] *(Windows/macOS)* Hovering the icon shows "DealDrop: 3 free games · 1 wishlist game on sale" (or just "DealDrop" at zero). Linux tray icons (AppIndicator) don't support tooltips, so KDE shows no counts on hover.

### Start DealDrop when I log in (2 min, AppImage)
- [ ] Run the built AppImage from where it will stay. Settings → Notifications → turn on **Start DealDrop when I log in**.
- [ ] `cat ~/.config/autostart/dealdrop.desktop`: `Exec="/path/to/DealDrop_….AppImage" --hidden` with the AppImage's own path (not `/tmp/.mount_…`), and `Icon=/home/<you>/.config/dealdrop/autostart-icon.png`. It also shows in System Settings → Autostart with the DealDrop icon (not a generic one).
- [ ] Log out and in: only the tray icon appears, no window. Tray → Open DealDrop shows it; starting the AppImage again while it runs shows the window too.
- [ ] Turn it off: the file and `autostart-icon.png` are gone. Delete the file by hand instead and reopen Settings: the toggle is off.
- [ ] Don't turn it on from `cargo tauri dev`: the entry would point at `target/debug/dealdrop`, which needs the dev server, so it would start blank at login.
- [ ] *(Windows/macOS)* See §2 **Start at login**.

### Accessibility (1 min)
- [ ] Focus after Hide/Unhide: covered above.
- [ ] Tab through Settings from the top: theme chips, toggles and threshold dropdowns, interval buttons, launcher choices, Sign out, Unhide, About links are all reachable in order, each with a visible focus ring; Space/Enter works on each. (With Orca: theme, interval and launcher choices say "pressed" on the selected one; Sign out says which store.)

### J / K list navigation (½ min)
- [ ] On Hot deals, Store sales, Wishlist and Free now, `j` focuses the first item (then the next), `k` the previous (or the last if nothing is focused); the focus ring is visible, the list scrolls to follow, Enter opens the item. `j` typed in a search box just types; on a long wishlist it stops at the last row before **Show more games**.

### Free DLC & loot (1 min)
- [ ] Free now has no **Free DLC & loot** section by default. Settings → Free now → **Show free DLC & loot** on: within a few seconds the section appears at the bottom of Free now with **LOOT** cards (GamerPower). The Free now badge and tray count don't change, and no notification fires.
- [ ] **Hide** on a loot card removes it; Ctrl+K finds a loot title ("Free DLC & loot · …"). Toggle off: the section and its search results go away.

### Daylight theme (½ min)
- [ ] Settings → Appearance → **Daylight**: FREE/WEEKEND and discount tags, **New**, **Lowest ever**, **Claimed ✓**, cheaper-store prices, Steam badges, sidebar badges, hints, counts, dates and the version label are all easy to read; nothing pale grey on white.

### Steam profile names with "]" (1 min)
- [ ] On Steam, temporarily set your profile name to something like `[TAG] Name ]`. In DealDrop sign out of Steam and sign in again (the name is only read at sign-in): the sidebar shows the full name, brackets included. Change the name back. (A Steam sign-in clears the "already notified" wishlist list, so a wishlist sale notification may follow.)

### Epic sign-in after a token refresh (1½ min)
- [ ] Signed in to Epic: quit DealDrop, and in config.json set `"expires_at": 0` inside the `"epic"` object. Start DealDrop: the sidebar still shows your Epic display name (not "Epic user"), the Epic wishlist loads without an error box, and config.json's `epic` now has a future `expires_at` and a non-empty `refresh_token`.
- [ ] Repeat once (the refreshed token must refresh too). Do the same with `"gog"`: the GOG sign-in code was refactored as well.

### Other unreleased changes (optional)
- [ ] With GOG Galaxy (or an Epic launcher) installed, clicking a GOG/Epic hot deal opens it in the launcher when the exact title is found, otherwise CheapShark's page in the browser. See §4.
- [ ] A Steam wishlist of 250+ games loads its cheaper-store prices without a CheapShark error in the error box.
- [ ] CheapShark rate limit (only if it happens): Hot deals keeps the last list and the error says "CheapShark is rate-limiting requests; trying again in N min (HH:MM UTC)". Refreshing before that time sends no CheapShark request; after it, Hot deals loads again on its own.

## 9. Report Results Template

Copy and fill this template when submitting test results:

```text
- OS: [Windows 10/11 / macOS Intel / Apple Silicon version]
- DealDrop Version: [the version shown in the sidebar]
- Passed:
  - [list features that passed]
- Failed / Issues:
  - [list features that failed]
- Epic deep link behavior: [Did store/p/<slug> open product page in Epic Launcher? Yes/No/Details]
- Copied Error Text:
  [Paste output from "Copy error" button, or "None"]
```
