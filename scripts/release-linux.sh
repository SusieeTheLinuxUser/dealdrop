#!/usr/bin/env bash
# Build the Linux AppImage with embedded update info (so Gear Lever / AppImageUpdate can update it),
# make its .zsync file and publish both to the GitHub release for the current version.
# Needs: cargo-tauri, gh, zsyncmake. Run from anywhere; version comes from src-tauri/tauri.conf.json.
set -euo pipefail
cd "$(dirname "$0")/../src-tauri"
ver=$(python3 -c 'import json;print(json.load(open("tauri.conf.json"))["version"])')
tag="v$ver"
export LDAI_UPDATE_INFORMATION='gh-releases-zsync|SusieeTheLinuxUser|dealdrop|latest|DealDrop_*_amd64.AppImage.zsync'
NO_STRIP=true APPIMAGE_EXTRACT_AND_RUN=1 cargo tauri build --bundles appimage
cd target/release/bundle/appimage
f="DealDrop_${ver}_amd64.AppImage"
zsyncmake -u "$f" -o "$f.zsync" "$f"
if gh release view "$tag" --repo SusieeTheLinuxUser/dealdrop >/dev/null 2>&1; then
  gh release upload "$tag" "$f" "$f.zsync" --repo SusieeTheLinuxUser/dealdrop --clobber
else
  gh release create "$tag" "$f" "$f.zsync" --repo SusieeTheLinuxUser/dealdrop --target main \
    --title "DealDrop $tag (Linux)" --notes "${RELEASE_NOTES:-Linux AppImage first for testing; Windows .msi and macOS .dmg follow once it is confirmed working.}"
fi
