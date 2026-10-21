#!/usr/bin/env bash
# Build the Linux Flatpak bundle (DealDrop_<version>_x86_64.flatpak) and publish it to the GitHub
# release for the current version. Installed copies find it there and update themselves (update.rs).
# Needs: flatpak, flatpak-builder (or the org.flatpak.Builder Flatpak), gh. The GNOME SDK and Rust
# extension the manifest names are installed per-user from Flathub. Version comes from src-tauri/tauri.conf.json.
set -euo pipefail
cd "$(dirname "$0")/.."
ver=$(python3 -c 'import json;print(json.load(open("src-tauri/tauri.conf.json"))["version"])')
tag="v$ver"
id=com.dealdrop.desktop
flathub=https://dl.flathub.org/repo/flathub.flatpakrepo
work=src-tauri/target/flatpak
f="DealDrop_${ver}_$(uname -m).flatpak"

git submodule update --init flatpak/shared-modules
builder=(flatpak-builder)
command -v flatpak-builder >/dev/null || builder=(flatpak run --no-documents-portal --filesystem="$PWD" org.flatpak.Builder)
flatpak remote-add --user --if-not-exists flathub "$flathub"
# --disable-rofiles-fuse: works without FUSE too
"${builder[@]}" --user --install-deps-from=flathub --disable-rofiles-fuse --force-clean \
  --repo="$work/repo" --state-dir="$work/state" "$work/build" flatpak/$id.yml
# --runtime-repo: `flatpak install` fetches the GNOME runtime from Flathub when it's missing
flatpak build-bundle --runtime-repo="$flathub" "$work/repo" "$work/$f" $id
ls -lh "$work/$f"
[ "${NO_UPLOAD:-}" ] && exit 0
if gh release view "$tag" --repo SusieeTheLinuxUser/dealdrop >/dev/null 2>&1; then
  gh release upload "$tag" "$work/$f" --repo SusieeTheLinuxUser/dealdrop --clobber
else
  gh release create "$tag" "$work/$f" --repo SusieeTheLinuxUser/dealdrop --target main \
    --title "DealDrop $tag" --notes "${RELEASE_NOTES:-Linux Flatpak first for testing; Windows .msi and macOS .dmg follow once it is confirmed working.}"
fi
