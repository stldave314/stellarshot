#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Build, install and package Stellarshot.
#
# One script so CI and local builds share a single path and cannot drift.
#
#   ./install.sh              build and install system-wide
#   ./install.sh build        build only
#   ./install.sh install      install an already-built tree
#   ./install.sh uninstall    remove an installed copy
#   ./install.sh deb          build a .deb into dist/
#   ./install.sh rpm          build an .rpm into dist/
#   ./install.sh tarball      build a portable tarball into dist/
#   ./install.sh package      build all three
#
# Environment:
#   PREFIX      install prefix (default /usr)
#   DESTDIR     staging root, for packaging
#   CARGO_JOBS  limit parallel compile jobs (e.g. 4 on a small machine)
set -euo pipefail

cd "$(dirname "$0")"

PREFIX="${PREFIX:-/usr}"
DESTDIR="${DESTDIR:-}"

APP_ID="io.github.stldave314.Stellarshot"
BIN_APP="stellarshot"
BIN_APPLET="stellarshot-applet"
APPLET_ID="$APP_ID.Applet"
DIST="dist"

# Every packaging target passes this. It forces developer debug logging off at
# compile time, so a released build can never carry it — this is deliberately
# mechanical rather than something to remember.
FEATURES="release-build"

info()  { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn()  { printf '\033[1;33m==>\033[0m %s\n' "$*" >&2; }
die()   { printf '\033[1;31m==>\033[0m %s\n' "$*" >&2; exit 1; }

need() {
    command -v "$1" >/dev/null 2>&1 || die "$1 is required but not installed${2:+ ($2)}"
}

# Run a command as root, or directly if already root.
as_root() {
    if [[ $EUID -eq 0 ]]; then
        "$@"
    elif command -v sudo >/dev/null 2>&1; then
        sudo "$@"
    else
        die "root privileges are required and sudo is not available"
    fi
}

# Set once `cmd_build` has run, so `package` builds once rather than once
# per format.
BUILT=0

cmd_build() {
    [[ "$BUILT" -eq 0 ]] || return 0
    need cargo "install a Rust toolchain from https://rustup.rs"
    info "Building (features: $FEATURES)"
    # `cargo auditable` embeds the exact dependency tree into the binary
    # itself, so `cargo audit bin` (or a distribution's own scanner) can
    # check what a *shipped* binary actually contains, not just what
    # Cargo.lock said at build time. Optional: a plain source build should
    # not gain a new hard dependency, but every packaging target
    # (`cmd_deb`/`cmd_rpm`/`cmd_tarball`, and CI) goes through this same
    # function, so installing it once makes every release artifact
    # auditable without a separate code path to keep in sync.
    if cargo auditable --version >/dev/null 2>&1; then
        cargo auditable build --release --locked --features "$FEATURES" \
            ${CARGO_JOBS:+-j "$CARGO_JOBS"}
    elif [[ "${CI:-}" == "true" ]]; then
        # A CI build is what produces the release artifacts; shipping them
        # without the embedded dependency data would only show up when
        # someone tried to audit one.
        die "cargo-auditable is required in CI (cargo install cargo-auditable)"
    else
        warn "cargo-auditable not installed; building without embedded dependency data" \
            "(cargo install cargo-auditable)"
        cargo build --release --locked --features "$FEATURES" \
            ${CARGO_JOBS:+-j "$CARGO_JOBS"}
    fi
    info "Built target/release/$BIN_APP and target/release/$BIN_APPLET"
    BUILT=1
}

# Install into $1 (a staging root, possibly empty for a real install).
# Uses plain `install` so it works the same under fakeroot for packaging.
stage() {
    local root="$1"
    local runner=("${@:2}")

    "${runner[@]}" install -Dm755 "target/release/$BIN_APP"    "$root$PREFIX/bin/$BIN_APP"
    "${runner[@]}" install -Dm755 "target/release/$BIN_APPLET" "$root$PREFIX/bin/$BIN_APPLET"

    "${runner[@]}" install -Dm644 "res/$APP_ID.desktop" \
        "$root$PREFIX/share/applications/$APP_ID.desktop"
    "${runner[@]}" install -Dm644 "res/$APPLET_ID.desktop" \
        "$root$PREFIX/share/applications/$APPLET_ID.desktop"

    "${runner[@]}" install -Dm644 "res/icons/hicolor/scalable/apps/$APP_ID.svg" \
        "$root$PREFIX/share/icons/hicolor/scalable/apps/$APP_ID.svg"
    "${runner[@]}" install -Dm644 "res/icons/hicolor/scalable/apps/$APP_ID-symbolic.svg" \
        "$root$PREFIX/share/icons/hicolor/scalable/apps/$APP_ID-symbolic.svg"

    "${runner[@]}" install -Dm644 "res/$APP_ID.metainfo.xml" \
        "$root$PREFIX/share/metainfo/$APP_ID.metainfo.xml"

    "${runner[@]}" install -Dm644 LICENSE \
        "$root$PREFIX/share/licenses/$BIN_APP/LICENSE"
}

cmd_install() {
    [[ -x "target/release/$BIN_APP" ]] || die "nothing built; run ./install.sh build first"

    if [[ -n "$DESTDIR" ]]; then
        info "Staging into $DESTDIR$PREFIX"
        stage "$DESTDIR"
    else
        # Installed system-wide so the desktop entry lands in the scan path
        # the launcher and the autostart machinery both read.
        info "Installing into $PREFIX (requires root)"
        stage "" as_root
        as_root update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
        as_root gtk-update-icon-cache -f "$PREFIX/share/icons/hicolor" 2>/dev/null || true
    fi

    info "Installed."
    echo
    echo "  Next: launch Stellarshot from the application launcher."
    echo
    if ! command -v rclone >/dev/null 2>&1; then
        warn "rclone is not installed. Local and USB backups work without it;"
        warn "cloud destinations such as Google Drive will need it."
    fi
}

cmd_uninstall() {
    info "Removing installed files from $PREFIX"
    as_root rm -f \
        "$PREFIX/bin/$BIN_APP" \
        "$PREFIX/bin/$BIN_APPLET" \
        "$PREFIX/share/applications/$APP_ID.desktop" \
        "$PREFIX/share/applications/$APPLET_ID.desktop" \
        "$PREFIX/share/icons/hicolor/scalable/apps/$APP_ID.svg" \
        "$PREFIX/share/icons/hicolor/scalable/apps/$APP_ID-symbolic.svg" \
        "$PREFIX/share/metainfo/$APP_ID.metainfo.xml"
    as_root rm -rf "$PREFIX/share/licenses/$BIN_APP" "$PREFIX/share/doc/$BIN_APP"
    as_root update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
    as_root gtk-update-icon-cache -f "$PREFIX/share/icons/hicolor" 2>/dev/null || true
    info "Removed. Settings in ~/.config/cosmic/$APP_ID and every repository were kept."
    warn "Any per-user systemd unit (a scheduled backup) is left" \
        "running and installed, since it belongs to your user account, not this" \
        "prefix. Stopping and disabling a unit does not delete its file, so both" \
        "steps are needed to remove them yourself:"
    warn "  systemctl --user list-unit-files --no-legend 'stellarshot-backup-*.timer' 'stellarshot-backup-*.path' \\"
    warn "    | awk '{print \$1}' | xargs -r systemctl --user disable --now"
    warn "  rm -f ~/.config/systemd/user/stellarshot-backup-*.{service,timer,path}"
    warn "  systemctl --user daemon-reload"
}

cmd_deb() {
    need cargo
    cargo deb --version >/dev/null 2>&1 || die "cargo-deb is required: cargo install cargo-deb"
    # Built once, by `cmd_build`, and packaged as it is: `cargo deb` would
    # otherwise run its own plain `cargo build` — without `cargo auditable`,
    # so the .deb's binaries would carry no dependency data, and not
    # necessarily the same binaries the .rpm and tarball ship.
    cmd_build
    mkdir -p "$DIST"
    info "Building .deb"
    cargo deb --no-build --output "$DIST"
    info "Wrote $(ls -1 "$DIST"/*.deb | tail -1)"
}

cmd_rpm() {
    need cargo
    cargo generate-rpm --version >/dev/null 2>&1 \
        || die "cargo-generate-rpm is required: cargo install cargo-generate-rpm"
    cmd_build
    mkdir -p "$DIST"
    info "Building .rpm"
    # gzip, not cargo-generate-rpm's own default of zstd: the rpm2cpio this
    # package ships to be inspected with (and older rpm itself, on an older
    # RPM-based distro someone might actually run this on) isn't guaranteed
    # to have been built with zstd support, and gzip is universally readable
    # by every rpm2cpio there has ever been.
    cargo generate-rpm --payload-compress gzip --output "$DIST"
    info "Wrote $(ls -1 "$DIST"/*.rpm | tail -1)"
}

cmd_tarball() {
    cmd_build
    local version arch stagedir name
    version=$(cargo metadata --no-deps --format-version 1 \
        | sed -n 's/.*"name":"stellarshot","version":"\([^"]*\)".*/\1/p')
    [[ -n "$version" ]] || die "could not determine the package version"
    arch=$(uname -m)
    name="$BIN_APP-$version-$arch"
    stagedir="$DIST/$name"

    info "Building tarball $name.tar.gz"
    rm -rf "$stagedir"
    mkdir -p "$stagedir"

    PREFIX=/usr stage "$stagedir"
    install -Dm755 install-tarball.sh "$stagedir/install-tarball.sh"
    install -Dm644 README.md "$stagedir/README.md"

    # Owned by root in the archive regardless of who built it: a plain
    # `tar -czf` records the *building* user's uid/gid, and extracting that
    # as root (as `install-tarball.sh` does for a system install) would
    # carry that uid into every file's ownership on disk.
    tar --owner=0 --group=0 --numeric-owner --sort=name \
        --mtime="@${SOURCE_DATE_EPOCH:-0}" \
        -czf "$DIST/$name.tar.gz" -C "$DIST" "$name"
    rm -rf "$stagedir"
    info "Wrote $DIST/$name.tar.gz"
}

cmd_package() {
    cmd_deb
    cmd_rpm
    cmd_tarball
}

case "${1:-all}" in
    build)      cmd_build ;;
    install)    cmd_install ;;
    uninstall)  cmd_uninstall ;;
    deb)        cmd_deb ;;
    rpm)        cmd_rpm ;;
    tarball)    cmd_tarball ;;
    package)    cmd_package ;;
    all)        cmd_build; cmd_install ;;
    -h|--help|help)
        sed -n '3,20p' "$0" | sed 's/^# \{0,1\}//'
        ;;
    *)
        die "unknown command '$1' (try ./install.sh --help)"
        ;;
esac
