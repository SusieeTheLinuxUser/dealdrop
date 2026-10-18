From the repository root, run `python3 -m http.server -d .`.
Open `http://localhost:8000/tools/preview/` in a browser.
Add `?onboarded=1` to skip the welcome tour; `?sale=winter` previews the winter sale theme.
Use `?errors=1`, `?empty=1`, or `?accounts=1` to preview those states; `?stress=1` adds long unbroken names, a long account name, and a four-digit price.
For a 570×420 view, size the browser pane; `?w=570&h=420` only documents that target size.
From the repo root, run the dependency-free helper checks with `node tools/preview/selfcheck.mjs`.
