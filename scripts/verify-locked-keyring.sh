#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Prove what Stellarshot does with a keyring that is locked, as one is after
# logging in without typing the login password (automatic login, a
# fingerprint), against a real GNOME Keyring and its real unlock prompt.
#
# GNOME Keyring neither gives out nor takes a password while locked, and does
# not offer to unlock by itself: it must be asked. The window asks
# (`keyring::allow_unlock_prompt`); anything unattended must not. Reading
# `src/keyring.rs` is not evidence of either: the window once never asked,
# so a remembered password was not found and a new one "could not be saved",
# with no prompt at all.
#
# Everything here is private to the run: a D-Bus session, a keyring in a
# throwaway home, and an X display from Xvfb for the prompt, so neither the
# real keyring nor the real desktop is touched or shown anything.
#
# Needs: dbus-run-session, gnome-keyring-daemon, gcr-prompter (package gcr),
# Xvfb and xdotool. Fails, rather than skips, without them.
#
# Usage: scripts/verify-locked-keyring.sh
set -euo pipefail

if [[ "${1:-}" != "--inside" ]]; then
    cd "$(dirname "$0")/.."
    for tool in dbus-run-session gnome-keyring-daemon Xvfb xdotool; do
        command -v "$tool" >/dev/null 2>&1 || {
            echo "FAIL: $tool is required" >&2
            exit 1
        }
    done
    [[ -x /usr/libexec/gcr-prompter ]] || {
        echo "FAIL: /usr/libexec/gcr-prompter (package gcr) is required" >&2
        exit 1
    }
    cargo build --quiet --example demo_keyring
    BIN="$PWD/target/debug/examples/demo_keyring"

    ROOT=$(mktemp -d /tmp/stellarshot-keyring.XXXXXX)
    XVFB_PID=""
    cleanup() {
        [[ -n "$XVFB_PID" ]] && kill "$XVFB_PID" 2>/dev/null || true
        rm -rf "$ROOT" 2>/dev/null || true
    }
    trap cleanup EXIT
    mkdir -p "$ROOT"/{home,runtime,config,data,cache}
    chmod 700 "$ROOT/runtime"
    # Xvfb picks a free display number itself and writes it here.
    Xvfb -displayfd 1 -screen 0 1280x800x24 -nolisten tcp > "$ROOT/display" 2>/dev/null &
    XVFB_PID=$!
    for _ in $(seq 50); do
        [[ -s "$ROOT/display" ]] && break
        sleep 0.1
    done
    [[ -s "$ROOT/display" ]] || {
        echo "FAIL: Xvfb did not start" >&2
        exit 1
    }
    # A private session bus alone is not isolation: the services it starts
    # would still use the real runtime folder, settings and display. The
    # prompt is a GTK program, which would also start an accessibility bus
    # and gvfs (a FUSE mount in the runtime folder) unless told not to.
    # The bus daemon's own chatter goes to a log, shown only on failure.
    code=0
    env -u WAYLAND_DISPLAY -u GNOME_KEYRING_CONTROL -u DBUS_SESSION_BUS_ADDRESS -u SSH_AUTH_SOCK \
        DISPLAY=":$(cat "$ROOT/display")" GDK_BACKEND=x11 HOME="$ROOT/home" \
        XDG_RUNTIME_DIR="$ROOT/runtime" XDG_CONFIG_HOME="$ROOT/config" \
        XDG_DATA_HOME="$ROOT/data" XDG_CACHE_HOME="$ROOT/cache" GSETTINGS_BACKEND=memory \
        NO_AT_BRIDGE=1 GTK_A11Y=none GIO_USE_VFS=local GVFS_DISABLE_FUSE=1 \
        dbus-run-session -- "$0" --inside "$BIN" < /dev/null 2> "$ROOT/session.log" || code=$?
    if [[ "$code" -ne 0 ]]; then
        cat "$ROOT/session.log" >&2
    fi
    exit "$code"
fi

# --- Inside the private session ---------------------------------------------
BIN="$2"
KEYRING_PASSWORD="private-keyring-password"
export DEMO_PASSWORD="a-backup-password"
FAILED=0

printf '%s' "$KEYRING_PASSWORD" |
    gnome-keyring-daemon --unlock --components=secrets \
        --control-directory="$XDG_RUNTIME_DIR/keyring" > /dev/null
trap 'pkill -f "control-directory=$XDG_RUNTIME_DIR/keyring" 2>/dev/null || true' EXIT
sleep 2

check() { # description, expected, actual
    if [[ "$2" == "$3" ]]; then
        echo "   OK   $1"
    else
        echo "   FAIL $1: expected '$2', got '$3'"
        FAILED=1
    fi
}

prompt_window() { # the unlock prompt's window, waiting up to $1 seconds
    local id
    for _ in $(seq $(($1 * 2))); do
        id=$(xdotool search --onlyvisible --class gcr-prompter 2>/dev/null | tail -1 || true)
        if [[ -n "$id" ]]; then
            echo "$id"
            return 0
        fi
        sleep 0.5
    done
    return 1
}

answer() { # "type" the keyring's password into the prompt, or "dismiss" it
    local id
    if id=$(prompt_window 20); then
        PROMPTED=yes
        xdotool windowfocus "$id" 2>/dev/null || true
        sleep 0.5
        if [[ "$1" == type ]]; then
            xdotool type --delay 40 "$KEYRING_PASSWORD"
            xdotool key Return
        else
            xdotool key Escape
        fi
    else
        PROMPTED=no
    fi
}

attempt() { # run demo_keyring in the background; its exit status lands in $RESULT_FILE
    RESULT_FILE=$(mktemp)
    (
        code=0
        "$BIN" "$@" > "$RESULT_FILE.out" 2>&1 || code=$?
        echo "$code" > "$RESULT_FILE"
    ) &
    ATTEMPT=$!
}

finish() { # wait for attempt(); sets $CODE and $OUTPUT
    wait "$ATTEMPT"
    CODE=$(cat "$RESULT_FILE")
    OUTPUT=$(tail -1 "$RESULT_FILE.out" 2>/dev/null || true)
}

# The fixture must be real: a keyring that works unlocked and really locks,
# or everything below would pass for the wrong reason.
"$BIN" store remembered
check "fixture: the private keyring starts unlocked" unlocked "$("$BIN" locked)"
check "fixture: a password is remembered while unlocked" remembered "$("$BIN" load remembered)"
"$BIN" lock
check "fixture: the keyring locks" locked "$("$BIN" locked)"

echo "Unattended (a scheduled run): never a prompt"
code=0; "$BIN" store unattended > /dev/null 2>&1 || code=$?
check "saving into a locked keyring fails" 1 "$code"
code=0; "$BIN" load remembered > /dev/null 2>&1 || code=$?
check "reading from a locked keyring fails, rather than saying none is remembered" 1 "$code"
sleep 1
check "no unlock prompt was shown" 0 "$(xdotool search --onlyvisible --class gcr-prompter 2>/dev/null | wc -l)"
check "the keyring is still locked" locked "$("$BIN" locked)"

echo "The window: asks the keyring to unlock"
attempt --may-prompt store from-window; answer type; finish
check "saving shows the unlock prompt" yes "$PROMPTED"
check "saving succeeds once it is answered" 0 "$CODE"
check "the keyring is unlocked afterwards" unlocked "$("$BIN" locked)"
"$BIN" lock
attempt --may-prompt load remembered; answer type; finish
check "looking for a remembered password shows the unlock prompt" yes "$PROMPTED"
check "the remembered password is found" remembered "$OUTPUT"
"$BIN" lock
attempt --may-prompt load from-window; answer type; finish
check "what was saved through the prompt is found again" remembered "$OUTPUT"

echo "The window: a dismissed prompt"
"$BIN" lock
attempt --may-prompt store dismissed; answer dismiss; finish
check "the prompt is shown" yes "$PROMPTED"
check "saving fails, rather than hanging or pretending" 1 "$CODE"
check "the keyring stays locked" locked "$("$BIN" locked)"

if [[ "$FAILED" -ne 0 ]]; then
    echo "RESULT: a locked keyring is NOT handled as it should be"
    exit 1
fi
echo "RESULT: a locked keyring is handled as it should be"
