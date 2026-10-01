{ pkgs ? import <nixpkgs> {} }:
  let
    libPath = with pkgs; lib.makeLibraryPath [
      libGL
      libxkbcommon
      wayland
    ];
  in {
    devShell = with pkgs; mkShell {
      buildInputs = [
        cargo
        pkg-config
        libxkbcommon
        wayland
        rustc
        rust-analyzer
        # What the test suite runs for real; see CONTRIBUTING.md.
        rclone
        curl
        openssl
        fuse3
        dbus
        gnome-keyring
      ];

      RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
      LD_LIBRARY_PATH = libPath;
    };
  }
