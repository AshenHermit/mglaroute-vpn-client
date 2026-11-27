//! Update payload shape tests. The Rust payloads must match the shared
//! `UpdateInfoSchema` / `UpdateProgressEventSchema` contracts exactly
//! (camelCase keys, lowercase phase), so the frontend can parse them as-is.

use mglaroute_client_lib::update::{
    release_url, validate_release_url, UpdateInfo, UpdatePhase, UpdateProgressEvent,
};
use serde_json::json;

const TAG_PREFIX: &str =
    "https://github.com/AshenHermit/mglaroute-vpn-client/releases/tag/v";

#[test]
fn release_url_uses_v_tag() {
    assert_eq!(release_url("0.2.0"), format!("{TAG_PREFIX}0.2.0"));
}

#[test]
fn update_info_serializes_camel_case() {
    let info = UpdateInfo {
        version: "0.2.0".to_string(),
        current_version: "0.1.0".to_string(),
        notes: Some("release notes".to_string()),
        pub_date: Some("2026-11-26T00:00:00Z".to_string()),
        can_install_in_app: true,
        release_url: release_url("0.2.0"),
    };

    let value = serde_json::to_value(&info).expect("serialize UpdateInfo");
    assert_eq!(value["version"], json!("0.2.0"));
    assert_eq!(value["currentVersion"], json!("0.1.0"));
    assert_eq!(value["notes"], json!("release notes"));
    assert_eq!(value["pubDate"], json!("2026-11-26T00:00:00Z"));
    assert_eq!(value["canInstallInApp"], json!(true));
    assert_eq!(value["releaseUrl"], json!(format!("{TAG_PREFIX}0.2.0")));

    // No snake_case duplicates may leak to the frontend.
    for snake in [
        "current_version",
        "pub_date",
        "can_install_in_app",
        "release_url",
    ] {
        assert!(value.get(snake).is_none(), "unexpected key {snake}");
    }
}

#[test]
fn update_progress_serializes_phase_and_counts() {
    let event = UpdateProgressEvent {
        phase: UpdatePhase::Downloading,
        received: 12,
        total: Some(100),
    };
    let value = serde_json::to_value(&event).expect("serialize UpdateProgressEvent");
    assert_eq!(value["phase"], json!("downloading"));
    assert_eq!(value["received"], json!(12));
    assert_eq!(value["total"], json!(100));

    let installing = serde_json::to_value(UpdateProgressEvent {
        phase: UpdatePhase::Installing,
        received: 100,
        total: Some(100),
    })
    .expect("serialize installing");
    assert_eq!(installing["phase"], json!("installing"));

    let done = serde_json::to_value(UpdateProgressEvent {
        phase: UpdatePhase::Done,
        received: 100,
        total: None,
    })
    .expect("serialize done");
    assert_eq!(done["phase"], json!("done"));
    assert_eq!(done["total"], json!(null));
}

#[test]
fn validate_release_url_accepts_only_release_tags() {
    assert!(validate_release_url(&release_url("0.2.0")));
    assert!(!validate_release_url(
        "https://evil.example/mglaroute-vpn-client/releases/tag/v0.2.0"
    ));
    assert!(!validate_release_url("file:///C:/malicious/installer.exe"));
    assert!(!validate_release_url(TAG_PREFIX.trim_end_matches('v')));
}
