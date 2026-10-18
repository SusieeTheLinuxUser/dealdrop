# Merge notes: project-status-review-1 + project-status-review-2

This is a dry-run only. Preserve both sets of changes; do not take whole files from either side.

## Shared files and hunk choices

- `AGENTS.md`: keep this branch's `tools/preview/` Layout row. Also apply the other tree's font/CSP and `settings.onboarded` Rules additions. They are in separate sections and do not conflict.
- `renderer/index.html`: keep this branch's Settings/About markup and onboarding stylesheet/script hooks. Apply the other tree's three-line removal of Google Fonts preconnect/stylesheet links in `<head>`. These are distinct hunks.
- `renderer/style.css`: put the other tree's three `@font-face` declarations and font comment at the very top, before this branch's reset, variables, and all component rules. Keep all existing CSS, including this branch's `.launcher-pref-grid` rule. Bundled font files must be present at `renderer/fonts/` before the CSS is used.
- `src-tauri/src/stores.rs`: keep this branch's `.without_url()` error handling and apply the other tree's country/currency character validation in `gog_user`; those hunks are separate.

## Files changed only in the other tree

Take its changes to `CONTRIBUTING.md`, `SECURITY.md`, `TODO.md`, `src-tauri/src/main.rs`, `src-tauri/src/sources.rs`, and `src-tauri/tauri.conf.json`, plus its new `CHANGELOG.md`, `TESTING.md`, and `renderer/fonts/` files. None has a same-path change in this branch (except the shared `stores.rs` noted above).

## Files changed only in this branch

Keep this branch's README and renderer onboarding/settings code, `src-tauri/src/launchers.rs`, and `tools/preview/`. The preview's copied HTML already omits font-CDN references and loads the real renderer CSS by relative path, so it will pick up the bundled-font declarations once merged.

## Suggested application order

1. Bring over `renderer/fonts/` and the CSS `@font-face` hunk first, with those declarations at line 1 of the merged stylesheet.
2. Remove the Google Fonts tags from `renderer/index.html`; retain its Settings/About and onboarding body changes.
3. Combine the separate `AGENTS.md` and `stores.rs` hunks as described above; then bring over the other-tree-only files.
4. Review the final combined diff. This note does not apply or resolve any changes.
