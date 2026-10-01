# SPDX-License-Identifier: GPL-3.0-only
#
# Shared by the scripts that inspect what `./install.sh package` wrote to
# dist/: extracts every .deb, .rpm and .tar.gz there and runs one check
# against every ELF binary inside. Sourced, not run.
#
# Usage, from a script that has `set -euo pipefail` and has cd'd to the
# repository root:
#
#   source scripts/lib/packages.sh
#   check_one() { ...; }          # "$1" is a binary; return 0 to pass, else 1
#   check_packaged_binaries "what is being proven" check_one
#
# `check_one` prints its own PASS/FAIL detail. The function returns nonzero
# if any binary failed, or if nothing was found to check at all — a check
# that finds no binaries must not pass.

pkg_workdir=""
pkg_checked=0
pkg_failed=0

_pkg_check_dir() {
    local label="$1" dir="$2" check="$3" found_any=0 bin mime
    while IFS= read -r -d '' bin; do
        # Only real ELF binaries: a package also carries text files
        # (README, .desktop, metainfo) that could contain anything.
        mime=$(file --brief --mime-type "$bin")
        [[ "$mime" == "application/x-executable" \
            || "$mime" == "application/x-pie-executable" \
            || "$mime" == "application/x-sharedlib" ]] || continue
        found_any=1
        pkg_checked=$((pkg_checked + 1))
        if ! "$check" "$label" "$bin"; then
            pkg_failed=1
        fi
    done < <(find "$dir" -type f -print0)
    if [[ "$found_any" -eq 0 ]]; then
        echo "FAIL ($label): no ELF binaries found to check in $dir — the extraction may be broken" >&2
        pkg_failed=1
    fi
}

check_packaged_binaries() {
    local purpose="$1" check="$2" previous
    pkg_workdir=$(mktemp -d)
    # Added to the caller's own EXIT trap rather than replacing it, which
    # would leak whatever that one cleans up. `trap -p` prints the command
    # quoted for reuse; `eval set --` unquotes it again.
    eval "set -- $(trap -p EXIT)"
    previous="${3-}"
    trap "rm -rf \"\$pkg_workdir\"${previous:+; $previous}" EXIT
    pkg_checked=0
    pkg_failed=0

    shopt -s nullglob
    local debs=(dist/*.deb) rpms=(dist/*.rpm) tarballs=(dist/*.tar.gz)
    shopt -u nullglob

    if [[ ${#debs[@]} -eq 0 && ${#rpms[@]} -eq 0 && ${#tarballs[@]} -eq 0 ]]; then
        echo "FAIL: no packages found in dist/ — run ./install.sh package first" >&2
        return 1
    fi

    local deb rpm tarball out payload
    for deb in "${debs[@]}"; do
        out="$pkg_workdir/deb"
        mkdir -p "$out"
        dpkg-deb -x "$deb" "$out"
        _pkg_check_dir "$(basename "$deb")" "$out" "$check"
    done

    for rpm in "${rpms[@]}"; do
        out="$pkg_workdir/rpm"
        mkdir -p "$out"
        # A real intermediate file rather than a pipe: piping rpm2cpio
        # straight into cpio left a failure on either side unreported.
        payload="$pkg_workdir/$(basename "$rpm").cpio"
        # rpm2cpio's own exit code is not reliable proof of anything: on at
        # least one real build of it, confirmed directly, it exits 1 even
        # after writing a complete, valid cpio stream. A nonzero exit is
        # noted, not fatal; cpio's own exit code below, and the "found no
        # binaries" check, are what catch a genuinely broken extraction.
        if ! rpm2cpio "$rpm" >"$payload"; then
            echo "NOTE ($(basename "$rpm")): rpm2cpio exited nonzero; checking the payload it wrote anyway"
        fi
        if ! (cd "$out" && cpio -idm --quiet <"$payload"); then
            echo "FAIL ($(basename "$rpm")): cpio failed unpacking the payload" >&2
            pkg_failed=1
            continue
        fi
        _pkg_check_dir "$(basename "$rpm")" "$out" "$check"
    done

    for tarball in "${tarballs[@]}"; do
        out="$pkg_workdir/tarball"
        mkdir -p "$out"
        tar -xzf "$tarball" -C "$out"
        _pkg_check_dir "$(basename "$tarball")" "$out" "$check"
    done

    echo
    echo "Checked $pkg_checked binaries across $(( ${#debs[@]} + ${#rpms[@]} + ${#tarballs[@]} )) packages ($purpose)."
    if [[ $pkg_failed -ne 0 ]]; then
        echo "RESULT: FAILED"
        return 1
    fi
    echo "RESULT: every shipped binary passed"
}
