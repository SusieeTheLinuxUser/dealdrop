# Contributing to DealDrop

Thanks for wanting to help! DealDrop is free and open source — contributions of all kinds are welcome.

## Getting started

```bash
git clone https://github.com/your-username/dealdrop
cd dealdrop
npm install
npm start
```

That's it. No build step, no bundler, no config needed.

## Project layout

```
main.js          — Electron main process
                   All API polling, tray, notifications, and Steam OpenID live here.
                   If you're adding a new data source, this is where it goes.

preload.js       — IPC bridge
                   Exposes a controlled window.api surface to the renderer.
                   Any new IPC handler in main.js needs a corresponding entry here.

renderer/
  index.html     — App shell and page markup
  style.css      — All styles (CSS variables at the top for theming)
  app.js         — All UI rendering logic, navigation, event wiring
```

## Adding a new data source

1. Write a `fetchXxx()` async function in `main.js` (see `fetchEpicFreeGames` as a pattern)
2. Call it inside the `poll()` function and include the result in the `data-update` push
3. Also include it in the `fetch-data` IPC handler so the renderer can request it on load
4. Render it in `renderer/app.js`

## Golden rules

- **No secrets in the code.** API keys are always user-supplied at runtime. See [SECURITY.md](SECURITY.md).
- **No telemetry.** DealDrop doesn't phone home. Don't add any.
- **All API calls go in `main.js`.** The renderer never calls external APIs directly.
- **Keep it dependency-light.** Currently only `electron-store` as a runtime dep. Think carefully before adding more.
- **AppImage first.** The primary distribution target is Linux AppImage. Test builds with `npm run dist` before opening a PR.

## Building the AppImage

```bash
npm run dist
# dist/DealDrop-x.x.x.AppImage
```

For all platforms:
```bash
npm run dist:all
```

## Opening a PR

- Keep PRs focused — one feature or fix per PR
- No need for a formal issue first for small things, just open the PR
- Large features: open an issue to discuss first so work doesn't get duplicated

## Code style

No linter configured (yet). Just match the surrounding style — 2-space indents, single quotes, `const`/`let` only.
