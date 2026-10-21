#!/bin/sh
# DEALDROP_BIN for run.sh: the installed Flatpak instead of the release binary.
# run.sh's throwaway HOME hides the real per-user installation, so point flatpak back at it.
export FLATPAK_USER_DIR="${FLATPAK_USER_DIR:-$(getent passwd "$(id -u)" | cut -d: -f6)/.local/share/flatpak}"
exec flatpak run --no-documents-portal com.dealdrop.desktop "$@"
