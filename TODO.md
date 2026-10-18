# TODO / known issues

## Minimize animation on KDE Plasma (Wayland) flies to the pointer

**Cause (confirmed 2026-10-18):** a Plasma bug, not ours. A panel set to *Dodge Windows* / auto-hide is never laid out while hidden, so the taskbar publishes each task button with a width of 0 (`iconGeometry` = `38,1402 0x30`). KWin's Magic Lamp treats that as "no target" and aims at the pointer. Same shape as upstream [KDE bug 513815](https://bugs.kde.org/show_bug.cgi?id=513815) (open). The older bugs 232612, 398541, 342873, 222420 are not this one.

**Workaround:** set the panel's Visibility to **Always Visible**; the animation then goes to the taskbar icon (tested by the maintainer).

The app can't fix it: `xdg_toplevel.set_minimized` has no target rectangle and only the taskbar may call `set_minimized_geometry`. DealDrop already uses `window.minimize()` (`window_minimize` in `main.rs`).

If it still happens with an always-visible panel, file a new bug with 513815 in See Also.

## Release status

- v0.7.9 has all three assets (AppImage, `.msi`, universal `.dmg`).
- Unverified end to end: sale themes during a live sale; launcher detection on Windows/macOS (desk research says `reg query HKCR\<scheme> /ve` and the app-bundle paths are right, not tested on a real machine). The Epic deep link `store/p/<slug>` is reported valid alongside the documented `store/product/<slug>` (not tested on Epic's launcher).
