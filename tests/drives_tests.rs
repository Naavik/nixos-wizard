use nixos_wizard::drives::*;

#[cfg(test)]
mod drive_utility_tests {
    use super::*;

    #[test]
    fn test_bytes_readable_bytes() {
        assert_eq!(bytes_readable(512), "512");
        assert_eq!(bytes_readable(1023), "1023");
    }

    #[test]
    fn test_bytes_readable_kib() {
        assert_eq!(bytes_readable(1024), "1.00 KiB");
        assert_eq!(bytes_readable(2048), "2.00 KiB");
        assert_eq!(bytes_readable(1536), "1.50 KiB");
    }

    #[test]
    fn test_bytes_readable_mib() {
        assert_eq!(bytes_readable(1024 * 1024), "1.00 MiB");
        assert_eq!(bytes_readable(1024 * 1024 * 2), "2.00 MiB");
        assert_eq!(bytes_readable(1024 * 1024 + 512 * 1024), "1.50 MiB");
    }

    #[test]
    fn test_bytes_readable_gib() {
        assert_eq!(bytes_readable(1024 * 1024 * 1024), "1.00 GiB");
        assert_eq!(bytes_readable(1024 * 1024 * 1024 * 4), "4.00 GiB");
    }

    #[test]
    fn test_bytes_readable_tib() {
        let tib = 1024u64.pow(4);
        assert_eq!(bytes_readable(tib), "1.00 TiB");
        assert_eq!(bytes_readable(tib * 2), "2.00 TiB");
    }

    #[test]
    fn test_bytes_disko_cfg_small() {
        let result = bytes_disko_cfg(512, 0, 512, 1000000);
        assert_eq!(result, "512B");
    }

    #[test]
    fn test_bytes_disko_cfg_kilobytes() {
        let result = bytes_disko_cfg(10000, 0, 512, 1000000);
        assert_eq!(result, "10K");
    }

    #[test]
    fn test_bytes_disko_cfg_megabytes() {
        let result = bytes_disko_cfg(100_000_000, 0, 512, 1000000000);
        assert_eq!(result, "100M");
    }

    #[test]
    fn test_bytes_disko_cfg_gigabytes() {
        let result = bytes_disko_cfg(10_000_000_000, 0, 512, 100_000_000_000);
        assert_eq!(result, "10G");
    }

    #[test]
    fn test_bytes_disko_cfg_terabytes() {
        let result = bytes_disko_cfg(2_000_000_000_000, 0, 512, 100_000_000_000_000);
        assert_eq!(result, "2T");
    }

    #[test]
    fn test_bytes_disko_cfg_rest_of_space() {
        // When requested + used is close to total, should return 100%
        let total_size = 1000000;
        let sector_size = 512;
        let used_sectors = 500000;
        let requested_bytes = (total_size - used_sectors - 1000) * sector_size; // Close to end
        
        let result = bytes_disko_cfg(requested_bytes, used_sectors, sector_size, total_size);
        assert_eq!(result, "100%");
    }

    #[test]
    fn test_parse_sectors_bytes() {
        assert_eq!(parse_sectors("1024B", 512, 1000), Some(2));
        assert_eq!(parse_sectors("512b", 512, 1000), Some(1));
    }

    #[test]
    fn test_parse_sectors_kilobytes() {
        assert_eq!(parse_sectors("1KB", 512, 1000), Some(2)); // 1000 bytes / 512 = ~2 sectors (rounded)
        assert_eq!(parse_sectors("1KiB", 512, 1000), Some(2)); // 1024 bytes / 512 = 2 sectors
        assert_eq!(parse_sectors("2kb", 512, 1000), Some(4)); // 2000 bytes / 512 = ~4 sectors (rounded)
    }

    #[test]
    fn test_parse_sectors_megabytes() {
        // 1MB = 1,000,000 bytes; 1,000,000 / 512 = 1953.125, rounded = 1953
        assert_eq!(parse_sectors("1MB", 512, 1000000), Some(1953)); 
        assert_eq!(parse_sectors("1MiB", 512, 1000000), Some((1024 * 1024) / 512));
        assert_eq!(parse_sectors("5mb", 512, 1000000), Some(9766)); // 5000000 bytes / 512 = ~9766 sectors (rounded)
    }

    #[test]
    fn test_parse_sectors_gigabytes() {
        assert_eq!(parse_sectors("1GB", 512, 10000000), Some(1000000000 / 512));
        assert_eq!(parse_sectors("1GiB", 512, 10000000), Some((1024 * 1024 * 1024) / 512));
    }

    #[test]
    fn test_parse_sectors_terabytes() {
        assert_eq!(parse_sectors("1TB", 512, 100000000000), Some(1000000000000 / 512));
        assert_eq!(parse_sectors("1TiB", 512, 100000000000), Some((1024u64.pow(4)) / 512));
    }

    #[test]
    fn test_parse_sectors_percentage() {
        assert_eq!(parse_sectors("50%", 512, 1000), Some(500));
        assert_eq!(parse_sectors("25%", 512, 2000), Some(500));
        assert_eq!(parse_sectors("100%", 512, 1000), Some(1000));
    }

    #[test]
    fn test_parse_sectors_raw_number() {
        assert_eq!(parse_sectors("100", 512, 1000), Some(100));
        assert_eq!(parse_sectors("500", 512, 1000), Some(500));
    }

    #[test]
    fn test_parse_sectors_invalid() {
        assert_eq!(parse_sectors("invalid", 512, 1000), None);
        assert_eq!(parse_sectors("", 512, 1000), None);
        assert_eq!(parse_sectors("1XB", 512, 1000), None);
    }

    #[test]
    fn test_parse_sectors_case_insensitive() {
        assert_eq!(parse_sectors("1gb", 512, 10000000), Some(1000000000 / 512));
        assert_eq!(parse_sectors("1GB", 512, 10000000), Some(1000000000 / 512));
        assert_eq!(parse_sectors("1Gb", 512, 10000000), Some(1000000000 / 512));
    }

    #[test]
    fn test_parse_sectors_with_whitespace() {
        assert_eq!(parse_sectors(" 1GB ", 512, 10000000), Some(1000000000 / 512));
        assert_eq!(parse_sectors("\t50%\t", 512, 1000), Some(500));
    }

    #[test]
    fn test_mb_to_sectors() {
        assert_eq!(mb_to_sectors(1, 512), (1024 * 1024) / 512);
        assert_eq!(mb_to_sectors(10, 512), (10 * 1024 * 1024) / 512);
        assert_eq!(mb_to_sectors(0, 512), 0);
    }

    #[test]
    fn test_mb_to_sectors_different_sector_sizes() {
        // For 4K sectors
        assert_eq!(mb_to_sectors(1, 4096), (1024 * 1024) / 4096);
        assert_eq!(mb_to_sectors(4, 4096), (4 * 1024 * 1024) / 4096);
    }

    #[test]
    fn test_mb_to_sectors_rounding() {
        // Test that we round up properly
        let mb = 1;
        let sector_size = 513; // Odd sector size to force rounding
        let expected_sectors = (1024 * 1024_u64).div_ceil(513);
        assert_eq!(mb_to_sectors(mb, sector_size), expected_sectors);
    }

    #[test]
    fn test_get_entry_id_unique() {
        let id1 = get_entry_id();
        let id2 = get_entry_id();
        let id3 = get_entry_id();
        
        // Each ID should be unique and incrementing
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
        assert!(id2 > id1);
        assert!(id3 > id2);
    }

    #[test]
    fn test_formatting_edge_cases() {
        // Test boundary conditions for formatting
        assert_eq!(bytes_readable(1023), "1023");
        assert_eq!(bytes_readable(1024), "1.00 KiB");
        
        let mib_boundary = 1024 * 1024;
        assert_eq!(bytes_readable(mib_boundary - 1), format!("{:.2} KiB", (mib_boundary - 1) as f64 / 1024.0));
        assert_eq!(bytes_readable(mib_boundary), "1.00 MiB");
        
        let gib_boundary = 1024 * 1024 * 1024;
        assert_eq!(bytes_readable(gib_boundary - 1), format!("{:.2} MiB", (gib_boundary - 1) as f64 / (1024.0 * 1024.0)));
        assert_eq!(bytes_readable(gib_boundary), "1.00 GiB");
    }

    #[test]
    fn test_disko_config_edge_cases() {
        // Test when exactly at the boundary
        let total_size = 1000000;
        let sector_size = 512;
        let used_sectors = 500000;
        let requested_bytes = (total_size - used_sectors - 2048) * sector_size; // Exactly at boundary
        
        let result = bytes_disko_cfg(requested_bytes, used_sectors, sector_size, total_size);
        assert_eq!(result, "100%");
    }

    #[test]
    fn test_parse_sectors_fractional_percentage() {
        assert_eq!(parse_sectors("33.33%", 512, 3000), Some(1000)); // 33.33% of 3000 ≈ 1000 (rounded)
        assert_eq!(parse_sectors("66.67%", 512, 3000), Some(2000)); // 66.67% of 3000 ≈ 2000
    }

    #[test]
    fn test_parse_sectors_fractional_units() {
        // Test fractional values with units
        // 1.5MB = 1,500,000 bytes; 1,500,000 / 512 = 2929.6875, rounded = 2930
        assert_eq!(parse_sectors("1.5MB", 512, 10000000), Some(2930)); 
        // 0.5GB = 500,000,000 bytes; 500,000,000 / 512 = 976562.5, rounded = 976563  
        assert_eq!(parse_sectors("0.5GB", 512, 10000000), Some(976563));
    }
}