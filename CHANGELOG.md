# Changelog

All notable changes to DealDrop are documented in this file.

## [v0.8.0] - 2026-10-18
- New: first-run setup guide (theme, notifications, stores) with an optional tour
- New: Steam games that are 100% off to keep now show up with the free games
- New: keyboard shortcuts (press ?), search, sort and minimum-discount filters on deal lists, "Updated N min ago" label
- New: wishlist search, sort and "on sale" filter; free games show when they end
- New: Wishlist shows Steam/GOG/Epic sign-in buttons when no store is connected
- New: your sort and filter choices and the last open page are remembered; "12 of 40" result counts
- Improved: loading placeholders instead of a blank list, smoother page changes, full titles on hover
- Improved: layout uses big windows well; animations respect "reduce motion"
- Improved: the error box sits right under your accounts, so Copy error is always in view
- Security: sign-in tokens are kept out of error messages; a sign-out during a refresh can't bring the old account back
- Improved: a store that fails keeps its last good results instead of emptying the list
- Improved: no burst of notifications on first run
- Improved: keyboard and screen-reader support, stronger text contrast in every theme
- Fixed: Steam sale calendar now uses Steam's published 2026–27 dates
- Fixed: sale themes, sign-out and reset keep the UI in sync
- Improved: themed dropdown menus; the guide's last button starts the tour; GOG games open in GOG Galaxy or the browser (Heroic and Lutris opened an install dialog instead of the store page, so they're gone); Steam hot deals open in the Steam app
- Privacy: fonts are bundled, so the app no longer contacts Google Fonts

## [v0.7.9] - 2026-10-16
- Fixed sidebar profile links to open user profiles properly in-app or browser
- Renamed "Check now" button and menu item to "Refresh all"
- Set minimum window dimensions to 570×420

## [v0.7.8] - 2026-10-16
- Added store logos, category tags, and direct profile links in the sidebar
- Added red error notification box with a "Copy error" button for failed sources
- Implemented custom border resize handles and adjusted minimum window sizing
- Added app icon to the titlebar and system tray

## [v0.7.7] - 2026-10-16
- Embedded AppImage update metadata for Gear Lever and AppImageUpdate integration

## [v0.7.6] - 2026-10-16
- Removed IsThereAnyDeal dependency and requirement for a Steam Web API key
- Added Steam and GOG user avatar images to the sidebar
- Switched deal search to keyless CheapShark API

## [v0.7.5] - 2026-10-16
- Fixed window background transparency so rounded window corners render correctly

## [v0.7.4] - 2026-10-16
- Hardened config file security by setting 0600 permissions upon creation
- Restricted OAuth sign-in windows strictly to HTTPS destinations
- Limited launcher deep links to canonical store domains and safe slugs

## [v0.7.3] - 2026-10-16
- Added X-AppImage-Version desktop entry to display the version number in Gear Lever

## [v0.7.2] - 2026-10-16
- Added an option for rounded window corners in Settings
- Improved error messages when API keys fail validation

## [v0.7.1] - 2026-10-16
- Added application version number display at the bottom of the sidebar

## [v0.7.0] - 2026-10-16
- Added official Windows (.msi) and macOS (.dmg) builds
- Implemented cross-platform launcher detection for Windows and macOS

## [v0.6.2] - 2026-10-16
- Restricted link opening exclusively to validated HTTPS URLs in the default browser
- Locked the main application webview strictly to bundled app UI

## [v0.6.1] - 2026-10-16
- Added launcher preference settings for GOG and Epic Games (Auto, native launcher, or browser)
- Added support for launching GOG games through Heroic Games Launcher and Lutris

## [v0.6.0] - 2026-10-16
- Added 6 base themes and automated seasonal Steam sale themes with canvas effects
- Linked wishlist items directly to the cheapest store via CheapShark
- Added quick-launch buttons to open games directly in installed store desktop clients
- Connected sale calendar entries directly to Steam sale event pages

## [v0.5.0] - 2026-10-16
- Rewrote backend in Rust using Tauri 2 for lower memory usage and improved performance
- Added dedicated Store Sales page for Steam, GOG, and Epic Games discounts
- Added GOG and Epic Games account sign-in and wishlist synchronization
- Added verification of Steam OpenID authentication assertions

## [v0.4.0 / v0.3.0] - 2026-05-11
- Fixed Steam OpenID login callback handling
- Improved Steam wishlist synchronization and price parsing

## [v0.2.1] - 2026-04-21
- Added game deal panel to wishlist items with live deal fetching

## [v0.2.0] - 2026-04-21
- Overhauled Electron desktop application core files and UI rendering

## [v0.0.1] - 2026-04-20
- Initial release tracking free games, store sales, and Steam wishlists with notifications
