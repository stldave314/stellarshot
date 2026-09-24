#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Regenerate the README and AppStream screenshots from the real application.
#
# Everything the app sees is a throwaway demo: its own settings directory, a
# small demo home folder, and a demo repository with real snapshots made by
# the real engine, so no personal paths or sizes end up in an image. Only
# Stellarshot's own window is captured (ImageMagick `import -window`), never
# the screen, which is why the app runs under Xwayland here: window capture
# needs an X11 window ID.
#
# The demo profile's password is put in the keyring for the duration of the
# run, so the profile page opens unlocked, and is removed again on exit.
#
# Needs: xdotool, ImageMagick (import), a running desktop session with a
# Secret Service (the keyring).
#
# Usage: scripts/screenshots.sh [name...]
#   With names (empty, wizard, profile, restore, profile-light), only those
#   screenshots are written; the others are left as they are.
set -euo pipefail

cd "$(dirname "$0")/.."

for tool in xdotool import; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "FAIL: $tool is required" >&2
        exit 1
    }
done
[[ -n "${DISPLAY:-}" ]] || {
    echo "FAIL: no X display (Xwayland) to run the app on" >&2
    exit 1
}

APP_ID="io.github.stldave314.Stellarshot"
PROFILE_ID="screenshot-demo"
PASSWORD="demo-password"
OUT="docs/screenshots"
BIN="target/debug/stellarshot"
ONLY=("$@")

DEMO=$(mktemp -d /tmp/stellarshot-demo.XXXXXX)
APP_PID=""
cleanup() {
    if [[ -n "$APP_PID" ]]; then
        kill "$APP_PID" 2>/dev/null || true
    fi
    if [[ -x target/debug/examples/demo_keyring ]]; then
        target/debug/examples/demo_keyring forget "$PROFILE_ID" || true
    fi
    rm -rf "$DEMO"
}
trap cleanup EXIT

cargo build --quiet --bin stellarshot --example demo_repository --example demo_keyring

# A small home folder with the kinds of things people back up and leave out.
HOME_DIR="$DEMO/home/alex"
mkdir -p "$HOME_DIR"/{Documents/Taxes,Pictures/2026,Music,.cache/thumbnails,Downloads,Projects/site/node_modules}
head -c 3000000 /dev/urandom > "$HOME_DIR/Pictures/2026/trip.jpg"
head -c 1800000 /dev/urandom > "$HOME_DIR/Pictures/2026/beach.jpg"
head -c 4200000 /dev/urandom > "$HOME_DIR/Music/album.flac"
head -c 90000 /dev/urandom > "$HOME_DIR/Documents/Taxes/2025.pdf"
head -c 24000 /dev/urandom > "$HOME_DIR/Documents/notes.odt"
head -c 900000 /dev/urandom > "$HOME_DIR/.cache/thumbnails/cache.bin"
head -c 2500000 /dev/urandom > "$HOME_DIR/Downloads/installer.iso"
head -c 700000 /dev/urandom > "$HOME_DIR/Projects/site/node_modules/lib.js"
echo "hello" > "$HOME_DIR/Projects/site/index.html"

# A demo repository with real snapshots.
REPO="$DEMO/backups/alex-home"
mkdir -p "$(dirname "$REPO")"
DEMO_PASSWORD="$PASSWORD" target/debug/examples/demo_repository "$REPO" "$HOME_DIR" 4

CONFIG="$DEMO/config"
mkdir -p "$DEMO/runtime"
chmod 700 "$DEMO/runtime"

# The demo backup is scheduled, and Stellarshot installs timers for
# scheduled backups when it starts. A stand-in systemctl keeps the demo from
# touching the real user session's systemd.
mkdir -p "$DEMO/bin"
printf '#!/bin/sh\nexit 0\n' > "$DEMO/bin/systemctl"
chmod +x "$DEMO/bin/systemctl"

run_app() {
    # $1: screenshot name; the rest: arguments for Stellarshot.
    local name="$1"
    shift
    if [[ ${#ONLY[@]} -gt 0 && ! " ${ONLY[*]} " =~ " $name " ]]; then
        return
    fi
    env -u WAYLAND_DISPLAY HOME="$HOME_DIR" XDG_CONFIG_HOME="$CONFIG" \
        XDG_STATE_HOME="$DEMO/state" XDG_RUNTIME_DIR="$DEMO/runtime" \
        PATH="$DEMO/bin:$PATH" "$BIN" "$@" >/dev/null 2>&1 &
    APP_PID=$!
    local window
    window=$(xdotool search --sync --onlyvisible --pid "$APP_PID" | head -1)
    # Let the page settle: the keyring lookup, the snapshot list, the size
    # estimate.
    sleep "${SETTLE:-6}"
    import -window "$window" "$OUT/$name.png"
    kill "$APP_PID"
    wait "$APP_PID" 2>/dev/null || true
    APP_PID=""
    echo "wrote $OUT/$name.png"
}

# 1. First launch: nothing set up yet.
run_app empty

# 2. The setup wizard, sizing the demo home folder.
run_app wizard --new-backup

# 3. A backup profile with snapshots, unlocked from the keyring.
SETTINGS="$CONFIG/cosmic/$APP_ID/v2"
mkdir -p "$SETTINGS"
cat > "$SETTINGS/profiles" <<RON
[
    (
        id: "$PROFILE_ID",
        name: "Home",
        destination: Local(path: "$REPO"),
        sources: ["$HOME_DIR"],
        excludes: ["$HOME_DIR/.cache", "$HOME_DIR/Downloads"],
        exclude_patterns: ["node_modules"],
        one_file_system: true,
        schedule: Daily,
        retention: Smart,
        last_success: None,
    ),
]
RON
DEMO_PASSWORD="$PASSWORD" target/debug/examples/demo_keyring store "$PROFILE_ID"
run_app profile

# 4. Getting files back: the restore page, straight from the desktop
#    entry's "Restore Files" action.
# Loading the tree index takes a while in an unoptimized debug build.
SETTLE=25 run_app restore --restore

# 5. The profile page in the light theme, for the second store screenshot.
echo "Light" > "$SETTINGS/app_theme"
run_app profile-light
