{
  description = "Nixos TUI Installer";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    fenix.url = "github:nix-community/fenix";
  };

  outputs = { self, nixpkgs, fenix }@inputs:
  let
    system = "x86_64-linux";
    mkRustToolchain = fenix.packages.${system}.complete.withComponents;
    pkgs = import nixpkgs { inherit system; };
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
        specialArgs = { inherit inputs nixosWizard; };
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
