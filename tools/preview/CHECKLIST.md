# Preview coverage for TESTING.md

Start the local server as described in [README.md](README.md), then use `http://localhost:8000/tools/preview/`. Flags can be combined with `&`.

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
| Coach tour fit and highlights | No flag — open guide, choose Show me around; inspect at a 570×420 pane. |
| Keyboard shortcuts and typing guard | `?onboarded=1` — press `?`, test shortcuts, then type in search. |
| Bundled fonts offline | Needs a real build for offline confirmation; preview sources fonts locally, no external requests. |
| Free games, sales, and hot deals populate | `?onboarded=1` — inspect Free now, Store sales, and Hot deals. |
| Refresh all reloads | `?onboarded=1` — click Refresh all and observe the busy state. |
| Updated label counts and resets | `?onboarded=1&stale=1` — check 3-hour label, refresh, then check reset. |
| Steam free-to-keep game/DLC with Steam tag | `?onboarded=1` — Free now; inspect the Squad Weapon Skins DLC row. |
| Free game and weekend end dates/order | `?onboarded=1` — Free now; verify dated items sort by soonest end, show an Ends in chip, and undated items appear last. |
| Initial list loading before the first snapshot | Needs a real build (preview data resolves immediately); inspect Deals, Store sales, and signed-in Wishlist before the first poll completes. |
| Failed source keeps earlier rows | Needs a real build (requires a prior successful poll followed by a real source failure). |
| Error box placement and Copy error | `?onboarded=1&errors=1&accounts=1` — verify the red box sits below store accounts and above navigation, with Copy error fully visible. |
| Launcher installed detection | Needs a real build (reads OS launcher registration/install state). |
| Browser fallback | Needs a real build (opens the system browser). |
| Steam / GOG deep links | Needs a real build (requires installed launcher and protocol handling). |
| Epic deep link | Needs a real build (Epic route and launcher behavior are unverified). |
| No notification burst on first run | Needs a real build (requires backend polling and OS notifications). |
| Notification toggles | `?onboarded=1` — change toggles in Settings; preview verifies UI only. |
| Receive desktop notification | Needs a real build (OS notification and poll timing). |
| Store sign-in windows | Needs a real build (requires store OAuth windows/accounts). |
| Account name/avatar display | `?onboarded=1&accounts=1` — inspect sidebar account rows. |
| Wishlist sync and Steam privacy behavior | Needs a real build (requires real account data and Steam privacy state). |
| Wishlist search, sort, sale-only filter, and clear state | `?onboarded=1&accounts=1` — Wishlist; try store tabs, search for no match, clear it, and compare discount/price/name plus On sale only. |
| Minimum discount chips and Clear filters | `?onboarded=1` — Hot deals and Store sales; try Any, 50%+, 75%+, 90%+, then clear a no-match filter. |
| Live sale theme activation | `?onboarded=1&sale=winter` — inspect sale banner, theme, and effects. |
