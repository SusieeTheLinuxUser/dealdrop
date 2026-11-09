#!/usr/bin/env bash
# Signs the Flatpak that release-linux.sh built (src-tauri/target/flatpak/repo) into the public OSTree repo
# and pushes it to the gh-pages branch, which GitHub Pages serves. Users get it through `flatpak update`/Discover.
# release-linux.sh runs this; on its own it publishes the build you already tested. Needs flatpak, ostree, gpg.
# The private key stays on this PC (AGENTS.md "Flatpak repo"). The overrides below exist for tools/e2e/flatpak-repo.sh.
set -euo pipefail
cd "$(dirname "$0")/.."
id=com.dealdrop.desktop
flathub=https://dl.flathub.org/repo/flathub.flatpakrepo
work=${FLATPAK_WORK:-src-tauri/target/flatpak}
url=${SITE_URL:-https://susieethelinuxuser.github.io/dealdrop/}
remote=${PAGES_REMOTE:-$(git remote get-url origin)}
gpg_home=${DEALDROP_GPG_HOME:-$HOME/.local/share/dealdrop-flatpak-gpg}
pubkey=${DEALDROP_GPG_PUBKEY:-flatpak/dealdrop-repo.gpg}
site=$work/site
repo=$site/repo

key=$(gpg --show-keys --with-colons "$pubkey" | awk -F: '/^fpr:/{print $10; exit}')
gpg --homedir "$gpg_home" --list-secret-keys "$key" >/dev/null 2>&1 || {
  echo "Signing key $key isn't in $gpg_home. Restore the backup: see AGENTS.md \"Flatpak repo\"." >&2; exit 1; }
sign=(--gpg-sign="$key" --gpg-homedir="$gpg_home")

# Start from what's live, so earlier versions stay available and clients keep their history
rm -rf "$site"
rc=0; git ls-remote --exit-code --heads "$remote" gh-pages >/dev/null || rc=$?
case $rc in
  0) git clone --quiet --depth 1 --branch gh-pages "$remote" "$site"; rm -rf "$site/.git" ;;
  2) mkdir -p "$site" ;; # first publish
  *) exit "$rc" ;;
esac
[ -d "$repo" ] && mkdir -p "$repo"/refs/{heads,mirrors,remotes} # git doesn't keep empty folders
[ -d "$repo" ] || ostree init --repo="$repo" --mode=archive

flatpak build-commit-from "${sign[@]}" --no-update-summary --src-repo="$work/repo" "$repo" "app/$id/$(flatpak --default-arch)/master"
# ponytail: keeps the last 3 builds (under 20 MB each), well under Pages' 1 GB
flatpak build-update-repo "${sign[@]}" --title=DealDrop --default-branch=master --prune --prune-depth=3 "$repo"
[ -s "$repo/summary.sig" ] # never publish an unsigned repo

key64=$(base64 -w0 "$pubkey")
cat > "$site/dealdrop.flatpakrepo" <<EOF
[Flatpak Repo]
Title=DealDrop
Url=${url}repo/
Homepage=https://github.com/SusieeTheLinuxUser/dealdrop
Comment=Free games, store sales and wishlist deals
DefaultBranch=master
RuntimeRepo=$flathub
GPGKey=$key64
EOF
cat > "$site/$id.flatpakref" <<EOF
[Flatpak Ref]
Title=DealDrop
Name=$id
Branch=master
Url=${url}repo/
IsRuntime=false
SuggestRemoteName=dealdrop
Homepage=https://github.com/SusieeTheLinuxUser/dealdrop
RuntimeRepo=$flathub
GPGKey=$key64
EOF
touch "$site/.nojekyll" # Pages serves the files as they are, no Jekyll build

# One commit, replaced on every publish: the old objects must not pile up in everyone's `git clone`
rm -rf "$repo/.lock" "$repo/tmp"
git -C "$site" init --quiet --initial-branch=gh-pages
git -C "$site" add --all --force .
git -C "$site" -c user.name=SusieeTheFemboy -c user.email=96988330+SusieeTheLinuxUser@users.noreply.github.com \
  commit --quiet --message "Flatpak repo from $(git rev-parse --short HEAD)"
git -C "$site" push --quiet --force "$remote" HEAD:refs/heads/gh-pages
echo "Published to ${url}"
