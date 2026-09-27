#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# `scripts/verify-release-build.sh` proves the *mechanism* works: built with
# `--features release-build`, `target/debug`'s (sic — its own two-way
# comparison) binaries lose the debug log path, built without it they keep
# it. That is not the same as proving the *shipped* artifacts — the .deb,
# the .rpm, the portable tarball, whatever `install.sh package` actually
# wrote to dist/ — were built that way. A packaging target that forgot to
# pass the feature (exactly what CONTRIBUTING already warns `cargo deb`
# needs it threaded through explicitly for) would pass the other script
# without ever being run.
#
# This extracts every package in dist/ and checks every binary inside for
# the debug log path — after `./install.sh package`, not instead of it.
#
# Usage: scripts/verify-packaged-binaries-strip-logging.sh
set -euo pipefail
trap 'echo "FAIL: \"$BASH_COMMAND\" failed at line $LINENO" >&2' ERR

cd "$(dirname "$0")/.."

LOG_PATH=$(grep -oP 'pub const PATH: &str = "\K[^"]+' src/debug.rs)
if [[ -z "$LOG_PATH" ]]; then
    echo "FAIL: could not find the debug log PATH constant in src/debug.rs" >&2
    exit 1
fi
echo "Debug log path under test: $LOG_PATH"

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

fail=0
checked=0

check_binaries_in() {
    local label="$1"
    local dir="$2"
    local found_any=0
    while IFS= read -r -d '' bin; do
        # Only look at real ELF binaries: a tarball or package also carries
        # text files (README, .desktop, metainfo) that could coincidentally
        # contain the path in a comment or example without meaning anything.
        [[ "$(file --brief --mime-type "$bin")" == "application/x-executable" \
            || "$(file --brief --mime-type "$bin")" == "application/x-pie-executable" \
            || "$(file --brief --mime-type "$bin")" == "application/x-sharedlib" ]] || continue
        found_any=1
        checked=$((checked + 1))
        if strings "$bin" | grep -qF "$LOG_PATH"; then
            echo "FAIL ($label): $bin still contains the debug log path"
            fail=1
        else
            echo "PASS ($label): $bin does not contain the debug log path"
        fi
    done < <(find "$dir" -type f -print0)
    if [[ "$found_any" -eq 0 ]]; then
        echo "FAIL ($label): no ELF binaries found to check in $dir — the extraction may be broken" >&2
        fail=1
    fi
}

shopt -s nullglob
debs=(dist/*.deb)
rpms=(dist/*.rpm)
tarballs=(dist/*.tar.gz)
shopt -u nullglob

if [[ ${#debs[@]} -eq 0 && ${#rpms[@]} -eq 0 && ${#tarballs[@]} -eq 0 ]]; then
    echo "FAIL: no packages found in dist/ — run ./install.sh package first" >&2
    exit 1
fi

for deb in "${debs[@]}"; do
    out="$workdir/deb"
    mkdir -p "$out"
    dpkg-deb -x "$deb" "$out"
    check_binaries_in "$(basename "$deb")" "$out"
done

for rpm in "${rpms[@]}"; do
    out="$workdir/rpm"
    mkdir -p "$out"
    # A real intermediate file rather than a pipe: piping rpm2cpio straight
    # into cpio left a failure on either side unreported, past both
    # pipefail and the ERR trap above, down to a bare exit code with no
    # text explaining which of the two — or why — actually failed.
    payload="$workdir/$(basename "$rpm").cpio"
    # rpm2cpio's own exit code isn't reliable proof of anything here: on at
    # least one real build of it, confirmed directly, it exits 1 even after
    # writing a complete, valid, TRAILER!!!-terminated cpio stream — extracting
    # that same output with cpio afterward works perfectly. So a nonzero exit
    # is noted but not fatal by itself; cpio's own exit code below, and
    # check_binaries_in's "found nothing" check, are what actually catch a
    # genuinely broken extraction.
    if ! rpm2cpio "$rpm" >"$payload"; then
        echo "NOTE ($(basename "$rpm")): rpm2cpio exited nonzero; checking the payload it wrote anyway"
    fi
    (cd "$out" && cpio -idm --quiet <"$payload") || {
        status=$?
        echo "FAIL ($(basename "$rpm")): cpio exited $status unpacking the payload" >&2
        fail=1
        continue
    }
    check_binaries_in "$(basename "$rpm")" "$out"
done

for tarball in "${tarballs[@]}"; do
    out="$workdir/tarball"
    mkdir -p "$out"
    tar -xzf "$tarball" -C "$out"
    check_binaries_in "$(basename "$tarball")" "$out"
done

echo
echo "Checked $checked binaries across $(( ${#debs[@]} + ${#rpms[@]} + ${#tarballs[@]} )) packages."
if [[ $fail -ne 0 ]]; then
    echo "RESULT: FAILED"
    exit 1
fi
echo "RESULT: every shipped binary strips developer logging"
