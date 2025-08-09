use nixos_wizard::{Installer, User, attrset, merge_attrs, list};
use serde_json::json;

#[cfg(test)]
mod macro_tests {
    use super::*;

    #[test]
    fn test_attrset_macro() {
        let result = attrset! {
            "foo" = "bar";
            "baz" = "42";
        };
        assert_eq!(result, r#"{ foo = bar; baz = 42; }"#);
    }

    #[test]
    fn test_attrset_macro_single_item() {
        let result = attrset! {
            "single" = "value";
        };
        assert_eq!(result, r#"{ single = value; }"#);
    }

    #[test]
    fn test_merge_attrs_macro() {
        let set1 = "{ foo = bar; }";
        let set2 = "{ baz = qux; }";
        let result = merge_attrs!(set1, set2);
        assert_eq!(result, "{ foo = bar;baz = qux; }");
    }

    #[test]
    fn test_merge_attrs_macro_single() {
        let set1 = "{ foo = bar; }";
        let result = merge_attrs!(set1);
        assert_eq!(result, "{ foo = bar; }");
    }

    #[test]
    fn test_list_macro() {
        let result = list!["foo", "bar", "baz"];
        assert_eq!(result, "[foo bar baz]");
    }

    #[test]
    fn test_list_macro_empty() {
        let empty: Vec<&str> = vec![];
        let result = format!("[{}]", empty.join(" "));
        assert_eq!(result, "[]");
    }

    #[test]
    fn test_list_macro_single() {
        let result = list!["single"];
        assert_eq!(result, "[single]");
    }

    #[test]
    #[should_panic(expected = "attrset must be a valid attribute set")]
    fn test_merge_attrs_invalid_input() {
        let invalid_set = "not an attribute set";
        merge_attrs!(invalid_set);
    }
}

#[cfg(test)]
mod installer_tests {
    use super::*;

    #[test]
    fn test_installer_default() {
        let installer = Installer::default();
        assert_eq!(installer.hostname, None);
        assert_eq!(installer.users.len(), 0);
        assert_eq!(installer.enable_flakes, false);
        assert_eq!(installer.use_swap, false);
    }

    #[test]
    fn test_installer_serialization() {
        let mut installer = Installer::default();
        installer.hostname = Some("test-host".to_string());
        installer.enable_flakes = true;
        installer.users.push(User {
            username: "testuser".to_string(),
            password_hash: "hashed_password".to_string(),
            groups: vec!["wheel".to_string(), "audio".to_string()],
        });

        let json = installer.to_json().unwrap();
        assert_eq!(json["config"]["hostname"], "test-host");
        assert_eq!(json["config"]["enable_flakes"], true);
        assert_eq!(json["config"]["users"][0]["username"], "testuser");
        assert_eq!(json["config"]["users"][0]["groups"][0], "wheel");
    }

    #[test]
    fn test_installer_deserialization() {
        let json_data = json!({
            "hostname": "test-host",
            "enable_flakes": true,
            "use_swap": false,
            "use_auto_drive_config": false,
            "drives": [],
            "system_pkgs": [],
            "users": [
                {
                    "username": "testuser",
                    "password_hash": "hashed_password",
                    "groups": ["wheel", "audio"]
                }
            ]
        });

        let installer: Installer = serde_json::from_value(json_data).unwrap();
        assert_eq!(installer.hostname.as_ref().unwrap(), "test-host");
        assert_eq!(installer.enable_flakes, true);
        assert_eq!(installer.users.len(), 1);
        assert_eq!(installer.users[0].username, "testuser");
        assert_eq!(installer.users[0].groups.len(), 2);
    }
}

#[cfg(test)]
mod user_tests {
    use super::*;

    #[test]
    fn test_user_creation() {
        let user = User {
            username: "testuser".to_string(),
            password_hash: "hashed_password".to_string(),
            groups: vec!["wheel".to_string(), "audio".to_string()],
        };

        assert_eq!(user.username, "testuser");
        assert_eq!(user.password_hash, "hashed_password");
        assert_eq!(user.groups, vec!["wheel", "audio"]);
    }

    #[test]
    fn test_user_as_table_row() {
        let user = User {
            username: "testuser".to_string(),
            password_hash: "hashed_password".to_string(),
            groups: vec!["wheel".to_string(), "audio".to_string()],
        };

        let row = user.as_table_row();
        assert_eq!(row, vec!["testuser", "wheel, audio"]);
    }

    #[test]
    fn test_user_serialization() {
        let user = User {
            username: "testuser".to_string(),
            password_hash: "hashed_password".to_string(),
            groups: vec!["wheel".to_string()],
        };

        let json = serde_json::to_value(&user).unwrap();
        assert_eq!(json["username"], "testuser");
        assert_eq!(json["password_hash"], "hashed_password");
        assert_eq!(json["groups"][0], "wheel");
    }

    #[test]
    fn test_user_deserialization() {
        let json_data = json!({
            "username": "testuser",
            "password_hash": "hashed_password",
            "groups": ["wheel", "audio"]
        });

        let user: User = serde_json::from_value(json_data).unwrap();
        assert_eq!(user.username, "testuser");
        assert_eq!(user.password_hash, "hashed_password");
        assert_eq!(user.groups, vec!["wheel", "audio"]);
    }
}