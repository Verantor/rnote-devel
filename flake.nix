{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];

        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in
      {
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            pkg-config
            ninja

            gcc
            clang
            clang-tools
            python3
            gnumake
            cmake
            meson
            just
            git
            openssl
            appstream
            gettext
            desktop-file-utils
            shared-mime-info
            appstream-glib

            (rust-bin.stable.latest.default.override {
              extensions = [
                "rust-src"
                "rust-analyzer"
              ];
            })

            cargo
            clippy
            rustfmt
          ];

          buildInputs = with pkgs; [
            glib
            gtk4
            libadwaita
            librsvg
            alsa-lib
            openssl
            libxml2
          ];
        };
      }
    );
}
