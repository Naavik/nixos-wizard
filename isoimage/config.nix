{ pkgs, modulesPath, nixosWizard, ... }: {
  imports = [
    "${modulesPath}/installer/cd-dvd/installation-cd-minimal.nix"
  ];

  environment.systemPackages = [
    nixosWizard
  ];

  nixpkgs.hostPlatform = "x86_64-linux";
  networking.networkmanager.enable = true;
}
