#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Prove that `--features release-build` really removes developer debug
# logging, rather than trusting that it does.
#
# Reading `src/debug.rs` is not evidence: the constant can be correct and the
# code still be emitted, or a packaging target can forget to pass the feature.
# This builds the binary both ways with the developer switch forced on and
# checks whether the debug log path survives into the binary.
#
# Usage: scripts/verify-release-build.sh
set -euo pipefail

cd "$(dirname "$0")/.."

LOG_PATH=$(grep -oP 'pub const PATH: &str = "\K[^"]+' src/debug.rs)
BINS=(stellarshot stellarshot-applet stellarshot-web)

if [[ -z "$LOG_PATH" ]]; then
    echo "FAIL: could not find the debug log PATH constant in src/debug.rs" >&2
    exit 1
fi
echo "Debug log path under test: $LOG_PATH"

restore() {
    if [[ -f src/debug.rs.orig ]]; then
        mv src/debug.rs.orig src/debug.rs
    fi
}
trap restore EXIT

# Force the developer switch on, so the only variable is the feature flag.
cp src/debug.rs src/debug.rs.orig
sed -i 's/^const DEVELOPER_LOGGING: bool = false;/const DEVELOPER_LOGGING: bool = true;/' src/debug.rs

if ! grep -q 'const DEVELOPER_LOGGING: bool = true;' src/debug.rs; then
    echo "FAIL: could not force DEVELOPER_LOGGING on; the constant may have been renamed" >&2
    exit 1
fi

fail=0
symbols=$(mktemp)
trap 'restore; rm -f "$symbols"' EXIT

# `strings ... | grep -q` must not be used here: grep exits on the first match
# and closes the pipe, `strings` dies of SIGPIPE, and under `set -o pipefail`
# the pipeline reports failure even though the string was found — which would
# invert both results below. Dump to a file and grep that instead.
contains_log_path() {
    strings "target/debug/$1" > "$symbols"
    grep -qF "$LOG_PATH" "$symbols"
}

for bin in "${BINS[@]}"; do
    echo
    echo "== $bin: building WITHOUT release-build (logging should be present) =="
    cargo build --quiet --bin "$bin"
    if contains_log_path "$bin"; then
        echo "PASS: the log path is present, so this check can detect its absence"
    else
        echo "FAIL: the log path is absent even with logging enabled."
        echo "      This check cannot prove anything in that state — it would report"
        echo "      success no matter what the feature flag did."
        fail=1
    fi

    echo
    echo "== $bin: building WITH release-build (logging must be stripped) =="
    cargo build --quiet --bin "$bin" --features release-build
    if contains_log_path "$bin"; then
        echo "FAIL: the log path is still in the binary; debug logging was not stripped"
        fail=1
    else
        echo "PASS: the log path is gone; the optimizer removed the logging code"
    fi
done

echo
if [[ $fail -ne 0 ]]; then
    echo "RESULT: FAILED"
    exit 1
fi
echo "RESULT: release-build strips developer logging"
