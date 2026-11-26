use serde_json::Value;

fn config() -> Value {
    serde_json::from_str(include_str!("../tauri.conf.json")).expect("valid tauri.conf.json")
}

#[test]
fn main_window_label_is_explicit() {
    assert_eq!(
        config().pointer("/app/windows/0/label").and_then(Value::as_str),
        Some("main")
    );
}

#[test]
fn bundle_icon_supplies_tray_icon() {
    let cfg = config();
    let icons = cfg
        .pointer("/bundle/icon")
        .and_then(Value::as_array)
        .expect("bundle.icon must be an array");
    assert!(
        icons
            .iter()
            .any(|icon| icon.as_str() == Some("icons/icon.ico")),
        "tray icon needs a bundled icon source; bundle.icon = {icons:?}"
    );
}

#[test]
fn tauri_dependency_enables_tray_icon_feature() {
    let manifest = include_str!("../Cargo.toml");
    let tauri_line = manifest
        .lines()
        .find(|line| line.trim_start().starts_with("tauri = "))
        .expect("tauri dependency line");
    assert!(
        tauri_line.contains("tray-icon"),
        "tray APIs are gated behind the tray-icon feature: {tauri_line}"
    );
}
