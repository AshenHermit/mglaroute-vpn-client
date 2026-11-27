//! Xray JSON config assembly. No process launch, no filesystem writes here:
//! callers persist the returned value next to the [`ConfigSnapshot`] path.

use std::path::Path;

use serde_json::{json, Map, Value};

use crate::{
    bypass::XRAY_BYPASS_IPS,
    storage::models::{Profile, RoutingRule},
    subscription::decode::decode_base64,
};

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

/// One config for a single probe core: one `mixed` inbound on loopback, one
/// outbound for this profile and one routing rule tying the two together. No
/// `direct` fallback and no exclusions — every request must use the proxy.
pub fn build_probe_config(profile: &Profile, port: u16) -> Result<Value> {
    let in_tag = "probe-in";
    let out_tag = "probe-out";
    Ok(json!({
        "log": {"loglevel": "warning"},
        "inbounds": [mixed_inbound(in_tag, port)],
        "outbounds": [build_outbound(profile, out_tag)?],
        "routing": routing(vec![json!({
            "type": "field",
            "inboundTag": [in_tag],
            "outboundTag": out_tag,
        })]),
    }))
}

/// Map a routing rule's `outbound` value to the xray outbound tag.
fn outbound_tag(value: &str) -> &'static str {
    match value {
        "direct" => "direct",
        "block" => "block",
        // `proxy` and any unknown value fall through to the profile outbound.
        _ => "proxy-out",
    }
}

/// Parse a rule's JSON `string[]` column, dropping empty entries. Entry strings
/// are preserved verbatim (so `geosite:`/`geoip:` prefixes survive); only empty
/// strings and surrounding whitespace are trimmed.
fn rule_entries(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw)
        .unwrap_or_default()
        .into_iter()
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !entry.is_empty())
        .collect()
}

/// One config for a live connection: single `proxy-in`, profile outbound,
/// `direct` freedom fallback, private bypass, exclusions, user routing rules
/// in sort order and a catch-all proxy rule last.
pub fn build_connection_config(
    snapshot: &ConfigSnapshot,
    exclusions: &[String],
    rules: &[RoutingRule],
    asset_dir: Option<&Path>,
) -> Result<Value> {
    let mut routing_rules = Vec::with_capacity(exclusions.len() + rules.len() * 2 + 2);
    // Local/private destinations must go direct: the remote node cannot reach
    // the user's LAN, and may answer with a Cloudflare HTTP/HTTPS port error.
    routing_rules.push(json!({
        "type": "field",
        "ip": XRAY_BYPASS_IPS,
        "outboundTag": "direct",
    }));
    for domain in exclusions.iter().filter(|domain| !domain.is_empty()) {
        routing_rules.push(json!({
            "type": "field",
            "field": "domain",
            "domain": [domain],
            "outboundTag": "direct",
        }));
    }

    let mut needs_block = false;
    for rule in rules.iter().filter(|rule| rule.enabled) {
        let tag = outbound_tag(&rule.outbound);
        if tag == "block" {
            needs_block = true;
        }
        let domains = rule_entries(&rule.domains);
        if !domains.is_empty() {
            routing_rules.push(json!({
                "type": "field",
                "field": "domain",
                "domain": domains,
                "outboundTag": tag,
            }));
        }
        let ips = rule_entries(&rule.ips);
        if !ips.is_empty() {
            routing_rules.push(json!({
                "type": "field",
                "field": "ip",
                "ip": ips,
                "outboundTag": tag,
            }));
        }
    }

    routing_rules.push(json!({
        "type": "field",
        "inboundTag": ["proxy-in"],
        "outboundTag": "proxy-out",
    }));

    let mut outbounds = vec![
        build_outbound(&snapshot.profile, "proxy-out")?,
        json!({"tag": "direct", "protocol": "freedom", "settings": {}}),
    ];
    if needs_block {
        outbounds.push(json!({"tag": "block", "protocol": "blackhole", "settings": {}}));
    }

    let mut config = Map::new();
    config.insert("log".to_string(), json!({"loglevel": "warning"}));
    config.insert(
        "inbounds".to_string(),
        json!([mixed_inbound("proxy-in", snapshot.proxy_port)]),
    );
    config.insert("outbounds".to_string(), Value::Array(outbounds));
    config.insert("routing".to_string(), routing(routing_rules));
    if let Some(dir) = asset_dir {
        config.insert(
            "env".to_string(),
            json!({"XRAY_LOCATION_ASSET": dir.to_string_lossy().into_owned()}),
        );
    }
    Ok(Value::Object(config))
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use serde_json::json;
    use std::path::Path;

    use super::*;
    use crate::core::snapshot::snapshot_for;
    use crate::storage::models::RoutingRule;

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
    fn probe_config_shape() {
        let profile = vless_profile("a", 443);
        let config = build_probe_config(&profile, 10801).expect("probe config");

        let inbounds = config["inbounds"].as_array().expect("inbounds array");
        let outbounds = config["outbounds"].as_array().expect("outbounds array");
        let rules = config["routing"]["rules"]
            .as_array()
            .expect("routing rules array");
        assert_eq!(inbounds.len(), 1);
        assert_eq!(outbounds.len(), 1);
        assert_eq!(rules.len(), 1);
        assert_eq!(inbounds[0]["port"], 10801);
        assert_eq!(inbounds[0]["protocol"], "mixed");
        assert_eq!(outbounds[0]["protocol"], "vless");
        assert_eq!(rules[0]["inboundTag"][0], inbounds[0]["tag"]);
        assert_eq!(rules[0]["outboundTag"], outbounds[0]["tag"]);
        assert!(
            outbounds
                .iter()
                .all(|outbound| outbound["protocol"] != "freedom"),
            "probe config must not add a direct outbound"
        );
    }

    #[test]
    fn connection_config_shape() {
        let profile = vless_profile("a", 443);
        let snapshot = snapshot_for(&profile, 10808, Path::new("C:/tmp/xray"));
        let config = build_connection_config(&snapshot, &["example.com".to_string()], &[], None)
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
            .find(|rule| rule["field"] == "domain" && rule["outboundTag"] == "direct")
            .expect("domain bypass rule");
        assert_eq!(bypass["field"], "domain");
        assert_eq!(bypass["domain"][0], "example.com");
        assert!(rules.iter().any(|rule| rule["outboundTag"] == "proxy-out"));
    }

    #[test]
    fn connection_config_bypasses_private_networks() {
        let profile = vless_profile("a", 443);
        let config = build_connection_config(
            &snapshot_for(&profile, 10808, Path::new("C:/tmp/xray")),
            &[],
            &[],
            None,
        )
        .expect("connection config");

        let rules = config["routing"]["rules"]
            .as_array()
            .expect("routing rules array");
        let bypass_index = rules
            .iter()
            .position(|rule| rule["ip"].is_array() && rule["outboundTag"] == "direct")
            .expect("private IP bypass rule must exist");
        let ips = rules[bypass_index]["ip"]
            .as_array()
            .expect("bypass ip array");
        for range in ["127.0.0.0/8", "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"] {
            assert!(
                ips.iter().any(|ip| ip == range),
                "bypass rule must contain {range}"
            );
        }
        let proxy_index = rules
            .iter()
            .position(|rule| rule["outboundTag"] == "proxy-out")
            .expect("catch-all proxy rule");
        assert!(
            bypass_index < proxy_index,
            "private bypass rule must precede the catch-all proxy rule"
        );
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
        let config = build_connection_config(
            &snapshot_for(profile, 10808, Path::new("C:/tmp/xray")),
            &[],
            &[],
            None,
        )
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
            &[],
            None,
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
            &[],
            None,
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
            &[],
            None,
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
            &[],
            None,
        )
        .is_err());
    }

    fn rule(outbound: &str, domains: &[&str], ips: &[&str], enabled: bool, sort: i64) -> RoutingRule {
        RoutingRule {
            id: format!("r-{sort}"),
            remark: String::new(),
            enabled,
            outbound: outbound.to_string(),
            domains: serde_json::to_string(&domains).expect("domains json"),
            ips: serde_json::to_string(&ips).expect("ips json"),
            sort,
            ..Default::default()
        }
    }

    fn config_for_rules(rules: &[RoutingRule]) -> Value {
        let profile = vless_profile("a", 443);
        build_connection_config(
            &snapshot_for(&profile, 10808, Path::new("C:/tmp/xray")),
            &["excluded.example".to_string()],
            rules,
            None,
        )
        .expect("connection config")
    }

    fn rules_of(config: &Value) -> Vec<Value> {
        config["routing"]["rules"]
            .as_array()
            .expect("routing rules array")
            .clone()
    }

    #[test]
    fn build_connection_config_no_active_rules_unchanged() {
        let config = config_for_rules(&[]);

        assert_eq!(config["outbounds"].as_array().expect("outbounds").len(), 2);
        let rules = rules_of(&config);
        // private bypass first, one exclusion rule, catch-all last
        assert!(rules[0]["ip"].is_array());
        assert_eq!(rules[0]["outboundTag"], "direct");
        assert_eq!(rules[1]["domain"][0], "excluded.example");
        assert_eq!(rules[1]["outboundTag"], "direct");
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[2]["inboundTag"][0], "proxy-in");
        assert_eq!(rules[2]["outboundTag"], "proxy-out");
    }

    #[test]
    fn build_connection_config_rule_order_and_disabled_rule_skipped() {
        let rules = vec![
            rule("proxy", &["proxy.example"], &[], true, 0),
            rule("block", &[], &["10.1.2.0/24"], true, 1),
            rule("direct", &["skip.example"], &[], false, 2),
        ];
        let config = config_for_rules(&rules);
        let generated = rules_of(&config);

        // private bypass + exclusion precede user rules, catch-all is last
        assert!(generated[0]["ip"].is_array());
        assert_eq!(generated[1]["domain"][0], "excluded.example");
        let last = generated.len() - 1;
        assert_eq!(generated[last]["inboundTag"][0], "proxy-in");
        assert_eq!(generated[last]["outboundTag"], "proxy-out");

        let user = &generated[2..last];
        assert_eq!(user.len(), 2, "disabled rule must be skipped");
        assert_eq!(user[0]["domain"][0], "proxy.example");
        assert_eq!(user[0]["outboundTag"], "proxy-out");
        assert_eq!(user[1]["ip"][0], "10.1.2.0/24");
        assert_eq!(user[1]["outboundTag"], "block");
        assert!(
            !user.iter().any(|r| r["domain"][0] == "skip.example"),
            "disabled rule must not appear"
        );
    }

    #[test]
    fn build_connection_config_block_rule_adds_blackhole() {
        let rules = vec![rule("block", &["bad.example"], &[], true, 0)];
        let config = config_for_rules(&rules);

        let outbounds = config["outbounds"].as_array().expect("outbounds");
        let block = outbounds
            .iter()
            .find(|outbound| outbound["tag"] == "block")
            .expect("block outbound");
        assert_eq!(block["protocol"], "blackhole");
        assert_eq!(block["settings"], json!({}));

        assert!(
            rules_of(&config)
                .iter()
                .any(|r| r["domain"][0] == "bad.example" && r["outboundTag"] == "block"),
            "user rule must point at block outbound"
        );
    }

    #[test]
    fn build_connection_config_no_block_rule_omits_blackhole() {
        let config = config_for_rules(&[rule("proxy", &["ok.example"], &[], true, 0)]);
        let outbounds = config["outbounds"].as_array().expect("outbounds");
        assert!(
            !outbounds.iter().any(|outbound| outbound["tag"] == "block"),
            "no block rule means no blackhole outbound"
        );
    }

    #[test]
    fn build_connection_config_preserves_geo_prefixes_and_sets_asset_env() {
        let rule = rule("direct", &["geosite:private"], &["geoip:private"], true, 0);
        let profile = vless_profile("a", 443);
        let snapshot = snapshot_for(&profile, 10808, Path::new("C:/tmp/xray"));

        let with_env = build_connection_config(
            &snapshot,
            &[],
            &[rule.clone()],
            Some(Path::new("C:/fake/binaries")),
        )
        .expect("config with asset env");
        assert_eq!(
            with_env["env"]["XRAY_LOCATION_ASSET"],
            "C:/fake/binaries"
        );
        let generated = rules_of(&with_env);
        assert!(generated.iter().any(|r| r["domain"][0] == "geosite:private"));
        assert!(generated.iter().any(|r| r["ip"][0] == "geoip:private"));

        let without_env =
            build_connection_config(&snapshot, &[], &[rule], None).expect("config without env");
        assert!(
            without_env.get("env").is_none(),
            "asset_dir None must not add an env key"
        );
    }

    #[test]
    fn build_probe_config_unchanged_with_routing_rules() {
        // build_probe_config takes no routing input: it must stay isolated even
        // when connection-level rules and exclusions would point elsewhere.
        let profile = vless_profile("a", 443);
        let config = build_probe_config(&profile, 1080).expect("probe config");

        let outbounds = config["outbounds"].as_array().expect("outbounds");
        assert_eq!(outbounds.len(), 1, "probe config has exactly one outbound");
        assert_eq!(outbounds[0]["tag"], "probe-out");

        let rules = rules_of(&config);
        assert_eq!(rules.len(), 1, "probe config has exactly one routing rule");
        assert_eq!(rules[0]["inboundTag"][0], "probe-in");
        assert_eq!(rules[0]["outboundTag"], "probe-out");
        assert!(
            !outbounds.iter().any(|o| o["tag"] == "block" || o["protocol"] == "blackhole"),
            "probe config must have no block outbound"
        );
        assert!(config.get("env").is_none(), "probe config must have no env");
    }
}
