#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Proves every binary in every package `./install.sh package` wrote to dist/
# carries `cargo auditable`'s embedded dependency list (an ELF section named
# `.dep-v0`), so `cargo audit bin` can check what a *shipped* binary
# actually contains. `install.sh` builds once and packages that one build
# three ways; this is what catches a packaging path that rebuilt its own
# plain binary instead.
#
# Usage: scripts/verify-packaged-binaries-auditable.sh
set -euo pipefail
trap 'echo "FAIL: \"$BASH_COMMAND\" failed at line $LINENO" >&2' ERR

cd "$(dirname "$0")/.."
source scripts/lib/packages.sh

check_one() {
    local label="$1" bin="$2" sections
    # Captured, then matched with a here-string: no pipe for `grep -q` to
    # SIGPIPE under `pipefail` (see verify-packaged-binaries-strip-logging.sh).
    sections=$(readelf -S --wide "$bin")
    if grep -qF '.dep-v0' <<<"$sections"; then
        echo "PASS ($label): $bin carries its dependency data"
        return 0
    fi
    echo "FAIL ($label): $bin has no .dep-v0 section; it was not built with cargo auditable"
    return 1
}

check_packaged_binaries "embedded dependency data" check_one
