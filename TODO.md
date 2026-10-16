# TODO / known issues

## Minimize animation on KDE Plasma (Wayland) flies to the wrong place

**Symptom:** on KDE Plasma 6.8 (Wayland, Magic Lamp effect) clicking DealDrop's minimize button sucks the window into the mouse pointer / the minimize button instead of its taskbar icon. Minimizing itself works and the taskbar icon restores the window.

**What we found (2026-10-16):**
- KWin aims the minimize effect at the icon position the Plasma taskbar publishes for each window. For DealDrop that position is unusable (`window.iconGeometry` = `38,1402 0x30`, zero width), so KWin falls back to the pointer. A freshly started GTK app (Volume Control) had the same zero-width result.
- Pinning DealDrop to the taskbar did **not** help (tested by the maintainer).
- KWin's scripting API exposes `iconGeometry` read-only, and the Wayland protocol only lets the taskbar set it, so the app cannot fix it itself.
- DealDrop already uses the normal minimize call (`window.minimize()` via `window_minimize` in `main.rs`) and matches its launcher entry (`StartupWMClass=dealdrop`).
- Related KDE reports: bugs 232612, 398541, 342873, 222420 on bugs.kde.org.

**Ideas if someone picks this up:**
- File a KDE bug with the evidence above.
- Re-test on newer Plasma/KWin releases; it may already be fixed upstream.
- Fallback: hide/restore without animation (loses the taskbar-minimize feel).

## Release status

- Windows `.msi` / macOS `.dmg` builds exist only up to v0.7.5; build newer ones with `gh workflow run release.yml -f tag=vX.Y.Z` once the Linux build is confirmed (see AGENTS.md "Releasing").
- Unverified end to end: the Epic launcher deep link on Linux, sale themes during a live sale.
