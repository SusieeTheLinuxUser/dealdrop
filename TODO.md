# TODO / known issues

## Next up (from the maintainer's v0.8.0 test, 2026-10-18)

- [ ] **Release v0.8.0** (all three platforms, see AGENTS.md → Releasing): bump the version in the four places, date the CHANGELOG `[Unreleased]` section, push `dev` and fast-forward `main`, run `scripts/release-linux.sh`, wait for the maintainer to OK the AppImage, then run `gh workflow run release.yml -f tag=v0.8.0` and check for all three assets.
- [ ] **Clean up branches:** delete the leftover agent branches `t3/project-status-review*` and the stale `.claude/worktrees/*` checkouts. Keep only `dev` and `main`.
- [ ] **Onboarding step 2 (Appearance) is cramped:** the theme chips sit right on top of the "Rounded window" row. Add spacing between them (`renderer/onboarding.css`).
- [ ] **Remove Heroic/Lutris from the GOG launcher choices:** they open an install dialog, not a store page. GOG cards now show only Browser unless GOG Galaxy is installed (`launcherOf` in `app.js`), so the Heroic/Lutris option in Settings → Launchers does nothing. Drop those rows from `GOG` in `launchers.rs` (plus `link()`, the tests and the AGENTS/README mentions) and from `tools/preview/mock-tauri.js`.
- [ ] **Hot deals for GOG/Epic in the launcher:** Steam deals now open in the Steam app. GOG and Epic deals still use CheapShark's redirect in the browser, because CheapShark gives no GOG id or Epic slug. That needs a title lookup.

Done in this pass: themed dropdowns (sort menus and the onboarding launcher picker), "Show me around" replaces Finish on the last onboarding step, GOG store cards show a full-width Browser button, Steam hot deals open in the Steam app.

## Minimize animation on KDE Plasma (Wayland) flies to the pointer

**Cause (confirmed 2026-10-18):** a Plasma bug, not ours. A panel set to *Dodge Windows* / auto-hide is never laid out while hidden, so the taskbar publishes each task button with a width of 0 (`iconGeometry` = `38,1402 0x30`). KWin's Magic Lamp treats that as "no target" and aims at the pointer. Same shape as upstream [KDE bug 513815](https://bugs.kde.org/show_bug.cgi?id=513815) (open). The older bugs 232612, 398541, 342873, 222420 are not this one.

**Workaround:** set the panel's Visibility to **Always Visible**; the animation then goes to the taskbar icon (tested by the maintainer).

The app can't fix it: `xdg_toplevel.set_minimized` has no target rectangle and only the taskbar may call `set_minimized_geometry`. DealDrop already uses `window.minimize()` (`window_minimize` in `main.rs`).

If it still happens with an always-visible panel, file a new bug with 513815 in See Also.

## Release status

- v0.7.9 has all three assets (AppImage, `.msi`, universal `.dmg`).
- Unverified end to end: sale themes during a live sale; launcher detection on Windows/macOS (desk research says `reg query HKCR\<scheme> /ve` and the app-bundle paths are right, not tested on a real machine). The Epic deep link `store/p/<slug>` is reported valid alongside the documented `store/product/<slug>` (not tested on Epic's launcher).

## Windows window frame (unverified, needs a real Windows machine)

Desk check (2026-10-18): the main window is undecorated + transparent and never calls `.shadow(..)`. In tao 0.37.1 the Windows shadow defaults to on, which may draw a native frame/white border and fight the 14px CSS corners. Candidate fix, **not applied** because it can't be tested on Linux: build the window with `let mut w = …transparent(true);` then `#[cfg(windows)] { w = w.shadow(false).no_redirection_bitmap(true); }` before `.on_navigation(..).build()` (add `#[allow(unused_mut)]` for Linux). Check on Windows first: is there a frame, are the corners clean, is there a ghost titlebar (Tauri issues 14764 / 14859), is there a first-show flash? Also check: toast from the Start Menu shortcut, `navigator.clipboard.writeText` from `tauri.localhost`, config.json ACL.
