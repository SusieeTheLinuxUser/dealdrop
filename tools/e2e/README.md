# Headless end-to-end checks

Runs the **real release app** (WebKitGTK webview, Rust backend, live store APIs) where nothing reaches your desktop, and lets a Node script evaluate JS in the main webview over WebDriver. Linux/KDE only, dev-only, never bundled.

```bash
tools/e2e/run.sh tools/e2e/hidden-start.mjs     # --hidden start doesn't mark unseen pages seen
tools/e2e/run.sh tools/e2e/signin-title.mjs     # sign-in window titles show the site
tools/e2e/run.sh tools/e2e/close-to-tray.mjs   # closing to the tray fires visibilitychange and marks the page seen
tools/e2e/run.sh tools/e2e/loot.mjs            # Free DLC & loot: cards, no duplicates, not notified or counted
tools/e2e/run.sh tools/e2e/tray.mjs           # tray menu + counts + shortcut, icon file, new-free-game notification
DEALDROP_BIN=tools/e2e/flatpak.sh tools/e2e/run.sh tools/e2e/flatpak.mjs   # the installed Flatpak: host config/storage/autostart, update button
DEALDROP_BIN=/path/to/dealdrop tools/e2e/run.sh <check.mjs> [app args...]
```

`flatpak.sh` runs `flatpak run com.dealdrop.desktop` from your real per-user installation (install the bundle first: `flatpak install --user src-tauri/target/flatpak/DealDrop_*.flatpak`); everything it writes still lands in the throwaway home. `E2E_SCREENSHOT=/tmp/x.png` saves a screenshot of the window with the update button. Inside the private bus there is no `org.freedesktop.Flatpak` service, so `flatpak-spawn --host` (installing an update, launcher detection) can't be exercised here.

`tray.mjs` starts `fake-desktop.py` (Python + GObject introspection) on the private bus: a StatusNotifierWatcher like Plasma's and a notification server that log what the app sends. It works with either `DEALDROP_BIN`.

Needs `WebKitWebDriver` (webkit2gtk-4.1), `kwin_wayland`, `dbus-run-session`, `qdbus6`, `node`, and a release build (`cd src-tauri && cargo tauri build --no-bundle`). Exit status is 0 when every check passed.

What `run.sh` isolates, so a DealDrop you're running is untouched:

- a fresh `mktemp -d` as `HOME`, `XDG_*` and `XDG_RUNTIME_DIR` (config.json, WebKit storage, caches), deleted afterwards;
- a private D-Bus session with no activatable services (tray, notifications, portals, the single-instance lock);
- a private `kwin_wayland --virtual` as the only display (`DISPLAY` unset);
- one process group, killed on exit, failure or Ctrl-C.

`TAURI_WEBVIEW_AUTOMATION=true` makes Tauri let WebKit automation drive its first webview; a new session launches the app.

## Writing a check

```js
import { launch, check, done } from './wd.mjs'
const app = await launch(['--hidden'])          // default: the args given to run.sh
try {
  await app.waitFor('return typeof state === "object" && state.dataLoaded')
  check('free games listed', (await app.exec('return state.freeGames.length')) > 0, '…')
} finally { await app.quit() }
done()
```

- `exec`/`execAsync` take a function body. `app.js` top-level bindings (`state`, `navigate`, `markSeen`, …) and `window.api` are in scope.
- Windows the app opens later (sign-in) show up in `app.handles()`; `switchTo(handle)` to script them. `title()` is `document.title`; the native window title comes from KWin (`kwinTitles` in `signin-title.mjs`).
- WebDriver's close-window only drops the page and leaves the Tauri window open. Close windows through KWin (`kwinClose`), which works like the titlebar X.
