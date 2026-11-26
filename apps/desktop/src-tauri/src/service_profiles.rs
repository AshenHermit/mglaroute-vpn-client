use rusqlite::{Connection, Error, Result};

use crate::storage::models::ServiceProfile;
use crate::storage::repos::service_profile_repo;

/// Seed profiles mirror the server's `OnlineServicesService.seedDefaults`.
/// `is_default` only marks a seed; it never enables or protects a profile.
const DEFAULT_PROFILES: [(&str, &str); 3] = [
    ("YouTube", "https://www.youtube.com"),
    ("Discord", "https://discord.com"),
    ("Google", "https://www.google.com"),
];

fn json_error(error: serde_json::Error) -> Error {
    Error::ToSqlConversionFailure(Box::new(error))
}

/// Insert each default profile only when no row with the same name exists.
pub fn seed_defaults(conn: &mut Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    for (name, url) in DEFAULT_PROFILES {
        if service_profile_repo::exists_by_name(&tx, name)? {
            continue;
        }
        let value = ServiceProfile {
            id: format!("default-{}", name.to_lowercase()),
            name: name.to_string(),
            test_urls: serde_json::to_string(&[url]).map_err(json_error)?,
            is_enabled: true,
            is_default: true,
            ..Default::default()
        };
        service_profile_repo::insert(&tx, &value)?;
    }
    tx.commit()
}

/// Seed only when the user has no service profiles at all (fresh install).
pub fn ensure_defaults_on_empty(conn: &mut Connection) -> Result<()> {
    let is_empty = service_profile_repo::list(conn)?.is_empty();
    if is_empty {
        seed_defaults(conn)?;
    }
    Ok(())
}

/// Test URLs must be a non-empty list of parseable HTTPS URLs.
pub fn validate_test_urls(urls: &[String]) -> bool {
    !urls.is_empty()
        && urls
            .iter()
            .all(|url| url.starts_with("https://") && reqwest::Url::parse(url).is_ok())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{ensure_defaults_on_empty, seed_defaults, validate_test_urls};
    use crate::storage::migrate;
    use crate::storage::models::ServiceProfile;
    use crate::storage::repos::service_profile_repo;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        migrate(&conn).expect("migrate");
        conn
    }

    #[test]
    fn seed_defaults_idempotent() {
        let mut conn = test_conn();
        seed_defaults(&mut conn).expect("seed once");
        seed_defaults(&mut conn).expect("seed twice");

        let profiles = service_profile_repo::list(&conn).expect("list profiles");
        assert_eq!(profiles.len(), 3);
        let expected = [
            ("YouTube", "[\"https://www.youtube.com\"]"),
            ("Discord", "[\"https://discord.com\"]"),
            ("Google", "[\"https://www.google.com\"]"),
        ];
        for (name, urls) in expected {
            let profile = profiles
                .iter()
                .find(|profile| profile.name == name)
                .unwrap_or_else(|| panic!("missing default {name}"));
            assert_eq!(profile.test_urls, urls);
            assert!(profile.is_enabled);
            assert!(profile.is_default);
        }

        ensure_defaults_on_empty(&mut conn).expect("ensure on non-empty");
        assert_eq!(
            service_profile_repo::list(&conn)
                .expect("list after non-empty ensure")
                .len(),
            3
        );

        let mut empty_conn = test_conn();
        ensure_defaults_on_empty(&mut empty_conn).expect("ensure on empty");
        assert_eq!(
            service_profile_repo::list(&empty_conn)
                .expect("list after empty ensure")
                .len(),
            3
        );
    }

    #[test]
    fn seed_does_not_overwrite_existing() {
        let mut conn = test_conn();
        let custom_urls = "[\"https://custom.example.com\"]".to_string();
        service_profile_repo::insert(
            &conn,
            &ServiceProfile {
                id: "custom-youtube".to_string(),
                name: "YouTube".to_string(),
                test_urls: custom_urls.clone(),
                is_enabled: false,
                is_default: false,
                ..Default::default()
            },
        )
        .expect("pre-insert YouTube");

        seed_defaults(&mut conn).expect("seed");

        let profiles = service_profile_repo::list(&conn).expect("list profiles");
        assert_eq!(profiles.len(), 3);
        let youtube = profiles
            .iter()
            .find(|profile| profile.name == "YouTube")
            .expect("YouTube present");
        assert_eq!(youtube.test_urls, custom_urls);
        assert!(!youtube.is_enabled);
        assert!(!youtube.is_default);
    }

    #[test]
    fn validate_test_urls_rejects_invalid() {
        assert!(!validate_test_urls(&[]));
        assert!(!validate_test_urls(&["http://example.com".to_string()]));
        assert!(!validate_test_urls(&["not a url".to_string()]));
        assert!(!validate_test_urls(&[
            "https://www.youtube.com".to_string(),
            "ftp://example.com".to_string(),
        ]));
        assert!(validate_test_urls(&["https://www.youtube.com".to_string()]));
        assert!(validate_test_urls(&[
            "https://www.youtube.com".to_string(),
            "https://discord.com".to_string(),
        ]));
    }
}
