# NixOS Wizard

A terminal-based installer for NixOS, similar to `archinstall` for Arch Linux. This project provides an interactive TUI (Terminal User Interface) to guide users through the NixOS installation process.

## ⚠️ Development Status

**This project is in early development and should be considered experimental.**

- The installer logic is functional but still being refined
- Some features may be incomplete or change significantly
- Use at your own risk - always have backups and recovery plans
- Not recommended for production systems yet

## Features

- Interactive terminal-based interface using Ratatui
- Guided disk partitioning and filesystem setup
- User account creation and configuration
- System package selection
- NixOS configuration generation (both traditional and flake-based)
- Real-time installation progress tracking

## Requirements

- Must be run as root
- Depends on several NixOS-specific commands, like `nixos-install` and `nixos-generate-config`. It is recommended to run this in a live NixOS installer environment.
- Terminal with proper color support

## Building

This project uses Nix flakes for development and building:

```bash
# Development shell
nix develop

# Build the package
nix build
```

## Running

```bash
# Run directly (as root)
sudo ./target/release/nixos-wizard

# Or via Nix
sudo nix run github:km-clay/nixos-wizard
```

## ISO Integration

The project includes configuration for building custom NixOS installer ISOs that include nixos-wizard:

```bash
# Build installer ISO
nix build github:km-clay/nixos-wizard#nixosConfigurations.installerIso.config.system.build.isoImage
```
