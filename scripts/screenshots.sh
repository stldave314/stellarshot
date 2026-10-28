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
# One screenshot (folder-tree) needs buttons pressed first, and a click sent
# to an Xwayland window never arrives. That one runs on a display of its own
# from Xvfb instead, where xdotool's clicks do land, the window is always the
# same size, and the real pointer is left alone.
#
# The demo profile's password is put in the keyring for the duration of the
# run, so the profile page opens unlocked, and is removed again on exit. The
# later screenshots (search, search-jump, applet) need no keyring at all:
# the demo profile gets its password from a command instead, and they run
# on Xvfb, so nothing of theirs appears on the real screen.
#
# Needs: xdotool, ImageMagick (import), a running desktop session with a
# Secret Service (the keyring) for profile, restore and profile-light, and
# Xvfb for folder-tree, search, search-jump and applet.
#
# Usage: scripts/screenshots.sh [name...]
#   With names (empty, wizard, folder-tree, profile, restore, profile-light,
#   search, search-jump, applet), only those screenshots are written; the
#   others are left as they are.
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
XVFB_PID=""
cleanup() {
    if [[ -n "$APP_PID" ]]; then
        kill "$APP_PID" 2>/dev/null || true
    fi
    if [[ -n "$XVFB_PID" ]]; then
        kill "$XVFB_PID" 2>/dev/null || true
    fi
    if [[ -x target/debug/examples/demo_keyring ]]; then
        target/debug/examples/demo_keyring forget "$PROFILE_ID" || true
    fi
    rm -rf "$DEMO"
}
trap cleanup EXIT

cargo build --quiet --bin stellarshot --example demo_repository --example demo_keyring \
    --example applet_preview

# A small home folder with the kinds of things people back up and leave out.
HOME_DIR="$DEMO/home/alex"
mkdir -p "$HOME_DIR"/{Documents/Taxes,Pictures/2026,Music,.cache/thumbnails,Downloads,Projects/site/node_modules}
head -c 3000000 /dev/urandom > "$HOME_DIR/Pictures/2026/trip.jpg"
head -c 1800000 /dev/urandom > "$HOME_DIR/Pictures/2026/beach.jpg"
head -c 4200000 /dev/urandom > "$HOME_DIR/Music/album.flac"
head -c 90000 /dev/urandom > "$HOME_DIR/Documents/Taxes/2025.pdf"
head -c 24000 /dev/urandom > "$HOME_DIR/Documents/notes.odt"
# Never changes, unlike notes.odt: a search for "notes" shows one of each.
printf 'Agenda\n- Budget\n- Holidays\n' > "$HOME_DIR/Documents/meeting notes.txt"
touch -d '6 days ago' "$HOME_DIR/Documents/meeting notes.txt"
head -c 900000 /dev/urandom > "$HOME_DIR/.cache/thumbnails/cache.bin"
head -c 2500000 /dev/urandom > "$HOME_DIR/Downloads/installer.iso"
head -c 700000 /dev/urandom > "$HOME_DIR/Projects/site/node_modules/lib.js"
echo "hello" > "$HOME_DIR/Projects/site/index.html"

# A demo repository with real snapshots. Made outside the demo home, so the
# snapshots do not contain it, and moved into it as `~/Backups` (Déjà Dup's
# own default is `~/deja-dup`) only after the wizard's screenshots, so their
# size estimate does not count it: the destination then reads as a path a
# person would choose, not a temporary folder's.
STAGED_REPO="$DEMO/staged-repo"
REPO="$HOME_DIR/Backups"
DEMO_PASSWORD="$PASSWORD" target/debug/examples/demo_repository "$STAGED_REPO" "$HOME_DIR" 4

CONFIG="$DEMO/config"
mkdir -p "$DEMO/runtime"
chmod 700 "$DEMO/runtime"

# The demo backup is scheduled, and Stellarshot installs timers for
# scheduled backups when it starts. A stand-in systemctl keeps the demo from
# touching the real user session's systemd.
mkdir -p "$DEMO/bin"
printf '#!/bin/sh\nexit 0\n' > "$DEMO/bin/systemctl"
chmod +x "$DEMO/bin/systemctl"

wanted() {
    # Whether screenshot $1 was asked for: all of them are when none is named.
    [[ ${#ONLY[@]} -eq 0 || " ${ONLY[*]} " =~ " $1 " ]]
}

run_app() {
    # $1: screenshot name; the rest: arguments for Stellarshot.
    local name="$1"
    shift
    wanted "$name" || return 0
    # COSMIC_SINGLE_INSTANCE=false: without it, run_single_instance() reaches
    # past every bit of isolation above through the session D-Bus (keyed only
    # by APP_ID, not by XDG_CONFIG_HOME) and hands this launch to a real,
    # already-running Stellarshot instead of starting the demo one — the
    # window this script then waits for never appears, since it belongs to a
    # process that already exited.
    env -u WAYLAND_DISPLAY HOME="$HOME_DIR" XDG_CONFIG_HOME="$CONFIG" \
        XDG_STATE_HOME="$DEMO/state" XDG_RUNTIME_DIR="$DEMO/runtime" \
        PATH="$DEMO/bin:$PATH" COSMIC_SINGLE_INSTANCE=false \
        "$BIN" "$@" >/dev/null 2>&1 &
    APP_PID=$!
    local window
    window=$(timeout 15 xdotool search --sync --onlyvisible --pid "$APP_PID" | head -1)
    if [[ -z "$window" ]]; then
        echo "FAIL: $name's window never appeared" >&2
        kill "$APP_PID" 2>/dev/null || true
        wait "$APP_PID" 2>/dev/null || true
        APP_PID=""
        exit 1
    fi
    # Let the page settle: the keyring lookup, the snapshot list, the size
    # estimate.
    sleep "${SETTLE:-6}"
    # A screenshot that needs the page driven first has a drive_<name>
    # function, with dashes in the name as underscores.
    local drive="drive_${name//-/_}"
    if declare -F "$drive" >/dev/null; then
        "$drive" "$window"
    fi
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

# 3. The wizard's folder tree, opened with "Browse…": what is included, what
#    is excluded, and how large each folder is. Projects is opened down to
#    node_modules, which is then left out, so all three marks are on show.
#
# Every position below is in pixels from the window's top left corner, and
# holds only at this scale and window size: the scale is the one the other
# screenshots were last taken at, so this one matches them. If the wizard's
# layout changes, measure again from the image this writes.
FOLDER_TREE_SCALE=1.75
FOLDER_TREE_WINDOW="1400 1700"
BROWSE_BUTTON="1097 728"
PROJECTS_ARROW="175 1264"
SITE_ARROW="210 1320"
NODE_MODULES_CHECKBOX="1078 1376"

click_at() {
    # $1: window; $2 $3: where. The pause lets the row under it be listed.
    xdotool mousemove --window "$1" "$2" "$3"
    sleep 0.5
    xdotool click 1
    sleep 2
}

drive_folder_tree() {
    local window="$1"
    # shellcheck disable=SC2086 # two numbers, split on purpose
    xdotool windowsize --sync "$window" $FOLDER_TREE_WINDOW
    sleep 1
    xdotool windowfocus "$window"
    local spot
    for spot in "$BROWSE_BUTTON" "$PROJECTS_ARROW" "$SITE_ARROW" "$NODE_MODULES_CHECKBOX"; do
        # shellcheck disable=SC2086
        click_at "$window" $spot
    done
    # Off the tree again, so no row is drawn hovered.
    xdotool mousemove --window "$window" 10 400
    sleep 1
}

start_xvfb() {
    # A display of its own, where clicks land and the real screen is left
    # alone. Xvfb picks a free display number itself and writes it here.
    command -v Xvfb >/dev/null 2>&1 || {
        echo "FAIL: Xvfb is required for this screenshot" >&2
        exit 1
    }
    rm -f "$DEMO/display"
    Xvfb -displayfd 1 -screen 0 2880x1800x24 -nolisten tcp > "$DEMO/display" 2>/dev/null &
    XVFB_PID=$!
    for _ in $(seq 50); do
        [[ -s "$DEMO/display" ]] && break
        sleep 0.1
    done
    [[ -s "$DEMO/display" ]] || {
        echo "FAIL: Xvfb did not start" >&2
        exit 1
    }
}

stop_xvfb() {
    kill "$XVFB_PID"
    wait "$XVFB_PID" 2>/dev/null || true
    XVFB_PID=""
}

if wanted folder-tree; then
    start_xvfb
    DISPLAY=":$(cat "$DEMO/display")" WINIT_X11_SCALE_FACTOR="$FOLDER_TREE_SCALE" \
        run_app folder-tree --new-backup
    stop_xvfb
fi

# 4. A backup profile with snapshots, unlocked from the keyring.
mv "$STAGED_REPO" "$REPO"
SETTINGS="$CONFIG/cosmic/$APP_ID/v2"
mkdir -p "$SETTINGS"
cat > "$SETTINGS/profiles" <<RON
[
    (
        id: "$PROFILE_ID",
        name: "Home",
        destination: Local(path: "$REPO"),
        sources: ["$HOME_DIR"],
        excludes: ["$HOME_DIR/.cache", "$HOME_DIR/Downloads", "$REPO"],
        exclude_patterns: ["node_modules"],
        one_file_system: true,
        // Manual, not Daily: a debug binary under target/debug isn't a
        // trustworthy location for a scheduled systemd unit, and opening a
        // profile that thinks it should be scheduled fails loudly with an
        // error dialog that covers the whole window.
        schedule: Manual,
        retention: Smart,
        last_success: None,
    ),
]
RON
if wanted profile || wanted restore || wanted profile-light; then
    DEMO_PASSWORD="$PASSWORD" target/debug/examples/demo_keyring store "$PROFILE_ID"
fi
run_app profile

# 5. Getting files back: the restore page, straight from the desktop
#    entry's "Restore Files" action.
# Loading the tree index takes a while in an unoptimized debug build.
SETTLE=25 run_app restore --restore

# 6. The profile page in the light theme, for the second store screenshot.
echo "Light" > "$SETTINGS/app_theme"
run_app profile-light
rm -f "$SETTINGS/app_theme"

# From here on the password comes from a command, not the keyring.
sed -i "s|last_success: None,|last_success: None,\n        password_command: \"printf $PASSWORD\",|" \
    "$SETTINGS/profiles"

# 7. Searching every snapshot for "notes": notes.odt changed in each one,
#    meeting notes.txt in none. Then 8, its second-newest version shown on
#    the Browse tab, selected and marked. Positions as for folder-tree.
SEARCH_SCALE=1.75
SEARCH_WINDOW="1500 1500"
SEARCH_TAB="${SEARCH_TAB:-1273 259}"
SEARCH_FIELD="${SEARCH_FIELD:-750 344}"
SHOW_SECOND_VERSION="${SHOW_SECOND_VERSION:-1385 691}"

drive_search() {
    local window="$1"
    # shellcheck disable=SC2086 # two numbers, split on purpose
    xdotool windowsize --sync "$window" $SEARCH_WINDOW
    sleep 1
    xdotool windowfocus "$window"
    [[ -n "${MEASURE:-}" ]] && import -window "$window" "$MEASURE/measure-1.png"
    # shellcheck disable=SC2086
    click_at "$window" $SEARCH_TAB
    # shellcheck disable=SC2086
    click_at "$window" $SEARCH_FIELD
    xdotool type --delay 50 "notes"
    xdotool key Return
    sleep 8
    xdotool mousemove --window "$window" 10 1400
    sleep 1
}

drive_search_jump() {
    local window="$1"
    drive_search "$window"
    [[ -n "${MEASURE:-}" ]] && import -window "$window" "$MEASURE/measure-2.png"
    # shellcheck disable=SC2086
    click_at "$window" $SHOW_SECOND_VERSION
    sleep 4
    # The jump leaves the pointer over a Download button, showing its
    # tooltip, and a bare move away is not enough to clear it: a click on
    # empty space below the listing is.
    click_at "$window" 750 1100
}

if wanted search || wanted search-jump; then
    start_xvfb
    for name in search search-jump; do
        DISPLAY=":$(cat "$DEMO/display")" WINIT_X11_SCALE_FACTOR="$SEARCH_SCALE" \
            SETTLE=25 run_app "$name" --restore
    done
    stop_xvfb
fi

# 9. The panel applet's popups, in a plain window (a panel's own popups
#    cannot be captured): the demo backup plus a second one that has fallen
#    behind, each with what it last did.
if wanted applet; then
    STATE="$DEMO/state/cosmic/$APP_ID/v2"
    mkdir -p "$STATE"
    NOW=$(date +%s)
    cat >> "$SETTINGS/profiles.applet" <<RON
    (
        id: "photos-demo",
        name: "Photos",
        destination: Local(path: "$DEMO/photos-drive"),
        sources: ["$HOME_DIR/Pictures"],
        schedule: Daily,
        retention: Smart,
        last_success: Some($((NOW - 4 * 86400))),
    ),
]
RON
    sed -i '$ d' "$SETTINGS/profiles"
    cat "$SETTINGS/profiles.applet" >> "$SETTINGS/profiles"
    rm "$SETTINGS/profiles.applet"
    cat > "$STATE/run-$PROFILE_ID" <<RON
(
    last_success: Some($((NOW - 2 * 3600))),
    last_check: Some($((NOW - 3600))),
    last_backup: Some((
        time: $((NOW - 2 * 3600)),
        files_new: 2,
        files_changed: 1,
        data_added: 1843200,
        total_bytes: 9254912,
    )),
)
RON
    cat > "$STATE/event-log-$PROFILE_ID" <<RON
([
    (time: $((NOW - 2 * 3600)), kind: BackedUp, source: Desktop),
    (time: $((NOW - 3600)), kind: Checked(damaged: false), source: Desktop),
])
RON
    cat > "$STATE/run-photos-demo" <<RON
(
    last_success: Some($((NOW - 4 * 86400))),
    last_backup: Some((
        time: $((NOW - 4 * 86400)),
        files_new: 41,
        files_changed: 0,
        data_added: 196083712,
        total_bytes: 4831838208,
    )),
)
RON
    cat > "$STATE/event-log-photos-demo" <<RON
([
    (time: $((NOW - 4 * 86400)), kind: BackedUp, source: Desktop),
    (time: $((NOW - 86400)), kind: Skipped(kind: r#destination-unavailable), source: Desktop),
])
RON
    start_xvfb
    BIN=target/debug/examples/applet_preview DISPLAY=":$(cat "$DEMO/display")" \
        WINIT_X11_SCALE_FACTOR="$SEARCH_SCALE" SETTLE=3 run_app applet
    stop_xvfb
fi
