#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Installs this portable tarball's own already-built usr/ tree.
#
# This is not install.sh (this project's own build script): that one needs
# cargo and the full source tree to build from, neither of which a portable
# binary tarball carries. This only copies files that are already built.
#
#   ./install-tarball.sh              install system-wide (requires root)
#   PREFIX=/usr/local ./install-tarball.sh   install under a different prefix
#   DESTDIR=/tmp/stage ./install-tarball.sh  stage into a root, for packaging
#   ./install-tarball.sh uninstall    remove a system-wide install
set -euo pipefail
cd "$(dirname "$0")"

PREFIX="${PREFIX:-/usr}"
DESTDIR="${DESTDIR:-}"

[[ -d usr ]] || {
    echo "usr/ not found next to this script — run it from inside the extracted tarball" >&2
    exit 1
}

as_root() {
    if [[ $EUID -eq 0 ]]; then
        "$@"
    elif command -v sudo >/dev/null 2>&1; then
        sudo "$@"
    else
        echo "root privileges are required and sudo is not available" >&2
        exit 1
    fi
}

target="$DESTDIR$PREFIX"

# Installs each file in usr/ individually with `install`, never `cp -a`:
# `cp -a` preserves the *extracting user's* ownership (whoever ran `tar xf`,
# following the README's own instructions, which is never root), and when a
# destination directory already exists, GNU `cp -a` re-applies that owner and
# mode to the directory too. Run under root that silently re-owns /usr,
# /usr/bin and every other already-existing directory to the extracting
# user — a local privilege escalation, not merely a cosmetic bug. `install`
# only ever creates or overwrites the one file it is told to, at the mode
# given, and never touches an existing parent directory's ownership or mode.
install_tree() {
    local runner=("$@")
    local f mode
    while IFS= read -r -d '' f; do
        f="${f#./}"
        mode=644
        [[ "$f" == bin/* ]] && mode=755
        "${runner[@]}" install -Dm"$mode" "usr/$f" "$target/$f"
    done < <(cd usr && find . -type f -print0)
}

cmd_uninstall() {
    echo "Removing installed files from $target"
    local f
    while IFS= read -r -d '' f; do
        f="${f#./}"
        as_root rm -f "$target/$f"
    done < <(cd usr && find . -type f -print0)
    echo "Removed."
    echo
    echo "Any per-user systemd unit (a scheduled backup) is left" >&2
    echo "running and installed, since it belongs to your user account, not this" >&2
    echo "prefix. Stopping and disabling a unit does not delete its file, so both" >&2
    echo "steps are needed to remove them yourself:" >&2
    echo "  systemctl --user disable --now 'stellarshot*'" >&2
    echo "  rm -f ~/.config/systemd/user/stellarshot*.service ~/.config/systemd/user/stellarshot*.timer" >&2
}

if [[ "${1:-}" == "uninstall" ]]; then
    cmd_uninstall
    exit 0
fi

if [[ -n "$DESTDIR" ]]; then
    mkdir -p "$target"
    install_tree
else
    echo "Installing into $PREFIX (requires root)"
    install_tree as_root
    as_root update-desktop-database "$target/share/applications" 2>/dev/null || true
    as_root gtk-update-icon-cache -f "$target/share/icons/hicolor" 2>/dev/null || true
fi

echo "Installed."
echo
echo "  Next: launch Stellarshot from the application launcher."
echo
if ! command -v rclone >/dev/null 2>&1; then
    echo "rclone is not installed. Local and USB backups work without it;" >&2
    echo "cloud destinations such as Google Drive will need it." >&2
fi
if ! command -v fusermount3 >/dev/null 2>&1; then
    echo "fuse3 is not installed. Everything works without it except" >&2
    echo "\"Mount as Folder\"." >&2
fi
