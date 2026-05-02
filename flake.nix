{
  description = "Sentinel — UAC-style PAM confirmation dialog for Linux";

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

        commonInputs = with pkgs; [
          pam
          wayland
          libxkbcommon
          fontconfig
          freetype
          mesa
          vulkan-loader
        ];

        sentinel = pkgs.rustPlatform.buildRustPackage {
          pname = "sentinel";
          version = "0.4.1";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
            allowBuiltinFetchGit = true;
          };

          nativeBuildInputs = with pkgs; [ pkg-config rustToolchain ];
          buildInputs = commonInputs;

          SENTINEL_PREFIX = "/usr";
          SENTINEL_SYSCONFDIR = "/etc";
          SENTINEL_LIBEXECDIR = "lib";

          postInstall = ''
            install -Dm755 target/release/libpam_sentinel.so \
              $out/lib/security/pam_sentinel.so
            install -Dm755 target/release/sentinel-helper \
              $out/lib/sentinel-helper
            install -Dm755 target/release/sentinel-polkit-agent \
              $out/lib/sentinel-polkit-agent
            install -Dm644 config/sentinel.conf \
              $out/etc/security/sentinel.conf
            install -Dm644 config/polkit-1 \
              $out/etc/pam.d/polkit-1
            install -Dm644 packaging/systemd/polkit-agent-helper@.service.d/sentinel.conf \
              $out/etc/systemd/system/polkit-agent-helper@.service.d/sentinel.conf
            install -Dm644 packaging/xdg-autostart/sentinel-polkit-agent.desktop \
              $out/etc/xdg/autostart/sentinel-polkit-agent.desktop

            # Generate completions + man pages.
            for bin in sentinel-helper sentinel-polkit-agent; do
              $out/lib/$bin completions bash > $out/share/bash-completion/completions/$bin
              $out/lib/$bin completions fish > $out/share/fish/vendor_completions.d/$bin.fish
              $out/lib/$bin completions zsh  > $out/share/zsh/site-functions/_$bin
              $out/lib/$bin man              > $out/share/man/man1/$bin.1
            done
            install -Dm644 packaging/man/sentinel.conf.5 \
              $out/share/man/man5/sentinel.conf.5
            install -Dm644 packaging/man/pam_sentinel.8 \
              $out/share/man/man8/pam_sentinel.8

            install -Dm644 LICENSE \
              $out/share/licenses/sentinel/LICENSE
            install -Dm644 README.md \
              $out/share/doc/sentinel/README.md
          '';

          meta = with pkgs.lib; {
            description = "UAC-style confirmation dialog for Linux privilege escalation";
            homepage = "https://github.com/atayozcan/sentinel";
            license = licenses.gpl3Plus;
            platforms = platforms.linux;
          };
        };
      in {
        packages = {
          default = sentinel;
          sentinel = sentinel;
        };

        apps.default = flake-utils.lib.mkApp {
          drv = sentinel;
          name = "sentinel-helper";
          exePath = "/lib/sentinel-helper";
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = [ rustToolchain pkgs.pkg-config ];
          buildInputs = commonInputs;
        };
      }) // {
        nixosModules.default = { config, lib, pkgs, ... }@args:
          import ./nix/module.nix (args // {
            config = args.config // {
              services.sentinel.package = lib.mkDefault
                self.packages.${pkgs.stdenv.hostPlatform.system}.default;
            };
          });
      };
}
