use std::{path::PathBuf, process::{Command, Stdio}};
use serde_json::Value;

use crate::{attrset, merge_attrs};

pub fn nixstr(val: impl ToString) -> String {
	let val = val.to_string();
	format!("\"{val}\"")
}
pub fn fmt_nix(nix: String) -> anyhow::Result<String> {
	// This installer should be run from the flake that provides it
	// And that flake provides 'nixfmt' as a build input, so we can assume it exists in this environment
	// isn't Nix a nice thing
	let mut nixfmt_child = Command::new("nixfmt")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.spawn()?;

	if let Some(stdin) = nixfmt_child.stdin.as_mut() {
		use std::io::Write;
		stdin.write_all(nix.as_bytes())?;
	}

	let output = nixfmt_child.wait_with_output()?;
	if output.status.success() {
		let formatted = String::from_utf8(output.stdout)?;
		Ok(formatted)
	} else {
		let err = String::from_utf8_lossy(&output.stderr);
		Err(anyhow::anyhow!("nixfmt failed: {}", err))
	}
}

pub struct NixSerializer {
	pub config: Value,
	pub output_dir: PathBuf,
	pub use_flake: bool
}
/*
{
  "config": {
    "audioBackend": "PulseAudio",
    "bootloader": "systemd-boot",
    "desktopEnvironment": "KDE Plasma",
    "enableSwap": true,
    "greeter": "SDDM",
    "hostname": "oganesson",
    "kernel": "linux",
    "keyboardLayout": "us",
    "language": "en_US",
    "locale": "en_US.UTF-8",
    "networkBackend": "NetworkManager",
    "profile": "Desktop",
    "rootPassword": "changeme",
    "swapSize": "10G",
    "timezone": "America/New_York",
    "useFlakes": true
  }
}
*/

impl NixSerializer {
	pub fn new(config: Value, output_dir: PathBuf, use_flake: bool) -> Self {
		Self { config, output_dir, use_flake }
	}
	pub fn mk_nix_config(&self) -> anyhow::Result<String> {
		let mut cfg_attrs = String::from("{}");
		let Value::Object(ref cfg) = self.config else { unreachable!() };
		for key in cfg.keys() {
			match key.as_str() {
				"audioBackend" => {
					if let Some(Value::String(value)) = cfg.get(key) {
						let audio_attrs = self.parse_audio(value.clone());
						cfg_attrs = merge_attrs!(cfg_attrs, audio_attrs);
					}
				}
				"bootloader" => {
					if let Some(Value::String(value)) = cfg.get(key) {
						let bootloader_attrs = self.parse_bootloader(value.clone());
						cfg_attrs = merge_attrs!(cfg_attrs, bootloader_attrs);
					}
				}
				"desktopEnvironment" => {
					if let Some(Value::String(value)) = cfg.get(key) {
						let de_attrs = self.parse_desktop_environment(value.clone());
						cfg_attrs = merge_attrs!(cfg_attrs, de_attrs);
					}
				}
				"greeter" => {
					if let Some(Value::String(value)) = cfg.get(key) {
						let greeter_attrs = self.parse_greeter(value.clone());
						cfg_attrs = merge_attrs!(cfg_attrs, greeter_attrs);
					}
				}
				"hostname" => {
					if let Some(Value::String(value)) = cfg.get(key) {
						let hostname_attrs = self.parse_hostname(value.clone());
						cfg_attrs = merge_attrs!(cfg_attrs, hostname_attrs);
					}
				}
				"kernel" => {
					if let Some(Value::String(value)) = cfg.get(key) {
						let kernel_attrs = self.parse_kernel(value.clone());
						cfg_attrs = merge_attrs!(cfg_attrs, kernel_attrs);
					}
				}
				"keyboardLayout" => {
					if let Some(Value::String(value)) = cfg.get(key) {
						let kb_attrs = self.parse_kb_layout(value.clone());
						cfg_attrs = merge_attrs!(cfg_attrs, kb_attrs);
					}
				}
				"locale" => {
					if let Some(Value::String(value)) = cfg.get(key) {
						let locale_attrs = self.parse_locale(value.clone());
						cfg_attrs = merge_attrs!(cfg_attrs, locale_attrs);
					}
				}
				"networkBackend" => {
					if let Some(Value::String(value)) = cfg.get(key) {
						let net_attrs = self.parse_network_backend(value.clone());
						cfg_attrs = merge_attrs!(cfg_attrs, net_attrs);
					}
				}
				"timezone" => {
					if let Some(Value::String(value)) = cfg.get(key) {
						let tz_attrs = self.parse_timezone(value.clone());
						cfg_attrs = merge_attrs!(cfg_attrs, tz_attrs);
					}
				}
				_ => {
					// Ignore other keys for now
				}
			}
		}
		let raw = format!("{{ config, pkgs, ... }}: {cfg_attrs}");
		fmt_nix(raw)
	}
	fn parse_timezone(&self, value: String) -> String {
		attrset! {
			"time.timeZone" = nixstr(&value);
		}
	}
	pub fn parse_network_backend(&self, value: String) -> String {
		match value.to_lowercase().as_str() {
			"networkmanager" => attrset! {
				"networking.networkmanager.enable" = "true";
			},
			"wpa_supplicant" => attrset! {
				"networking.wireless.enable" = "true";
			},
			_ => panic!("Unsupported network backend: {value}"),
		}
	}
	pub fn parse_locale(&self, value: String) -> String {
		attrset! {
			"i18n.defaultLocale" = nixstr(&value);
		}
	}
	fn parse_kb_layout(&self, value: String) -> String {
		attrset! {
			"services.xserver.xkb.layout" = nixstr(value);
		}
	}
	fn parse_kernel(&self, value: String) -> String {
		let kernel_pkg = match value.to_lowercase().as_str() {
			"linux" => "pkgs.linuxPackages".to_string(),
			_ => panic!("Unsupported kernel: {value}"),
		};
		attrset! {
			"boot.kernelPackages" = kernel_pkg;
		}
	}
	fn parse_hostname(&self, value: String) -> String {
		format!("networking.hostName = \"{value}\";")
	}
	fn parse_greeter(&self, value: String) -> String {
		match value.to_lowercase().as_str() {
			"sddm" => attrset! {
				"services.displayManager.sddm.enable" = true;
			},
			"gdm" => attrset! {
				"services.xserver.displayManager.gdm.enable" = true;
			},
			"lightdm" => attrset! {
				"services.xserver.displayManager.lightdm.enable" = true;
			},
			_ => panic!("Unsupported greeter: {value}"),
		}
	}
	fn parse_desktop_environment(&self, value: String) -> String {
		match value.to_lowercase().as_str() {
			"gnome" => attrset! {
				"services.xserver.desktopManager.gnome.enable" = true;
			},
			"plasma" | "kde plasma" => attrset! {
				"services.xserver.desktopManager.plasma5.enable" = true;
			},
			"xfce" => attrset! {
				"services.xserver.desktopManager.xfce.enable" = true;
			},
			"cinnamon" => attrset! {
				"services.xserver.desktopManager.cinnamon.enable" = true;
			},
			"mate" => attrset! {
				"services.xserver.desktopManager.mate.enable" = true;
			},
			"lxqt" => attrset! {
				"services.xserver.desktopManager.lxqt.enable" = true;
			},
			_ => panic!("Unsupported desktop environment: {value}"),
		}
	}
	fn parse_audio(&self, value: String) -> String {
		match value.to_lowercase().as_str() {
			"pulseaudio" => attrset! {
				"services.pulseaudio.enable" = true;
			},
			"pipewire" => attrset! {
				"services.pipewire.enable" = true;
			},
			_ => panic!("Unsupported audio backend: {value}"),
		}
	}
	fn parse_bootloader(&self, value: String) -> String {
		let bootloader_attrs = match value.to_lowercase().as_str() {
			"systemd-boot" => attrset! {
				"systemd-boot.enable" = true;
				"efi.canTouchEfiVariables" = true;
			},

			"grub" => attrset! {
				grub = attrset! {
					enable = true;
					efiSupport = true;
					device = "nodev";
				};
				"efi.canTouchEfiVariables" = true;
			},
			_ => panic!("Unsupported bootloader: {value}"),
		};
		attrset! {
			"boot.loader" = bootloader_attrs;
		}
	}
}
