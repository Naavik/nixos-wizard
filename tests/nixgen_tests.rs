use nixos_wizard::nixgen::{NixWriter, nixstr};
use serde_json::json;

#[cfg(test)]
mod nix_generation_tests {
    use super::*;

    #[test]
    fn test_nixstr() {
        assert_eq!(nixstr("test"), r#""test""#);
        assert_eq!(nixstr("with spaces"), r#""with spaces""#);
        assert_eq!(nixstr(""), r#""""#);
    }

    #[test]
    fn test_parse_hostname() {
        let config = json!({
            "hostname": "test-host"
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        assert!(result.contains("networking.hostName = \"test-host\""));
        assert!(result.contains("imports = [ ./hardware-configuration.nix ]"));
    }

    #[test]
    fn test_parse_enable_flakes() {
        let config = json!({
            "enable_flakes": true
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        assert!(result.contains("nix.settings.experimental-features = [ \"nix-command\" \"flakes\" ]"));
    }

    #[test]
    fn test_parse_bootloader_systemd_boot() {
        let config = json!({
            "bootloader": "systemd-boot"
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        assert!(result.contains("boot.loader"));
        assert!(result.contains("systemd-boot.enable = true"));
        assert!(result.contains("efi.canTouchEfiVariables = true"));
    }

    #[test]
    fn test_parse_bootloader_grub() {
        let config = json!({
            "bootloader": "grub"
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        assert!(result.contains("boot.loader"));
        assert!(result.contains("grub"));
        assert!(result.contains("enable = true"));
        assert!(result.contains("efiSupport = true"));
        assert!(result.contains("device = nodev"));
    }

    #[test]
    fn test_parse_desktop_environment_gnome() {
        let config = json!({
            "desktop_environment": "gnome"
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        assert!(result.contains("services.xserver.desktopManager.gnome.enable = true"));
    }

    #[test]
    fn test_parse_desktop_environment_kde() {
        let config = json!({
            "desktop_environment": "kde plasma"
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        assert!(result.contains("services.xserver.desktopManager.plasma5.enable = true"));
    }

    #[test]
    fn test_parse_audio_backend() {
        let config = json!({
            "audio_backend": "pulseaudio"
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        assert!(result.contains("services.pulseaudio.enable = true"));
    }

    #[test]
    fn test_parse_network_backend_networkmanager() {
        let config = json!({
            "network_backend": "networkmanager"
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        assert!(result.contains("networking.networkmanager.enable = true"));
    }

    #[test]
    fn test_parse_system_packages() {
        let config = json!({
            "system_pkgs": ["vim", "git", "htop"]
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        assert!(result.contains("environment.systemPackages"));
        assert!(result.contains("with pkgs; [ vim git htop ]"));
    }

    #[test]
    fn test_parse_system_packages_empty() {
        let config = json!({
            "system_pkgs": []
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        // Should not contain environment.systemPackages when empty
        assert!(!result.contains("environment.systemPackages"));
    }

    #[test]
    fn test_parse_users() {
        let config = json!({
            "users": [
                {
                    "username": "testuser",
                    "password_hash": "hashed_password",
                    "groups": ["wheel", "audio"]
                }
            ]
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        assert!(result.contains("users.users"));
        assert!(result.contains("testuser"));
        assert!(result.contains("isNormalUser = true"));
        assert!(result.contains("extraGroups = [ \"wheel\" \"audio\" ]"));
        assert!(result.contains("hashedPassword = \"hashed_password\""));
    }

    #[test]
    fn test_parse_users_empty_groups() {
        let config = json!({
            "users": [
                {
                    "username": "testuser",
                    "password_hash": "hashed_password", 
                    "groups": []
                }
            ]
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        // The formatter might change how empty arrays are displayed
        assert!(result.contains("extraGroups") && (result.contains("[]") || result.contains("[ ]")));
    }

    #[test]
    fn test_parse_kernels() {
        let config = json!({
            "kernels": ["linux_zen"]
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        assert!(result.contains("boot.kernelPackages = pkgs.linuxPackages_zen"));
    }

    #[test]
    fn test_parse_swap_enabled() {
        let config = json!({
            "use_swap": true
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        // The formatter might change spacing and line breaks
        assert!(result.contains("swapDevices") && result.contains("/swapfile") && result.contains("4096"));
    }

    #[test]
    fn test_parse_locale_and_keyboard() {
        let config = json!({
            "locale": "en_US.UTF-8",
            "keyboard_layout": "us"
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        assert!(result.contains("i18n.defaultLocale = \"en_US.UTF-8\""));
        assert!(result.contains("services.xserver.xkb.layout = \"us\""));
    }

    #[test]
    fn test_parse_timezone() {
        let config = json!({
            "timezone": "America/New_York"
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        assert!(result.contains("time.timeZone = \"America/New_York\""));
    }

    #[test]
    fn test_comprehensive_config() {
        let config = json!({
            "hostname": "nixos-test",
            "enable_flakes": true,
            "bootloader": "systemd-boot",
            "desktop_environment": "gnome",
            "audio_backend": "pulseaudio",
            "network_backend": "networkmanager",
            "system_pkgs": ["vim", "git"],
            "users": [
                {
                    "username": "alice",
                    "password_hash": "alice_hash",
                    "groups": ["wheel"]
                }
            ],
            "use_swap": true,
            "locale": "en_US.UTF-8",
            "timezone": "America/Chicago"
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        // Check that all components are present
        assert!(result.contains("networking.hostName = \"nixos-test\""));
        assert!(result.contains("nix.settings.experimental-features"));
        assert!(result.contains("systemd-boot.enable = true"));
        assert!(result.contains("services.xserver.desktopManager.gnome.enable = true"));
        assert!(result.contains("services.pulseaudio.enable = true"));
        assert!(result.contains("networking.networkmanager.enable = true"));
        assert!(result.contains("with pkgs; [ vim git ]"));
        assert!(result.contains("\"alice\""));
        assert!(result.contains("swapDevices"));
        assert!(result.contains("i18n.defaultLocale = \"en_US.UTF-8\""));
        assert!(result.contains("time.timeZone = \"America/Chicago\""));
    }

    #[test]
    fn test_disko_config_simple() {
        let disko_config = json!({
            "device": "/dev/sda",
            "type": "disk",
            "content": {
                "type": "gpt",
                "partitions": {
                    "BOOT": {
                        "format": "vfat",
                        "mountpoint": "/boot",
                        "size": "512M",
                        "type": "EF00"
                    },
                    "ROOT": {
                        "format": "ext4",
                        "mountpoint": "/",
                        "size": "100%",
                        "type": "8300"
                    }
                }
            }
        });

        let writer = NixWriter::new(json!({"disko": disko_config}));
        let result = writer.write_disko_config(disko_config).unwrap();
        
        assert!(result.contains("disko.devices.disk.main"));
        assert!(result.contains("device = \"/dev/sda\""));
        assert!(result.contains("type = \"disk\""));
        assert!(result.contains("\"BOOT\""));
        assert!(result.contains("\"ROOT\""));
        assert!(result.contains("format = \"vfat\""));
        assert!(result.contains("mountpoint = \"/boot\""));
        assert!(result.contains("format = \"ext4\""));
        assert!(result.contains("mountpoint = \"/\""));
    }

    #[test]
    fn test_root_password_hash() {
        let config = json!({
            "root_passwd_hash": "root_hashed_password"
        });

        let writer = NixWriter::new(json!({"config": config}));
        let result = writer.write_sys_config(config).unwrap();
        
        assert!(result.contains("users.users.root.hashedPassword = \"root_hashed_password\""));
    }
}