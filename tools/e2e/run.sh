#!/usr/bin/env bash
# Run a Node check against the real release app in a throwaway, invisible session (see README.md).
# usage: tools/e2e/run.sh <check.mjs> [app args...]     env: DEALDROP_BIN (default: the release build)
set -euo pipefail

if [ -z "${E2E_INNER:-}" ]; then
  [ $# -ge 1 ] || { echo "usage: $0 <check.mjs> [app args...]" >&2; exit 2; }
  root=$(cd "$(dirname "$0")/../.." && pwd)
  export DEALDROP_BIN=$(realpath "${DEALDROP_BIN:-$root/src-tauri/target/release/dealdrop}")
  script=$(realpath "$1"); shift
  tmp=$(mktemp -d /tmp/dealdrop-e2e.XXXXXX)
  # A fresh home for everything (the app's config.json, WebKit storage, GTK, the D-Bus daemon):
  # set before dbus-run-session so the bus and anything it starts never see the real one.
  export E2E_TMP=$tmp HOME=$tmp/home XDG_RUNTIME_DIR=$tmp/run
  export XDG_CONFIG_HOME=$HOME/.config XDG_DATA_HOME=$HOME/.local/share XDG_CACHE_HOME=$HOME/.cache XDG_STATE_HOME=$HOME/.local/state
  mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$XDG_STATE_HOME"
  mkdir -m 700 "$XDG_RUNTIME_DIR"
  # Private session bus with no activatable services: tray, notifications, portals and the
  # single-instance lock stay inside it, and nothing gets started on the app's behalf
  cat >"$tmp/bus.conf" <<EOF
<busconfig><type>session</type><listen>unix:dir=$tmp</listen><auth>EXTERNAL</auth>
<policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy></busconfig>
EOF
  # Own process group, so cleanup kills everything started below (kwin, WebKitWebDriver, the app)
  E2E_INNER=1 setsid dbus-run-session --config-file="$tmp/bus.conf" -- "$0" "$script" "$@" &
  pid=$!
  trap 'kill -TERM -- -$pid 2>/dev/null || true; sleep 1; kill -KILL -- -$pid 2>/dev/null || true; rm -rf "$tmp"' EXIT
  trap 'exit 130' INT TERM
  wait $pid
  exit
fi

unset DISPLAY XAUTHORITY
export WAYLAND_DISPLAY=dealdrop-e2e GDK_BACKEND=wayland NO_AT_BRIDGE=1
export TAURI_WEBVIEW_AUTOMATION=true # tauri-runtime-wry: let WebKit automation drive the first webview

kwin_wayland --virtual --socket "$WAYLAND_DISPLAY" --width 1280 --height 800 >"$E2E_TMP/kwin.log" 2>&1 &
for _ in $(seq 50); do [ -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ] && break; sleep 0.1; done
[ -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ] || { echo "kwin_wayland --virtual didn't start:" >&2; cat "$E2E_TMP/kwin.log" >&2; exit 1; }

export WEBDRIVER_URL=http://127.0.0.1:$((20000 + RANDOM % 20000))
WebKitWebDriver --port="${WEBDRIVER_URL##*:}" >"$E2E_TMP/webdriver.log" 2>&1 &
for _ in $(seq 50); do curl -sf "$WEBDRIVER_URL/status" >/dev/null && break; sleep 0.1; done

script=$1; shift
E2E_APP_ARGS=$(printf '%s\n' "$@") node "$script" || { s=$?; echo "--- webdriver.log" >&2; tail -20 "$E2E_TMP/webdriver.log" >&2; exit $s; }
