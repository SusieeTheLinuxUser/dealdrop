#!/usr/bin/env bash
# The signed Flatpak repo end to end: publish, add the remote, install, start, update, and refuse tampering.
# Needs a build first (NO_UPLOAD=1 scripts/release-linux.sh). Everything happens in a temp folder on 127.0.0.1 with
# throwaway keys and a throwaway FLATPAK_USER_DIR: the installed DealDrop, your remotes and the real key aren't touched.
# E2E_GPG_HOME=<dir> signs the good repo with the key in <dir> instead. SKIP_APP=1 skips starting the app.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
id=com.dealdrop.desktop
ref=app/$id/$(flatpak --default-arch)/master
port=${PORT:-18765}
url=http://127.0.0.1:$port/
tmp=$(mktemp -d "${TMPDIR:-/tmp}/dealdrop-repo-e2e.XXXXXX")
export FLATPAK_USER_DIR=$tmp/user
trap 'kill "${server:-}" 2>/dev/null || true; for h in "$tmp"/gpg-*; do gpgconf --homedir "$h" --kill gpg-agent || true; done; rm -rf "$tmp"' EXIT

fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "ok: $*"; }
newkey() { # dir name
  mkdir -m 700 "$1"
  gpg --homedir "$1" --batch --quiet --passphrase '' --pinentry-mode loopback --quick-generate-key "$2" rsa2048 sign never 2>/dev/null
  gpg --homedir "$1" --batch --quiet --export --output "$1/pub.gpg"
}
publish() { # gpg-home pubkey pages.git
  git init --quiet --bare "$3"
  FLATPAK_WORK=$tmp/work SITE_URL=$url PAGES_REMOTE=file://$3 DEALDROP_GPG_HOME=$1 DEALDROP_GPG_PUBKEY=$2 \
    "$root/scripts/publish-flatpak-repo.sh" >/dev/null
  serve "$3"
}
serve() { # pages.git: only what was committed is served, like GitHub Pages
  # file://, not a plain path: local clones hardlink and fail now and then
  rm -rf "$tmp/www"; git clone --quiet --branch gh-pages "file://$1" "$tmp/www"; rm -rf "$tmp/www/.git"
}
new_build() { # label: a real new commit of the app with one changed file
  rm -rf "$tmp/co"
  ostree --repo="$tmp/work/repo" checkout -U "$ref" "$tmp/co"
  echo "$1" > "$tmp/co/files/share/e2e-marker"
  flatpak build-export --no-update-summary "$tmp/work/repo" "$tmp/co" master >/dev/null
}
commit() { flatpak info --user --show-commit $id; }
marker() { cat "$(flatpak info --user --show-location $id)/files/share/e2e-marker"; }
update() { flatpak update --user -y --noninteractive $id; }
refused() { # what-was-attacked
  local before; before=$(commit)
  # flatpak update only says "Nothing to update" when the summary's signature fails; remote-ls names it
  { update; flatpak remote-ls --user dealdrop; } >"$tmp/out" 2>&1 || true
  [ "$(commit)" = "$before" ] || { cat "$tmp/out"; fail "accepted $1"; }
  grep -qiE 'gpg|signature|signed|corrupted|checksum' "$tmp/out" || { cat "$tmp/out"; fail "$1 failed for some other reason"; }
  pass "refused $1: $(grep -iE 'gpg|signature|signed|corrupted|checksum' "$tmp/out" | tail -1 | cut -c1-200)"
}

mkdir -p "$tmp/work" "$tmp/www"
cp -a "$root/src-tauri/target/flatpak/repo" "$tmp/work/repo"
good=${E2E_GPG_HOME:-$tmp/gpg-good}
pub=$tmp/gpg-good.pub
[ "${E2E_GPG_HOME:-}" ] && cp "$root/flatpak/dealdrop-repo.gpg" "$pub" || { newkey "$good" "E2E Good"; cp "$good/pub.gpg" "$pub"; }
newkey "$tmp/gpg-evil" "E2E Evil"
python3 -m http.server --bind 127.0.0.1 --directory "$tmp/www" "$port" >/dev/null 2>&1 &
server=$!
sleep 1

# 1. Add the remote and install, as a user would
publish "$good" "$pub" "$tmp/pages.git"
[ -s "$tmp/www/repo/summary.sig" ] || fail "summary isn't signed"
flatpak remote-add --user --if-not-exists dealdrop "${url}dealdrop.flatpakrepo"
grep -q "gpg-verify=true" "$FLATPAK_USER_DIR/repo/config" || fail "remote has GPG checks off"
flatpak install --user -y --noninteractive dealdrop $id >/dev/null
[ "$(flatpak info --user --show-origin $id)" = dealdrop ] || fail "wrong origin"
[ "$(flatpak remote-info --user --show-commit dealdrop $id//master)" = "$(commit)" ] || fail "commit differs from the remote's"
pass "installed from the signed remote"
ls "$tmp"/www/repo/refs/heads/appstream2/* >/dev/null || fail "no appstream data for Discover"
flatpak update --user --appstream dealdrop >/dev/null 2>&1
flatpak search --user dealdrop | grep -q $id || fail "app store data doesn't list it"
pass "signed app store data lists it"
if [ -z "${SKIP_APP:-}" ]; then
  DEALDROP_BIN=$root/tools/e2e/flatpak.sh "$root/tools/e2e/run.sh" "$root/tools/e2e/flatpak.mjs" || fail "app doesn't start"
  pass "app starts"
fi

# 2. A new published build reaches `flatpak update`
new_build second
publish "$good" "$pub" "$tmp/pages.git"
first=$(commit); update >/dev/null
[ "$(commit)" != "$first" ] && [ "$(marker)" = second ] || fail "update didn't pick up the new build"
pass "flatpak update picked up the second build"

# 3. Attacks on a third build. The client has only ever seen build two, so nothing here can come from its cache.
find "$tmp/www/repo/objects" -type f | sort > "$tmp/before"
new_build third

publish "$tmp/gpg-evil" "$tmp/gpg-evil/pub.gpg" "$tmp/evil.git"
refused "a repo signed with another key"

rm -f "$tmp"/www/repo/summary.sig "$tmp"/www/repo/summary.idx.sig
find "$tmp/www/repo" \( -name '*.commitmeta' -o -name '*.sig' \) -delete
refused "a repo with no signatures at all"

publish "$good" "$pub" "$tmp/pages.git"
find "$tmp/www/repo/objects" -name '*.filez' | sort | comm -13 "$tmp/before" - > "$tmp/new"
[ -s "$tmp/new" ] || fail "no new file object to tamper with"
f=$(head -1 "$tmp/new"); size=$(stat -c%s "$f")
cmp -s <(tail -c 5 "$f") <(printf XXXXX) && fail "object already ends in XXXXX"
printf XXXXX | dd of="$f" bs=1 seek=$((size - 5)) conv=notrunc 2>/dev/null
refused "a modified object"

# The refusals above weren't just a broken setup: the real third build still installs
serve "$tmp/pages.git"
update >/dev/null
[ "$(marker)" = third ] || fail "the untampered build didn't install"
pass "the untampered third build installs"

# 4. Nothing secret went out
grep -rlaE 'BEGIN PGP PRIVATE' "$tmp/www" && fail "private key in the served files"
# Only the text files: compressed objects can match an email pattern by chance
cat "$tmp"/www/*.flatpakre* "$tmp/www/repo/config" | grep -E '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[a-z]{2,}' && fail "an email in the served files"
case $(git -C "$tmp/pages.git" log -1 --format=%ae gh-pages) in *@users.noreply.github.com) ;; *) fail "gh-pages commit has a personal email" ;; esac
[ "$(gpg --list-packets "$pub" | grep -c '^:secret')" = 0 ] || fail "private key material in the public key file"
pass "served files have no key or personal email"
echo "ALL PASSED"
