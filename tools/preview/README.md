From the repository root, run `python3 -m http.server -d .`.
Open `http://localhost:8000/tools/preview/` in a browser.
Add `?onboarded=1` to skip the welcome tour; `?sale=winter` (or `lunar`, `spring`, `summer`, `halloween`, `autumn`) previews that live sale's theme, its "· live" chip in Settings → Appearance and its effect; an unknown key is a live sale with no theme. Reloading without it is the sale ending.
Use `?errors=1`, `?empty=1`, or `?accounts=1` to preview those states; `?accounts=steam` or `?accounts=steam,gog` previews partially signed-in wishlists. `?stress=1` adds long unbroken names, a long account name, and a four-digit price.
Size the browser pane to 570×420, 1280×800, or 1600×1000 for layout checks; `?w=570&h=420` only documents the compact target.
From the repo root, run the dependency-free helper checks with `node tools/preview/selfcheck.mjs`.
