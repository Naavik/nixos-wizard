{ pkgs, modulesPath, nixosWizard, ... }: {
  imports = [
    "${modulesPath}/installer/cd-dvd/installation-cd-minimal.nix"
  ];

  environment.systemPackages = [
    pkgs.nixfmt
    pkgs.nixfmt-classic
    nixosWizard
  ];

  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  nixpkgs.hostPlatform = "x86_64-linux";
  networking.networkmanager.enable = true;
}
