use serde_json::Value;

const ENDPOINT: &str = "https://github.com/AshenHermit/mglaroute-vpn-client/releases/latest/download/latest.json";

fn config() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.conf.json");
    let text = std::fs::read_to_string(path).expect("src-tauri/tauri.conf.json must exist");
    serde_json::from_str(&text).expect("valid tauri.conf.json")
}

fn capabilities() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/capabilities/default.json");
    let text =
        std::fs::read_to_string(path).expect("src-tauri/capabilities/default.json must exist");
    serde_json::from_str(&text).expect("valid capability JSON")
}

#[test]
fn bundle_creates_updater_artifacts() {
    assert_eq!(
        config().pointer("/bundle/createUpdaterArtifacts"),
        Some(&Value::Bool(true)),
        "signed updater artifacts must be produced by the bundler"
    );
}

#[test]
fn updater_endpoints_include_latest_json() {
    let cfg = config();
    let endpoints = cfg
        .pointer("/plugins/updater/endpoints")
        .and_then(Value::as_array)
        .expect("plugins.updater.endpoints must be an array");
    assert!(
        endpoints
            .iter()
            .any(|endpoint| endpoint.as_str() == Some(ENDPOINT)),
        "updater needs the GitHub Releases latest.json endpoint: {endpoints:?}"
    );
}

#[test]
fn updater_pubkey_is_inline_and_non_empty() {
    assert!(
        config()
            .pointer("/plugins/updater/pubkey")
            .and_then(Value::as_str)
            .is_some_and(|pubkey| !pubkey.trim().is_empty()),
        "plugins.updater.pubkey must be a non-empty inline public key"
    );
}

#[test]
fn windows_install_mode_is_passive() {
    assert_eq!(
        config()
            .pointer("/plugins/updater/windows/installMode")
            .and_then(Value::as_str),
        Some("passive"),
        "in-app install must not show the NSIS wizard"
    );
}

#[test]
fn capabilities_grant_updater_and_shell_open() {
    let caps = capabilities();
    let permissions = caps
        .get("permissions")
        .and_then(Value::as_array)
        .expect("capability permissions must be an array");
    for required in ["updater:default", "shell:allow-open"] {
        assert!(
            permissions
                .iter()
                .any(|permission| permission.as_str() == Some(required)),
            "capability must grant {required}: {permissions:?}"
        );
    }
}
