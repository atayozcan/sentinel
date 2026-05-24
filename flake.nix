# SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# A development shell for hacking on Sentinel-KDE (Rust + cxx-qt + KF6).
# Sentinel installs via ./install.sh on openSUSE Tumbleweed; a full Nix
# package of the cxx-qt/Kirigami helper is a TODO (it needs Qt app wrapping).
{
  description = "Sentinel-KDE — UAC-style PAM + polkit confirmation dialog for KDE Plasma";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in {
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rustToolchain
            pkg-config
            qt6.wrapQtAppsHook
          ];
          buildInputs = with pkgs; [
            pam
            qt6.qtbase
            qt6.qtdeclarative
            qt6.qtwayland
            kdePackages.kirigami
            kdePackages.qqc2-desktop-style
            kdePackages.layer-shell-qt
            kdePackages.breeze-icons
          ];
          shellHook = ''
            export SENTINEL_HELPER_PATH=/usr/lib/sentinel-helper-kde
          '';
        };
      });
}
