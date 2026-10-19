# Preview coverage for TESTING.md

Start the local server as described in [README.md](README.md), then use `http://localhost:8000/tools/preview/`. Flags can be combined with `&`. Run `node tools/preview/selfcheck.mjs` from the repo root for helper assertions.

| TESTING.md item (sections 2–7) | Preview or real-build check |
|---|---|
| Window launch without native titlebar | Needs a real build (native window chrome). |
| Tray icon appears | Needs a real build (OS tray integration). |
| Minimize / close to tray | Needs a real build (native window behavior). |
| Tray interaction and menu | Needs a real build (OS tray menu). |
| Resize handles and 570×420 minimum | Needs a real build (native resize constraints); preview can be pane-sized only. |
| Rounded corners setting | `?onboarded=1` — toggle in Settings; inspect corners (browser transparency may differ). |
| First-run guide and Skip/Escape persistence | No flag — test guide dismissal; persistent config across restart needs a real build. |
| Guide again from Settings | `?onboarded=1` — Settings → About → Show welcome guide again. |
| Guide fit and tour highlights | No flag — Notifications/Stores keep actions visible at 570×420; Show me around highlights all four targets and matches current UI copy. |
| Keyboard shortcuts and typing guard | `?onboarded=1` — press `?`, try `/` on Wishlist/Store sales/Hot deals, then confirm shortcuts are ignored while typing. |
| Bundled fonts offline | Needs a real build for offline confirmation; preview sources fonts locally, no external requests. |
| Free/sale/deal cards, tags, prices and keyboard open | `?onboarded=1` — inspect all three pages, verify DLC/end-date order, tab to a card, press Enter, and compare badge placement and old/current/discount order. |
| Refresh all reloads | `?onboarded=1` — click Refresh all; verify it disables and says “Refreshing…” then returns to “Refresh all”. |
| Updated label counts and resets | `?onboarded=1&stale=1` — check 3-hour label; refresh and verify “Updated just now”. |
| Initial skeleton loading before first snapshot | Needs a real build (preview data resolves immediately); inspect three skeleton cards/rows on Free, Store sales, Hot deals, and signed-in Wishlist before the first poll completes. |
| Live announcements and result counts | `?onboarded=1&accounts=1` — search/sort/filter each list; verify its result count and screen-reader status, then refresh and copy an error. |
| Remembered page and list choices | `?onboarded=1` — change page, sort, and filter, reload to confirm restore; search text should be blank. Reset settings should clear the saved UI choices. |
| Page navigation motion and scroll reset | `?onboarded=1` — switch pages after scrolling; content returns to top with a brief 120 ms entrance. Reduced motion should make it instant. |
| Truncated title tooltips | `?onboarded=1&stress=1` — hover/focus long card, deal, and wishlist titles; full title should be available. |
| Failed source keeps earlier rows | Needs a real build (requires a prior successful poll followed by a real source failure). |
| Error box placement and Copy error | `?onboarded=1&errors=1&accounts=1` — verify the red-tinted box sits below store accounts and above navigation, with Copy error fully visible. |
| Settings labels and hints | `?onboarded=1` — Settings; verify theme, reset, interval, and refresh helper text is visible. |
| Launcher installed detection | Needs a real build (reads OS launcher registration/install state). |
| Browser fallback | Needs a real build (opens the system browser). |
| Steam / GOG deep links | Needs a real build (requires installed launcher and protocol handling). |
| Epic deep link | Needs a real build (Epic route and launcher behavior are unverified). |
| Notification timing and first-run burst | Needs a real build (requires backend polling and OS notification delivery). |
| Notification toggles | `?onboarded=1` — change toggles in Settings; preview verifies UI only. |
| Store sign-in windows | Needs a real build (requires store OAuth windows/accounts). |
| Account display and wishlist sign-in states | Skip the guide to see three no-account sign-in actions; `?onboarded=1&accounts=steam` tests the partial hint; `?accounts=1` shows all stores signed in. |
| Wishlist sync and Steam privacy behavior | Needs a real build (requires real account data and Steam privacy state). |
| Wishlist search, sort, sale-only filter, and clear state | `?onboarded=1&accounts=1` — Wishlist; try store tabs, search for no match, clear it, and compare discount/price/name plus On sale only. |
| Minimum discount chips and Clear filters | `?onboarded=1` — Hot deals and Store sales; try Any, 50%+, 75%+, 90%+, then clear a no-match filter. |
| Responsive layout at narrow and wide sizes | `?stress=1&onboarded=1` — inspect every page at 570×420, 520px, 1280×800 and 1600×1000; check long text, $1234.56, toolbars, and grid fill. |
| Reduced motion and scrolling | `?onboarded=1` — emulate `prefers-reduced-motion: reduce`; verify transitions, scrolling, and tour spotlight movement are instant. |
| Live sale theme activation | `?onboarded=1&sale=<key>` (lunar, spring, summer, halloween, autumn, winter) — inspect sale banner, theme, "· live" chip, and effects; reload without `sale` and confirm the base theme returns. |
