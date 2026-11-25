//! Xray JSON config assembly. No process launch, no filesystem writes here:
//! callers persist the returned value next to the [`ConfigSnapshot`] path.

use serde_json::{json, Map, Value};

use crate::{storage::models::Profile, subscription::decode::decode_base64};

use super::snapshot::ConfigSnapshot;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid raw_config JSON: {0}")]
    InvalidRawConfig(#[from] serde_json::Error),
    #[error("unsupported protocol: {0}")]
    UnsupportedProtocol(String),
    #[error("missing {field} credential in link for protocol {protocol}")]
    MissingCredential {
        protocol: &'static str,
        field: &'static str,
    },
}

pub type Result<T> = std::result::Result<T, ConfigError>;

fn raw_object(profile: &Profile) -> Result<Map<String, Value>> {
    Ok(serde_json::from_str::<Map<String, Value>>(
        &profile.raw_config,
    )?)
}

fn string_param(raw: &Map<String, Value>, key: &str) -> Option<String> {
    raw.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

fn bool_param(raw: &Map<String, Value>, key: &str) -> Option<bool> {
    match raw.get(key)? {
        Value::Bool(value) => Some(*value),
        Value::String(value) => match value.as_str() {
            "1" | "true" | "True" => Some(true),
            "0" | "false" | "False" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn set_string(map: &mut Map<String, Value>, key: &str, value: String) {
    map.insert(key.to_string(), Value::String(value));
}

/// Prefer the stored protocol; fall back to the link scheme for older rows.
fn protocol_of(profile: &Profile) -> String {
    if let Some(protocol) = profile
        .protocol
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        return protocol.to_string();
    }
    for (prefix, name) in [
        ("vmess://", "vmess"),
        ("vless://", "vless"),
        ("trojan://", "trojan"),
        ("ss://", "ss"),
    ] {
        if profile.link.starts_with(prefix) {
            return name.to_string();
        }
    }
    String::new()
}

/// vless UUID / trojan password: the userinfo segment of the URI authority.
fn link_userinfo(profile: &Profile) -> Option<String> {
    let after_scheme = profile.link.split_once("://")?.1;
    let authority = after_scheme.split(['?', '#']).next()?;
    let (userinfo, _) = authority.rsplit_once('@')?;
    (!userinfo.is_empty()).then(|| userinfo.to_string())
}

/// ss credentials live only in the link: SIP002 encodes `method:password` in
/// the userinfo; legacy encodes `method:password@host:port` in the authority.
fn ss_credentials(profile: &Profile) -> Result<(String, String)> {
    let missing = || ConfigError::MissingCredential {
        protocol: "ss",
        field: "userinfo",
    };
    let rest = profile.link.strip_prefix("ss://").ok_or_else(missing)?;
    let head = rest.split(['?', '#']).next().unwrap_or(rest);
    let encoded = head.rsplit_once('@').map_or(head, |(userinfo, _)| userinfo);
    let decoded = decode_base64(encoded).ok_or_else(missing)?;
    let pair = decoded
        .rsplit_once('@')
        .map_or(decoded.as_str(), |(userinfo, _)| userinfo);
    let (method, password) = pair.split_once(':').ok_or_else(missing)?;
    if method.is_empty() || password.is_empty() {
        return Err(missing());
    }
    Ok((method.to_string(), password.to_string()))
}

fn vmess_link_object(profile: &Profile) -> Option<Map<String, Value>> {
    let encoded = profile.link.strip_prefix("vmess://")?;
    let json = decode_base64(encoded)?;
    serde_json::from_str::<Map<String, Value>>(&json).ok()
}

/// Legacy vmess keeps its credential in the base64 JSON link. `raw_config`
/// normally mirrors it; decoding the link makes older rows work too.
fn vmess_param(profile: &Profile, raw: &Map<String, Value>, key: &str) -> Option<String> {
    string_param(raw, key).or_else(|| {
        let link = vmess_link_object(profile)?;
        string_param(&link, key)
    })
}

fn outbound_settings(profile: &Profile, protocol: &str, raw: &Map<String, Value>) -> Result<Value> {
    let address = profile.address.clone().unwrap_or_default();
    let port = profile
        .port
        .and_then(|value| u16::try_from(value).ok())
        .unwrap_or(0);
    match protocol {
        "vmess" => {
            let id = vmess_param(profile, raw, "id").ok_or(ConfigError::MissingCredential {
                protocol: "vmess",
                field: "id",
            })?;
            let alter_id = vmess_param(profile, raw, "aid")
                .and_then(|value| value.parse::<u16>().ok())
                .unwrap_or(0);
            let security = vmess_param(profile, raw, "scy").unwrap_or_else(|| "auto".to_string());
            Ok(json!({
                "vnext": [{
                    "address": address,
                    "port": port,
                    "users": [{
                        "id": id,
                        "alterId": alter_id,
                        "security": security,
                        "level": 0,
                    }],
                }],
            }))
        }
        "vless" => {
            let id = link_userinfo(profile).ok_or(ConfigError::MissingCredential {
                protocol: "vless",
                field: "uuid",
            })?;
            let flow = string_param(raw, "flow").unwrap_or_default();
            let encryption = string_param(raw, "encryption").unwrap_or_else(|| "none".to_string());
            Ok(json!({
                "vnext": [{
                    "address": address,
                    "port": port,
                    "users": [{
                        "id": id,
                        "flow": flow,
                        "encryption": encryption,
                        "level": 0,
                    }],
                }],
            }))
        }
        "trojan" => {
            let password = link_userinfo(profile).ok_or(ConfigError::MissingCredential {
                protocol: "trojan",
                field: "password",
            })?;
            Ok(json!({
                "servers": [{
                    "address": address,
                    "port": port,
                    "password": password,
                    "level": 0,
                }],
            }))
        }
        "ss" => {
            let (method, password) = ss_credentials(profile)?;
            Ok(json!({
                "servers": [{
                    "address": address,
                    "port": port,
                    "method": method,
                    "password": password,
                    "level": 0,
                }],
            }))
        }
        other => Err(ConfigError::UnsupportedProtocol(other.to_string())),
    }
}

/// Translate decoded `raw_config` fields into xray `streamSettings`.
fn stream_settings(profile: &Profile, protocol: &str, raw: &Map<String, Value>) -> Value {
    let mut stream = Map::new();
    let is_vmess = protocol == "vmess";
    let get = |key: &str| {
        if is_vmess {
            vmess_param(profile, raw, key)
        } else {
            string_param(raw, key)
        }
    };

    let network = get(if is_vmess { "net" } else { "type" }).unwrap_or_else(|| "raw".to_string());
    set_string(&mut stream, "network", network.clone());

    let requested = if is_vmess {
        get("tls").or_else(|| get("security"))
    } else {
        get("security")
    }
    .unwrap_or_default();
    let security = match requested.as_str() {
        "" | "none" | "0" => "none",
        value => value,
    };
    set_string(&mut stream, "security", security.to_string());

    let sni = get("sni");
    let fp = get("fp");
    let mut tls = Map::new();
    if let Some(value) = sni.clone() {
        set_string(&mut tls, "serverName", value);
    }
    if let Some(value) = fp.clone() {
        set_string(&mut tls, "fingerprint", value);
    }
    if let Some(value) = get("alpn") {
        let alpn: Vec<String> = value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect();
        tls.insert("alpn".to_string(), json!(alpn));
    }
    if bool_param(raw, "allowInsecure").or_else(|| bool_param(raw, "skip-cert-verify"))
        == Some(true)
    {
        tls.insert("allowInsecure".to_string(), Value::Bool(true));
    }
    if !tls.is_empty() {
        stream.insert("tlsSettings".to_string(), Value::Object(tls));
    }

    if security == "reality" {
        let mut reality = Map::new();
        if let Some(value) = sni {
            set_string(&mut reality, "serverName", value);
        }
        if let Some(value) = fp {
            set_string(&mut reality, "fingerprint", value);
        }
        for (source, target) in [("pbk", "publicKey"), ("sid", "shortId"), ("spx", "spiderX")] {
            if let Some(value) = string_param(raw, source) {
                set_string(&mut reality, target, value);
            }
        }
        stream.insert("realitySettings".to_string(), Value::Object(reality));
    }

    let path = get("path");
    let host = get("host");
    match network.as_str() {
        "ws" => {
            let mut ws = Map::new();
            if let Some(value) = path {
                set_string(&mut ws, "path", value);
            }
            if let Some(value) = host {
                let mut headers = Map::new();
                set_string(&mut headers, "Host", value);
                ws.insert("headers".to_string(), Value::Object(headers));
            }
            stream.insert("wsSettings".to_string(), Value::Object(ws));
        }
        "grpc" => {
            let mut grpc = Map::new();
            if let Some(value) = string_param(raw, "serviceName") {
                set_string(&mut grpc, "serviceName", value);
            }
            if get("mode").as_deref() == Some("multi") {
                grpc.insert("multiMode".to_string(), Value::Bool(true));
            }
            stream.insert("grpcSettings".to_string(), Value::Object(grpc));
        }
        "http" | "h2" => {
            let mut http = Map::new();
            if let Some(value) = path {
                set_string(&mut http, "path", value);
            }
            if let Some(value) = host {
                http.insert("host".to_string(), json!([value]));
            }
            stream.insert("httpSettings".to_string(), Value::Object(http));
        }
        _ => {
            if let Some(value) = get("headerType") {
                let mut header = Map::new();
                set_string(&mut header, "type", value);
                let mut raw_settings = Map::new();
                raw_settings.insert("header".to_string(), Value::Object(header));
                stream.insert("rawSettings".to_string(), Value::Object(raw_settings));
            }
        }
    }
    Value::Object(stream)
}

fn build_outbound(profile: &Profile, tag: &str) -> Result<Value> {
    let protocol = protocol_of(profile);
    let raw = raw_object(profile)?;
    Ok(json!({
        "tag": tag,
        "protocol": protocol,
        "settings": outbound_settings(profile, &protocol, &raw)?,
        "streamSettings": stream_settings(profile, &protocol, &raw),
        "mux": {"enabled": false},
    }))
}

fn mixed_inbound(tag: &str, port: u16) -> Value {
    json!({
        "tag": tag,
        "listen": "127.0.0.1",
        "port": port,
        "protocol": "mixed",
        "noauth": true,
        "settings": {"auth": "noauth", "noauth": true, "udp": true},
        "sniffing": {"enabled": false},
    })
}

fn routing(rules: Vec<Value>) -> Value {
    json!({"domainStrategy": "AsIs", "rules": rules})
}

/// One config per speed-test batch: `mixed` inbound on loopback per port,
/// one outbound per profile, one routing rule tying the two together.
pub fn build_speedtest_config(profiles: &[Profile], ports: &[u16]) -> Result<Value> {
    let mut inbounds = Vec::with_capacity(ports.len());
    let mut outbounds = Vec::with_capacity(ports.len());
    let mut rules = Vec::with_capacity(ports.len());
    for (index, (profile, port)) in profiles.iter().zip(ports.iter()).enumerate() {
        let in_tag = format!("probe-in-{index}");
        let out_tag = format!("probe-out-{index}");
        inbounds.push(mixed_inbound(&in_tag, *port));
        outbounds.push(build_outbound(profile, &out_tag)?);
        rules.push(json!({
            "type": "field",
            "inboundTag": [in_tag],
            "outboundTag": out_tag,
        }));
    }
    Ok(json!({
        "log": {"loglevel": "warning"},
        "inbounds": inbounds,
        "outbounds": outbounds,
        "routing": routing(rules),
    }))
}

/// One config for a live connection: single `proxy-in`, profile outbound,
/// `direct` freedom fallback and bypass rules for `exclusions`.
pub fn build_connection_config(snapshot: &ConfigSnapshot, exclusions: &[String]) -> Result<Value> {
    let mut rules = Vec::with_capacity(exclusions.len() + 1);
    for domain in exclusions.iter().filter(|domain| !domain.is_empty()) {
        rules.push(json!({
            "type": "field",
            "field": "domain",
            "domain": [domain],
            "outboundTag": "direct",
        }));
    }
    rules.push(json!({
        "type": "field",
        "inboundTag": ["proxy-in"],
        "outboundTag": "proxy-out",
    }));
    Ok(json!({
        "log": {"loglevel": "warning"},
        "inbounds": [mixed_inbound("proxy-in", snapshot.proxy_port)],
        "outbounds": [
            build_outbound(&snapshot.profile, "proxy-out")?,
            json!({"tag": "direct", "protocol": "freedom", "settings": {}}),
        ],
        "routing": routing(rules),
    }))
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use serde_json::json;
    use std::path::Path;

    use super::*;
    use crate::core::snapshot::snapshot_for;

    fn vless_profile(id: &str, port: u16) -> Profile {
        Profile {
            id: format!("sub-{id}"),
            subscription_id: "sub".to_string(),
            server_id: id.to_string(),
            link: format!(
                "vless://{id}@node.example.com:{port}?security=tls&type=ws&path=/p&host=h.example.com#Node-{id}"
            ),
            remarks: Some(format!("Node-{id}")),
            protocol: Some("vless".to_string()),
            address: Some("node.example.com".to_string()),
            port: Some(i64::from(port)),
            raw_config: json!({
                "security": "tls",
                "sni": "tls.example.com",
                "type": "ws",
                "path": "/p",
                "host": "h.example.com",
            })
            .to_string(),
            ..Default::default()
        }
    }

    fn trojan_profile() -> Profile {
        Profile {
            id: "sub-trojan".to_string(),
            subscription_id: "sub".to_string(),
            server_id: "trojan".to_string(),
            link: "trojan://trojan-secret@trojan.example.com:8443?security=tls&sni=s.example.com#T"
                .to_string(),
            protocol: Some("trojan".to_string()),
            address: Some("trojan.example.com".to_string()),
            port: Some(8443),
            raw_config: json!({"security": "tls", "sni": "s.example.com"}).to_string(),
            ..Default::default()
        }
    }

    fn ss_profile(sip002: bool) -> Profile {
        let (link, address, port) = if sip002 {
            let userinfo =
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("aes-256-gcm:sip-secret");
            (
                format!("ss://{userinfo}@ss.example.com:8388#SS-SIP002"),
                "ss.example.com",
                8388,
            )
        } else {
            let encoded = base64::engine::general_purpose::STANDARD
                .encode("aes-128-gcm:legacy-secret@ss-legacy.example.com:8389");
            (
                format!("ss://{encoded}#SS-Legacy"),
                "ss-legacy.example.com",
                8389,
            )
        };
        Profile {
            id: format!("sub-ss-{sip002}"),
            subscription_id: "sub".to_string(),
            server_id: format!("ss-{sip002}"),
            link,
            protocol: Some("ss".to_string()),
            address: Some(address.to_string()),
            port: Some(port),
            raw_config: json!({"method": if sip002 { "aes-256-gcm" } else { "aes-128-gcm" }})
                .to_string(),
            ..Default::default()
        }
    }

    fn vmess_profile() -> Profile {
        let json = json!({
            "ps": "VMessNode",
            "add": "vm.example.com",
            "port": "8443",
            "id": "vmess-uuid",
            "aid": "0",
            "scy": "auto",
            "net": "ws",
            "host": "h.example.com",
            "path": "/vmess",
            "tls": "tls",
            "sni": "s.example.com",
        })
        .to_string();
        let encoded = base64::engine::general_purpose::STANDARD.encode(&json);
        Profile {
            id: "sub-vmess".to_string(),
            subscription_id: "sub".to_string(),
            server_id: "vmess".to_string(),
            link: format!("vmess://{encoded}"),
            protocol: Some("vmess".to_string()),
            address: Some("vm.example.com".to_string()),
            port: Some(8443),
            raw_config: json.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn speedtest_config_shape() {
        let profiles = vec![vless_profile("a", 443), trojan_profile()];
        let ports = [10801u16, 10802];
        let config = build_speedtest_config(&profiles, &ports).expect("speedtest config");

        assert_eq!(config["log"]["loglevel"], "warning");
        let inbounds = config["inbounds"].as_array().expect("inbounds array");
        let outbounds = config["outbounds"].as_array().expect("outbounds array");
        let rules = config["routing"]["rules"]
            .as_array()
            .expect("routing rules array");
        assert_eq!(inbounds.len(), 2);
        assert_eq!(outbounds.len(), 2);
        assert_eq!(rules.len(), 2);
        assert_eq!(config["routing"]["domainStrategy"], "AsIs");

        for (index, inbound) in inbounds.iter().enumerate() {
            assert_eq!(inbound["listen"], "127.0.0.1");
            assert_eq!(inbound["protocol"], "mixed");
            assert_eq!(inbound["port"], ports[index]);
            assert_eq!(inbound["noauth"], true);
            assert_eq!(inbound["settings"]["noauth"], true);
            let in_tag = inbound["tag"].as_str().expect("inbound tag");
            let out_tag = outbounds[index]["tag"].as_str().expect("outbound tag");
            assert!(
                rules.iter().any(|rule| {
                    rule["inboundTag"][0] == in_tag && rule["outboundTag"] == out_tag
                }),
                "missing rule {in_tag} -> {out_tag}"
            );
        }
    }

    #[test]
    fn connection_config_shape() {
        let profile = vless_profile("a", 443);
        let snapshot = snapshot_for(&profile, 10808, Path::new("C:/tmp/xray"));
        let config = build_connection_config(&snapshot, &["example.com".to_string()])
            .expect("connection config");

        assert_eq!(config["log"]["loglevel"], "warning");
        let inbounds = config["inbounds"].as_array().expect("inbounds array");
        assert_eq!(inbounds.len(), 1);
        assert_eq!(inbounds[0]["tag"], "proxy-in");
        assert_eq!(inbounds[0]["listen"], "127.0.0.1");
        assert_eq!(inbounds[0]["port"], 10808);
        assert_eq!(inbounds[0]["protocol"], "mixed");

        let outbounds = config["outbounds"].as_array().expect("outbounds array");
        assert_eq!(outbounds.len(), 2);
        let proxy = outbounds
            .iter()
            .find(|outbound| outbound["tag"] == "proxy-out")
            .expect("proxy-out");
        let direct = outbounds
            .iter()
            .find(|outbound| outbound["tag"] == "direct")
            .expect("direct");
        assert_eq!(proxy["protocol"], "vless");
        assert_eq!(proxy["settings"]["vnext"][0]["users"][0]["id"], "a");
        assert_eq!(direct["protocol"], "freedom");

        let rules = config["routing"]["rules"]
            .as_array()
            .expect("routing rules array");
        let bypass = rules
            .iter()
            .find(|rule| rule["outboundTag"] == "direct")
            .expect("bypass rule");
        assert_eq!(bypass["field"], "domain");
        assert_eq!(bypass["domain"][0], "example.com");
        assert!(rules.iter().any(|rule| rule["outboundTag"] == "proxy-out"));
    }

    #[test]
    fn snapshot_is_immutable_value() {
        let profile = vless_profile("a", 443);
        let dir = Path::new("C:/tmp/xray-session");
        let first = snapshot_for(&profile, 10808, dir);
        let second = snapshot_for(&profile, 10808, dir);

        assert_eq!(
            serde_json::to_string(&first).expect("serialize first"),
            serde_json::to_string(&second).expect("serialize second")
        );
        assert_eq!(first, second);
        assert_eq!(first.subscription_id, "sub");
        assert_eq!(first.proxy_port, 10808);
        assert_eq!(first.anti_dpi, None);
        assert_eq!(first.profile, profile);
        assert_eq!(first.config_path, dir.join("xray-sub-a-10808.json"));
    }

    fn proxy_outbound(profile: &Profile) -> Value {
        let config =
            build_connection_config(&snapshot_for(profile, 10808, Path::new("C:/tmp/xray")), &[])
                .unwrap_or_else(|error| panic!("config build failed (test helper): {error}"));
        config["outbounds"][0].clone()
    }

    #[test]
    fn credentials_come_from_link() {
        let vless = proxy_outbound(&vless_profile("real-vless-uuid", 443));
        assert_eq!(
            vless["settings"]["vnext"][0]["users"][0]["id"],
            "real-vless-uuid"
        );

        let trojan = proxy_outbound(&trojan_profile());
        assert_eq!(
            trojan["settings"]["servers"][0]["password"],
            "trojan-secret"
        );

        let sip002 = proxy_outbound(&ss_profile(true));
        assert_eq!(sip002["settings"]["servers"][0]["method"], "aes-256-gcm");
        assert_eq!(sip002["settings"]["servers"][0]["password"], "sip-secret");

        let legacy = proxy_outbound(&ss_profile(false));
        assert_eq!(legacy["settings"]["servers"][0]["method"], "aes-128-gcm");
        assert_eq!(
            legacy["settings"]["servers"][0]["password"],
            "legacy-secret"
        );

        let vmess = proxy_outbound(&vmess_profile());
        assert_eq!(
            vmess["settings"]["vnext"][0]["users"][0]["id"],
            "vmess-uuid"
        );
    }

    #[test]
    fn raw_config_merged_into_outbound() {
        let profile = vless_profile("u", 443);
        let config = build_connection_config(
            &snapshot_for(&profile, 10808, Path::new("C:/tmp/xray")),
            &[],
        )
        .expect("config");
        let outbounds = config["outbounds"].as_array().unwrap();
        let proxy = &outbounds[0];
        assert_eq!(proxy["streamSettings"]["security"], "tls");
        assert_eq!(proxy["streamSettings"]["network"], "ws");
        assert_eq!(
            proxy["streamSettings"]["tlsSettings"]["serverName"],
            "tls.example.com"
        );
        assert_eq!(proxy["streamSettings"]["wsSettings"]["path"], "/p");
        assert_eq!(
            proxy["streamSettings"]["wsSettings"]["headers"]["Host"],
            "h.example.com"
        );

        let reality = Profile {
            link: "vless://reality-uuid@example.com:443?security=reality&sni=cdn.com&fp=chrome&pbk=abc&sid=de&type=raw#R".to_string(),
            raw_config: json!({
                "security": "reality", "sni": "cdn.com", "fp": "chrome",
                "pbk": "abc", "sid": "de", "type": "raw",
            })
            .to_string(),
            ..vless_profile("reality-uuid", 443)
        };
        let config = build_connection_config(
            &snapshot_for(&reality, 10808, Path::new("C:/tmp/xray")),
            &[],
        )
        .expect("reality config");
        let proxy = &config["outbounds"][0];
        assert_eq!(proxy["streamSettings"]["security"], "reality");
        assert_eq!(
            proxy["streamSettings"]["realitySettings"]["publicKey"],
            "abc"
        );
        assert_eq!(proxy["streamSettings"]["realitySettings"]["shortId"], "de");
        assert_eq!(
            proxy["streamSettings"]["realitySettings"]["fingerprint"],
            "chrome"
        );

        let config = build_connection_config(
            &snapshot_for(&vmess_profile(), 10808, Path::new("C:/tmp/xray")),
            &[],
        )
        .expect("vmess config");
        let proxy = &config["outbounds"][0];
        assert_eq!(proxy["streamSettings"]["network"], "ws");
        assert_eq!(proxy["streamSettings"]["security"], "tls");
        assert_eq!(proxy["streamSettings"]["wsSettings"]["path"], "/vmess");
    }

    #[test]
    fn unsupported_protocol_returns_error() {
        let profile = Profile {
            protocol: Some("hysteria".to_string()),
            ..vless_profile("x", 443)
        };
        assert!(build_connection_config(
            &snapshot_for(&profile, 10808, Path::new("C:/tmp/xray")),
            &[],
        )
        .is_err());
    }
}
