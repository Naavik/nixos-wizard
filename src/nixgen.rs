use serde_json::Value;
use std::{
  path::PathBuf,
  process::{Command, Stdio},
};

use crate::{attrset, installer::users::User, merge_attrs};

/// Just wraps a string in quotes basically
///
/// Necessary for the generator to produce valid Nix strings
/// and way better than writing 'format!("\"{string}\"")' everywhere
pub fn nixstr(val: impl ToString) -> String {
  let val = val.to_string();
  format!("\"{val}\"")
}
pub fn fmt_nix(nix: String) -> anyhow::Result<String> {
  // This installer should be run from the flake that provides it
  // And that flake provides 'nixfmt' as a build input, so we can assume it exists
  // in this environment isn't Nix a nice thing
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
pub fn highlight_nix(nix: &str) -> anyhow::Result<String> {
  let mut bat_child = Command::new("bat")
    .arg("-p")
    .arg("-f")
    .arg("-l")
    .arg("nix")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .spawn()?;
  if let Some(stdin) = bat_child.stdin.as_mut() {
    use std::io::Write;
    stdin.write_all(nix.as_bytes())?;
  }

  let output = bat_child.wait_with_output()?;
  if output.status.success() {
    let highlighted = String::from_utf8(output.stdout)?;
    Ok(highlighted)
  } else {
    let err = String::from_utf8_lossy(&output.stderr);
    Err(anyhow::anyhow!("bat failed: {}", err))
  }
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
#[derive(Debug)]
pub struct Configs {
  pub system: String,
  pub disko: String,
  pub flake_path: Option<String>,
}

pub struct NixWriter {
  config: Value,
}

impl NixWriter {
  pub fn new(config: Value) -> Self {
    Self { config }
  }
  pub fn write_configs(&self) -> anyhow::Result<Configs> {
    let disko = {
      let config = self.config["disko"].clone();
      self.write_disko_config(config)?
    };
    let sys_cfg = {
      let config = self.config["config"].clone();
      self.write_sys_config(config)?
    };
    let flake_path = self
      .config
      .get("flake_path")
      .and_then(|v| v.as_str().map(|s| s.to_string()));

    Ok(Configs {
      system: sys_cfg,
      disko,
      flake_path,
    })
  }
  pub fn write_sys_config(&self, config: Value) -> anyhow::Result<String> {
    // initialize the attribute set
    let Value::Object(ref cfg) = config else {
      return Err(anyhow::anyhow!("Config must be a JSON object"));
    };
    let mut cfg_attrs = String::from("{}");
    let mut install_home_manager = false;
    for (key, value) in cfg.iter() {
      log::debug!("Processing key: {key}");
      log::debug!("Value: {value}");
      let parsed_config = match key.trim().to_lowercase().as_str() {
        "audio_backend" => value.as_str().map(Self::parse_audio),
        "bootloader" => {
          let res = value.as_str().map(Self::parse_bootloader);
          match res {
            Some(Ok(cfg)) => Some(cfg),
            Some(Err(e)) => return Err(e),
            None => None,
          }
        }
        "desktop_environment" => value.as_str().map(Self::parse_desktop_environment),
        "enable_flakes" => value
          .as_bool()
          .filter(|&b| b)
          .map(|_| Self::parse_enable_flakes()),
        "greeter" => None,
        "hostname" => value.as_str().map(Self::parse_hostname),
        "kernels" => value.as_array().map(Self::parse_kernels),
        "keyboard_layout" => value.as_str().map(Self::parse_kb_layout),
        "locale" => value.as_str().map(Self::parse_locale),
        "network_backend" => value.as_str().map(Self::parse_network_backend),
        "profile" => None,
        "root_passwd_hash" => Some(Self::parse_root_pass_hash(value)?),
        "system_pkgs" => value.as_array().map(Self::parse_system_packages),
        "timezone" => value.as_str().map(Self::parse_timezone),
        "use_swap" => value.as_bool().filter(|&b| b).map(|_| Self::parse_swap()),
        "users" => {
          let users: Vec<User> = serde_json::from_value(value.clone())?;
          install_home_manager = users.iter().any(|user| user.home_manager_cfg.is_some());
          Some(self.parse_users(users)?)
        }
        _ => {
          log::warn!("Unknown configuration key: {key}");
          None
        }
      };

      if let Some(config) = parsed_config {
        cfg_attrs = merge_attrs!(cfg_attrs, config);
      }
    }
    let imports = if install_home_manager {
      String::from(
        r#"{imports = [ (import "${home-manager}/nixos") ./hardware-configuration.nix ];}"#,
      )
    } else {
      String::from("{imports = [./hardware-configuration.nix];}")
    };

    let state_version = attrset! {
      "system.stateVersion" = nixstr("25.11");
    };

    cfg_attrs = merge_attrs!(imports, cfg_attrs, state_version);

    let raw = if install_home_manager {
      format!(
        "{{ config, pkgs, ... }}: let home-manager = builtins.fetchTarball https://github.com/nix-community/home-manager/archive/release-25.05.tar.gz; in {cfg_attrs}"
      )
    } else {
      format!("{{ config, pkgs, ... }}: {cfg_attrs}")
    };
    fmt_nix(raw)
  }
  /*
  "disko": {
    "content": {
      "partitions": {
        "BOOT": {
          "format": "vfat",
          "mountpoint": "/boot",
          "size": "524M",
          "type": "EF00"
        },
        "ROOT": {
          "format": "ext4",
          "mountpoint": "/",
          "size": "2T",
          "type": "8300"
        }
      },
      "type": "gpt"
    },
    "device": "/dev/nvme1n1",
    "type": "disk"
  },
   */
  pub fn write_disko_config(&self, config: Value) -> anyhow::Result<String> {
    log::debug!("Writing Disko config: {config}");
    let device = config["device"].as_str().unwrap_or("/dev/sda");
    let disk_type = config["type"].as_str().unwrap_or("disk");
    let content = Self::parse_disko_content(&config["content"])?;

    let disko_config = attrset! {
      "device" = nixstr(device);
      "type" = nixstr(disk_type);
      "content" = content;
    };

    let raw = format!("{{ disko.devices.disk.main = {disko_config}; }}");
    fmt_nix(raw)
  }

  fn parse_root_pass_hash(content: &Value) -> anyhow::Result<String> {
    let hash = content
      .as_str()
      .ok_or_else(|| anyhow::anyhow!("Root password hash must be a string"))?;
    Ok(attrset! {
      "users.users.root.hashedPassword" = nixstr(hash);
    })
  }

  fn parse_disko_content(content: &Value) -> anyhow::Result<String> {
    let content_type = content["type"].as_str().unwrap_or("gpt");
    let partitions = &content["partitions"];

    if let Some(partitions_obj) = partitions.as_object() {
      let mut partition_attrs = Vec::new();

      for (name, partition) in partitions_obj {
        let partition_config = Self::parse_partition(partition)?;
        partition_attrs.push(format!("{} = {};", nixstr(name), partition_config));
      }

      let partitions_attr = format!("{{ {} }}", partition_attrs.join(" "));

      Ok(attrset! {
        "type" = nixstr(content_type);
        "partitions" = partitions_attr;
      })
    } else {
      Ok(attrset! {
        "type" = nixstr(content_type);
      })
    }
  }

  fn parse_partition(partition: &Value) -> anyhow::Result<String> {
    let format = partition["format"]
      .as_str()
      .ok_or_else(|| anyhow::anyhow!("Missing required 'format' field in partition"))?;
    let mountpoint = partition["mountpoint"]
      .as_str()
      .ok_or_else(|| anyhow::anyhow!("Missing required 'mountpoint' field in partition"))?;
    let size = partition["size"]
      .as_str()
      .ok_or_else(|| anyhow::anyhow!("Missing required 'size' field in partition"))?;
    let part_type = partition.get("type").and_then(|v| v.as_str());
    log::debug!(
      "Parsing partition: format={format}, mountpoint={mountpoint}, size={size}, type={part_type:?}"
    );

    if let Some(part_type) = part_type {
      Ok(attrset! {
        type = nixstr(part_type);
        size = nixstr(size);
        content = attrset! {
          type = nixstr("filesystem");
          format = nixstr(format);
          mountpoint = nixstr(mountpoint);
        };
      })
    } else {
      Ok(attrset! {
        size = nixstr(size);
        content = attrset! {
          type = nixstr("filesystem");
          format = nixstr(format);
          mountpoint = nixstr(mountpoint);
        };
      })
    }
  }
  fn parse_timezone(value: &str) -> String {
    attrset! {
      "time.timeZone" = nixstr(value);
    }
  }
  pub fn parse_network_backend(value: &str) -> String {
    match value.to_lowercase().as_str() {
      "networkmanager" => attrset! {
        "networking.networkmanager.enable" = true;
      },
      "wpa_supplicant" => attrset! {
        "networking.wireless.enable" = true;
      },
      "systemd-networkd" => attrset! {
        "networking.useNetworkd" = true;
        "systemd.network.enable" = true;
      },
      _ => String::new(),
    }
  }
  pub fn parse_locale(value: &str) -> String {
    attrset! {
      "i18n.defaultLocale" = nixstr(value);
    }
  }
  fn parse_kb_layout(value: &str) -> String {
    let (xkb, console) = match value {
      "us(qwerty)" => ("us", "us"),
      "us(dvorak)" => ("us", "dvorak"),
      "us(colemak)" => ("us", "colemak"),
      "uk" => ("gb", "uk"),
      "de" => ("de", "de"),
      "fr" => ("fr", "fr"),
      "es" => ("es", "es"),
      "it" => ("it", "it"),
      "ru" => ("ru", "ru"),
      "cn" => ("cn", "us"),
      "jp" => ("jp", "us"),
      "kr" => ("kr", "us"),
      "in" => ("in", "us"),
      "br" => ("br", "br-abnt2"),
      "nl" => ("nl", "nl"),
      "se" => ("se", "us"),
      "no" => ("no", "no"),
      "fi" => ("fi", "fi"),
      "dk" => ("dk", "dk"),
      "pl" => ("pl", "pl"),
      "tr" => ("tr", "trq"),
      "gr" => ("gr", "gr"),
      _ => ("us", "us"),
    };

    attrset! {
      "services.xserver.xkb.layout" = nixstr(xkb);
      "console.keyMap" = nixstr(console);
    }
  }

  #[allow(clippy::ptr_arg)]
  fn parse_kernels(kernels: &Vec<Value>) -> String {
    if kernels.is_empty() {
      return String::from("{}");
    }

    // Take the first kernel as the primary one
    if let Some(Value::String(kernel)) = kernels.first() {
      let kernel_pkg = match kernel.to_lowercase().as_str() {
        "linux" => "pkgs.linuxPackages",
        "linux_zen" => "pkgs.linuxPackages_zen",
        "linux_hardened" => "pkgs.linuxPackages_hardened",
        "linux_lts" => "pkgs.linuxPackages_lts",
        _ => "pkgs.linuxPackages", // Default fallback
      };
      attrset! {
        "boot.kernelPackages" = kernel_pkg;
      }
    } else {
      String::from("{}")
    }
  }
  fn parse_hostname(value: &str) -> String {
    attrset! {
      "networking.hostName" = nixstr(value);
    }
  }
  fn _parse_greeter(value: &str, de: Option<&str>) -> String {
    match value.to_lowercase().as_str() {
      "sddm" => {
        if let Some(de) = de {
          match de {
            "hyprland" => attrset! {
              "services.displayManager.sddm" = attrset! {
                "wayland.enable" = true;
                "enable" = true;
              };
            },
            _ => attrset! {
              "services.displayManager.sddm.enable" = true;
            },
          }
        } else {
          attrset! {
            "services.displayManager.sddm.enable" = true;
          }
        }
      }
      "gdm" => attrset! {
        "services.xserver.displayManager.gdm.enable" = true;
      },
      "lightdm" => attrset! {
        "services.xserver.displayManager.lightdm.enable" = true;
      },
      _ => String::new(),
    }
  }
  fn parse_desktop_environment(value: &str) -> String {
    match value.to_lowercase().as_str() {
      "gnome" => attrset! {
        "services.xserver.enable" = true;
        "services.xserver.desktopManager.gnome.enable" = true;
      },
      "hyprland" => attrset! {
        "programs.hyprland.enable" = true;
      },
      "plasma" | "kde plasma" => attrset! {
        "services.xserver.enable" = true;
        "services.xserver.desktopManager.plasma5.enable" = true;
      },
      "xfce" => attrset! {
        "services.xserver.enable" = true;
        "services.xserver.desktopManager.xfce.enable" = true;
      },
      "cinnamon" => attrset! {
        "services.xserver.enable" = true;
        "services.xserver.desktopManager.cinnamon.enable" = true;
      },
      "mate" => attrset! {
        "services.xserver.enable" = true;
        "services.xserver.desktopManager.mate.enable" = true;
      },
      "lxqt" => attrset! {
        "services.xserver.enable" = true;
        "services.xserver.desktopManager.lxqt.enable" = true;
      },
      "budgie" => attrset! {
        "services.xserver.enable" = true;
        "services.xserver.desktopManager.budgie.enable" = true;
      },
      "i3" => attrset! {
        "services.xserver.enable" = true;
        "services.xserver.windowManager.i3.enable" = true;
      },
      _ => String::new(),
    }
  }
  fn parse_audio(value: &str) -> String {
    match value.to_lowercase().as_str() {
      "pulseaudio" => attrset! {
        "services.pulseaudio.enable" = true;
      },
      "pipewire" => attrset! {
        "services.pipewire.enable" = true;
      },
      _ => String::new(),
    }
  }
  fn parse_bootloader(value: &str) -> anyhow::Result<String> {
    let bootloader_attrs = match value.to_lowercase().as_str() {
      "systemd-boot" => attrset! {
        "systemd-boot.enable" = true;
        "efi.canTouchEfiVariables" = true;
      },

      "grub" => attrset! {
        grub = attrset! {
          device = nixstr("nodev");
          enable = true;
          efiSupport = true;
        };
        "efi.canTouchEfiVariables" = true;
      },
      _ => String::new(),
    };
    Ok(attrset! {
      "boot.loader" = bootloader_attrs;
    })
  }

  fn parse_users(&self, users: Vec<User>) -> anyhow::Result<String> {
    if users.is_empty() {
      return Ok(String::from("{}"));
    }

    let mut user_configs = Vec::new();
    let mut hm_configs = Vec::new();

    for user in users {
      let groups_list = if user.groups.is_empty() {
        "[]".to_string()
      } else {
        let group_strings: Vec<String> = user.groups.iter().map(nixstr).collect();
        format!("[ {} ]", group_strings.join(" "))
      };
      let user_config = attrset! {
        "isNormalUser" = "true";
        "extraGroups" = groups_list;
        "hashedPassword" = nixstr(user.password_hash);
      };
      user_configs.push(format!("\"{}\" = {};", user.username, user_config));

      if let Some(cfg) = user.home_manager_cfg {
        let pkg_list = if cfg.packages.is_empty() {
          "with pkgs; []".to_string()
        } else {
          let pkgs: Vec<String> = cfg.packages.iter().map(|s| s.to_string()).collect();
          format!("with pkgs; [ {} ]", pkgs.join(" "))
        };
        let hm_config_body = attrset! {
          home = attrset! {
            packages = pkg_list;
            stateVersion = nixstr("24.05");
          };
        };
        let hm_config_expr = format!("{{pkgs, ...}}: {hm_config_body}");
        let user_hm_config = format!("\"{}\" = {};", user.username, hm_config_expr);
        hm_configs.push(user_hm_config);
      }
    }

    let users = if !hm_configs.is_empty() {
      attrset! {
        "users.users" = format!("{{ {} }}", user_configs.join(" "));
        "home-manager.users" = format!("{{ {} }}", hm_configs.join(" "));
      }
    } else {
      attrset! {
        "users.users" = format!("{{ {} }}", user_configs.join(" "));
      }
    };

    log::debug!("Parsed users config: {users}");

    Ok(users)
  }

  #[allow(clippy::ptr_arg)]
  fn parse_system_packages(packages: &Vec<Value>) -> String {
    if packages.is_empty() {
      return String::from("{}");
    }

    let pkg_list: Vec<String> = packages
      .iter()
      .filter_map(&Value::as_str)
      .map(&str::to_string)
      .collect();

    if pkg_list.is_empty() {
      return String::from("{}");
    }

    let packages_attr = format!("with pkgs; [ {} ]", pkg_list.join(" "));
    attrset! {
      "environment.systemPackages" = packages_attr;
    }
  }

  fn parse_enable_flakes() -> String {
    attrset! {
      "nix.settings.experimental-features" = "[ \"nix-command\" \"flakes\" ]";
    }
  }

  fn parse_swap() -> String {
    attrset! {
      "swapDevices" = "[ { device = \"/swapfile\"; size = 4096; } ]";
    }
  }
}
