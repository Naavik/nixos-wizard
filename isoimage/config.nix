{ pkgs, modulesPath, nixosWizard, ... }: {
  imports = [
    "${modulesPath}/installer/cd-dvd/installation-cd-minimal.nix"
  ];

  environment.etc."issue".text = ''
\e[92m<<< Welcome to NixOS 25.11.20250728.dc96378 \r (\m) - \l >>>\e[0m
The "nixos" and "root" accounts have empty passwords.

To log in over ssh you must set a password for either "nixos" or "root"
with `passwd` (prefix with `sudo` for "root"), or add your public key to
/home/nixos/.ssh/authorized_keys or /root/.ssh/authorized_keys.

To set up a wireless connection, run `nmtui`.


Run 'sudo nixos-wizard' to enter the installer.

Run 'nixos-help' for the NixOS manual.
  '';

  environment.systemPackages = [
    pkgs.nixfmt
    pkgs.nixfmt-classic
    nixosWizard
  ];

  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  nixpkgs.hostPlatform = "x86_64-linux";
  networking.networkmanager.enable = true;
}
