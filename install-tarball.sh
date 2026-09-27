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

if [[ -n "$DESTDIR" ]]; then
    mkdir -p "$target"
    cp -a usr/. "$target/"
else
    echo "Installing into $PREFIX (requires root)"
    as_root mkdir -p "$target"
    as_root cp -a usr/. "$target/"
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
