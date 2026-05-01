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
  };
}
