{
  description = "Nixos TUI Installer";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    fenix.url = "github:nix-community/fenix";
    disko.url = "github:nix-community/disko/latest";
  };

  outputs = { self, nixpkgs, fenix, disko }@inputs:
  let
    system = "x86_64-linux";
    mkRustToolchain = fenix.packages.${system}.complete.withComponents;
    pkgs = import nixpkgs { inherit system; };
    diskoPkg = disko.packages.${system}.disko;
    nixosWizard = pkgs.rustPlatform.buildRustPackage {
      pname = "nixos-wizard";
      version = "0.1.0";

      src = ./.;

      cargoLock = {
        lockFile = ./Cargo.lock;
      };

      buildInputs = [];
    };
  in
  {
    nixosConfigurations = {
      installerIso = nixpkgs.lib.nixosSystem {
        specialArgs = { inherit inputs diskoPkg nixosWizard; };
        modules = [
          ./isoimage/config.nix
        ];
      };
    };

    packages.${system} = {
      default = nixosWizard;
    };

    devShells.${system}.default = let
      toolchain = mkRustToolchain [
        "cargo"
        "clippy"
        "rustfmt"
        "rustc"
      ];
    in
      pkgs.mkShell {
        packages = [ toolchain pkgs.rust-analyzer ];

        shellHook = ''
          export SHELL=${pkgs.zsh}/bin/zsh
          exec ${pkgs.zsh}/bin/zsh
        '';
      };
  };
}
