# DealDrop Manual Testing Checklist

Short manual test checklist for Windows and macOS. Check each item as you verify it.

## 1. Installing Unsigned Builds
- [ ] **Windows (.msi)**: Run installer; on SmartScreen prompt click **More info -> Run anyway**.
- [ ] **macOS (.dmg)**: Open `.dmg`, drag DealDrop to `/Applications`. Unsigned build warning: right-click DealDrop -> **Open**, or run `xattr -dr com.apple.quarantine /Applications/DealDrop.app`.

## 2. First Launch & Window Management
- [ ] **Window launch**: App opens showing the main UI without native titlebar framing.
- [ ] **Tray icon**: DealDrop tray icon appears in system tray / menu bar.
- [ ] **Minimize / close**: Minimize button minimizes window; close button (`X`) closes to tray instead of quitting.
- [ ] **Tray interaction**: Clicking tray icon restores/toggles the window; right-click menu offers Open, Refresh all, Quit.
- [ ] **Resize handles**: Hovering edges and corners shows resize cursors; dragging resizes window (min size 570x420).
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
- [ ] **Receive notification**: Lower the check interval to 15 minutes in Settings (or wait/refresh during a new free game, >75% deal, or sale) and verify an OS desktop notification appears.

## 6. Store Accounts & Wishlists (Optional)
- [ ] **Sign-in windows**: Click "Sign in" for Steam, GOG, or Epic in the sidebar; store authentication window opens.
- [ ] **Account display**: After signing in, username and avatar/logo display in the sidebar.
- [ ] **Wishlist sync**: Wishlist tab displays synced games (Steam wishlist must be set to Public in Steam Privacy Settings).

## 7. Sale Themes
- [ ] **Theme activation**: Sale themes (and seasonal effects) are only visible during a live Steam sale (see TODO.md). Check if a sale banner / theme unlocks if a Steam sale is active.

## 8. Report Results Template

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
