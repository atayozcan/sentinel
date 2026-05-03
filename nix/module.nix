# SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
# SPDX-License-Identifier: GPL-3.0-or-later
# NixOS module for Sentinel.
#
# Usage in configuration.nix / flake:
#
#   imports = [ inputs.sentinel.nixosModules.default ];
#   services.sentinel.enable = true;
#
{ config, lib, pkgs, ... }:

let
  cfg = config.services.sentinel;
in {
  options.services.sentinel = {
    enable = lib.mkEnableOption "Sentinel UAC-style PAM confirmation";

    package = lib.mkOption {
      type = lib.types.package;
      description = "Sentinel package to install.";
    };

    enableForSudo = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Wire pam_sentinel.so into /etc/pam.d/sudo as `sufficient`.";
    };

    polkitAgent.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Run sentinel-polkit-agent as the session's polkit authentication
        agent. When enabled, this systemd --user unit Conflicts= with
        cosmic-osd / polkit-gnome / polkit-kde so only one agent runs.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];

    environment.etc."security/sentinel.conf".source =
      "${cfg.package}/etc/security/sentinel.conf";

    security.pam.services.polkit-1.text = lib.mkForce ''
      #%PAM-1.0
      auth       sufficient ${cfg.package}/lib/security/pam_sentinel.so
      auth       include    system-auth
      account    include    system-auth
      password   include    system-auth
      session    include    system-auth
    '';

    security.pam.services.sudo.text = lib.mkIf cfg.enableForSudo (lib.mkForce ''
      #%PAM-1.0
      auth       sufficient ${cfg.package}/lib/security/pam_sentinel.so
      auth       include    system-auth
      account    include    system-auth
      password   include    system-auth
      session    include    system-auth
    '');

    # The polkit agent ships as an XDG autostart entry; the compositor
    # forks it as a direct child so it inherits the graphical session's
    # kernel sessionid. (A `systemd --user` unit can't satisfy polkit's
    # session check because user@1000.service runs in a different session
    # than the user's compositor.)
    environment.etc."xdg/autostart/sentinel-polkit-agent.desktop" = lib.mkIf cfg.polkitAgent.enable {
      source = "${cfg.package}/etc/xdg/autostart/sentinel-polkit-agent.desktop";
    };
  };
}