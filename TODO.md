# TODO / known issues

## Next up

Left over from the 2026-10-18/19 overnight work (shipped in v0.9.0):

- **Cleanup done 2026-10-21** (date math, test SteamID, parser defaults, `loadJson`/`saveJson`, `makeHideButton`). Deliberately left: merging `keep_prices` into `carry_cheapest` (risks re-arming price targets); an `X-DealDrop-Exe` key in `autostart.rs` (changes the on-disk entry and still needs the `Exec=` reader for old ones); parsing CheapShark bodies once (mostly test churn); `settingRow`/dialog helpers (save about a line each).
- **Check for real** (see AGENTS.md "Known unverified paths" and TESTING.md §8): the tray on a real panel (its D-Bus side is checked by `tools/e2e/tray.mjs`), start at login from the Flatpak, the Flatpak updating itself, the sale-soon notification (2026-12-16).

## Minimize animation on KDE Plasma (Wayland) flies to the pointer

**Cause (confirmed 2026-10-18):** a Plasma bug, not ours. A panel set to *Dodge Windows* / auto-hide is never laid out while hidden, so the taskbar publishes each task button with a width of 0 (`iconGeometry` = `38,1402 0x30`). KWin's Magic Lamp treats that as "no target" and aims at the pointer. Same shape as upstream [KDE bug 513815](https://bugs.kde.org/show_bug.cgi?id=513815) (open). The older bugs 232612, 398541, 342873, 222420 are not this one.

**Workaround:** set the panel's Visibility to **Always Visible**; the animation then goes to the taskbar icon (tested by the maintainer).

The app can't fix it: `xdg_toplevel.set_minimized` has no target rectangle and only the taskbar may call `set_minimized_geometry`. DealDrop already uses `window.minimize()` (`window_minimize` in `main.rs`).

If it still happens with an always-visible panel, file a new bug with 513815 in See Also.

## Release status

- Since the Flatpak move: Linux Flatpak first, then Windows `.msi` and universal macOS `.dmg` from CI.
- Unverified end to end: sale themes during a live sale; launcher detection on Windows/macOS (desk research says `reg query HKCR\<scheme> /ve` and the app-bundle paths are right, not tested on a real machine). The Epic deep link `store/p/<slug>` is reported valid alongside the documented `store/product/<slug>` (not tested on Epic's launcher).

## Windows window frame (unverified, needs a real Windows machine)

Desk check (2026-10-18): the main window is undecorated + transparent and never calls `.shadow(..)`. In tao 0.37.1 the Windows shadow defaults to on, which may draw a native frame/white border and fight the 14px CSS corners. Candidate fix, **not applied** because it can't be tested on Linux: build the window with `let mut w = …transparent(true);` then `#[cfg(windows)] { w = w.shadow(false).no_redirection_bitmap(true); }` before `.on_navigation(..).build()` (add `#[allow(unused_mut)]` for Linux). Check on Windows first: is there a frame, are the corners clean, is there a ghost titlebar (Tauri issues 14764 / 14859), is there a first-show flash? Also check: toast from the Start Menu shortcut, `navigator.clipboard.writeText` from `tauri.localhost`, config.json ACL.
