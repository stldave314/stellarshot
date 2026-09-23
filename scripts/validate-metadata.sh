#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Validate the desktop entries and AppStream metadata.
#
# Every finding is a hard failure. This is deliberately not a blanket
# `|| true`: a validator that always passes is not a validator.
#
# `appstreamcli validate` on its own is not enough to know the app will appear
# correctly in a software centre. It passes a file with no screenshots, and it
# has no opinion about whether the newest release listed is the version that
# actually ships. Both are checked here, because both are invisible until
# someone looks at the store page and finds it empty or a version behind.
set -euo pipefail

cd "$(dirname "$0")/.."

for tool in desktop-file-validate appstreamcli; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "FAIL: $tool is required (install desktop-file-utils and appstream)" >&2
        exit 1
    }
done

APP_ID="io.github.stldave314.Stellarshot"
fail=0

for file in res/*.desktop; do
    echo "== $file"
    output=$(desktop-file-validate "$file" 2>&1) && status=0 || status=$?

    # Drop hints, which are advisory; anything left is real.
    remaining=$(printf '%s\n' "$output" \
        | grep -v ': hint: ' \
        | grep -v '^$' || true)

    if [[ -n "$remaining" ]]; then
        printf '%s\n' "$remaining"
        echo "FAIL: $file"
        fail=1
    elif [[ $status -ne 0 ]]; then
        echo "OK (hints only)"
    else
        echo "OK"
    fi
done

for file in res/*.metainfo.xml; do
    echo "== $file"
    if appstreamcli validate --no-net --explain "$file"; then
        echo "OK"
    else
        echo "FAIL: $file"
        fail=1
    fi
done

# ---------------------------------------------------------------------------
# Checks appstreamcli does not make
# ---------------------------------------------------------------------------

METAINFO="res/$APP_ID.metainfo.xml"
DESKTOP="res/$APP_ID.desktop"

echo "== $METAINFO: store presentation"

# Fail loudly rather than silently finding nothing to check. An assertion that
# passes when its fixture is missing reports green on a regressed tree.
for file in "$METAINFO" "$DESKTOP" Cargo.toml; do
    [[ -f "$file" ]] || {
        echo "FAIL: $file is missing, so nothing below was actually checked" >&2
        exit 1
    }
done

check() {
    local label="$1" actual="$2" expected="$3"
    if [[ "$actual" == "$expected" ]]; then
        printf '   OK   %s: %s\n' "$label" "$actual"
    else
        printf '   FAIL %s: got %s, expected %s\n' "$label" "${actual:-<none>}" "$expected"
        fail=1
    fi
}

# A component with no screenshots is accepted by appstreamcli and rejected by
# Flathub, and renders in the COSMIC Store as a blank card.
# `<screenshot` alone also matches the enclosing `<screenshots>` element, which
# would make the count one too high and the caption comparison always fail.
shots=$(grep -c '<screenshot[ >]' "$METAINFO" || true)
if [[ "$shots" -gt 0 ]]; then
    printf '   OK   screenshots: %s\n' "$shots"
else
    printf '   FAIL screenshots: none; the store page would have no images\n'
    fail=1
fi

# Every screenshot needs a caption, and there must be exactly one default.
captions=$(grep -c '<caption>' "$METAINFO" || true)
check "captions" "$captions" "$shots"
check "default screenshot" "$(grep -c 'screenshot type="default"' "$METAINFO" || true)" "1"

# The newest release must be the version being shipped. A stale entry here is
# what makes a store page advertise an old version indefinitely.
cargo_version=$(sed -n '0,/^version = /s/^version = "\(.*\)"/\1/p' Cargo.toml)
metainfo_version=$(sed -n 's/.*<release version="\([^"]*\)".*/\1/p' "$METAINFO" | head -1)
check "release version matches Cargo.toml" "$metainfo_version" "$cargo_version"

# The three identities have to agree or the launcher, the icon lookup and the
# store entry come apart.
check "component id" "$(sed -n 's:.*<id>\(.*\)</id>.*:\1:p' "$METAINFO" | head -1)" "$APP_ID"
check "launchable" \
    "$(sed -n 's:.*<launchable type="desktop-id">\(.*\)</launchable>.*:\1:p' "$METAINFO")" \
    "$APP_ID.desktop"
check "desktop Icon=" "$(sed -n 's/^Icon=//p' "$DESKTOP")" "$APP_ID"

# Store front-ends need these; a missing developer name or vcs-browser URL
# leaves the listing without an author or a link to the source.
for tag in "<developer id=" "<name>" "<content_rating" "vcs-browser" "<branding>"; do
    if grep -q -- "$tag" "$METAINFO"; then
        printf '   OK   present: %s\n' "$tag"
    else
        printf '   FAIL missing: %s\n' "$tag"
        fail=1
    fi
done

# Icons named in the desktop entry must actually be installed under the app ID,
# or the store shows a generic placeholder.
for icon in "res/icons/hicolor/scalable/apps/$APP_ID.svg" \
            "res/icons/hicolor/scalable/apps/$APP_ID-symbolic.svg"; do
    if [[ -f "$icon" ]]; then
        printf '   OK   icon: %s\n' "$icon"
    else
        printf '   FAIL icon missing: %s\n' "$icon"
        fail=1
    fi
done

if [[ $fail -ne 0 ]]; then
    echo
    echo "RESULT: FAILED"
    exit 1
fi
echo
echo "RESULT: metadata is valid"
