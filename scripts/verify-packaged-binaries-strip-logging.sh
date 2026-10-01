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
source scripts/lib/packages.sh

LOG_PATH=$(grep -oP 'pub const PATH: &str = "\K[^"]+' src/debug.rs)
if [[ -z "$LOG_PATH" ]]; then
    echo "FAIL: could not find the debug log PATH constant in src/debug.rs" >&2
    exit 1
fi
echo "Debug log path under test: $LOG_PATH"

symbols=$(mktemp)
trap 'rm -f "$symbols"' EXIT

# `strings ... | grep -q` must not be used here: grep exits on the first
# match and closes the pipe, `strings` dies of SIGPIPE, and under
# `set -o pipefail` the pipeline reports failure even though the string was
# found — which inverts the result below into a false PASS. Dump to a file
# and grep that instead (the same fix `verify-release-build.sh` already
# uses, and for the same reason).
check_one() {
    local label="$1" bin="$2"
    strings "$bin" > "$symbols"
    if grep -qF "$LOG_PATH" "$symbols"; then
        echo "FAIL ($label): $bin still contains the debug log path"
        return 1
    fi
    echo "PASS ($label): $bin does not contain the debug log path"
}

check_packaged_binaries "no debug log path" check_one
