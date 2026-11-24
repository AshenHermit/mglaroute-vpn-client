use base64::Engine;
use serde_json::{Map, Value};
use std::collections::HashSet;

/// URI markers that identify decoded text as a subscription body.
const URI_MARKERS: [&str; 4] = ["vmess://", "vless://", "trojan://", "ss://"];

/// Query parameters persisted into `raw_config`. Unknown params are dropped.
const PARAM_WHITELIST: [&str; 18] = [
    "security",
    "sni",
    "fp",
    "pbk",
    "sid",
    "spx",
    "flow",
    "encryption",
    "type",
    "path",
    "host",
    "alpn",
    "serviceName",
    "headerType",
    "mode",
    "packetEncoding",
    "allowInsecure",
    "skip-cert-verify",
];

/// Legacy vmess JSON keys (per design spec).
const VMESS_FIELDS: [&str; 13] = [
    "ps", "add", "port", "id", "aid", "scy", "net", "host", "path", "tls", "sni", "alpn", "fp",
];

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedProfile {
    pub link: String,
    pub remarks: String,
    pub protocol: String,
    pub address: String,
    pub port: u16,
    pub raw_config: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodeOutcome {
    pub profiles: Vec<ParsedProfile>,
    pub bad_lines: usize,
}

/// Strip whitespace, map the URL-safe alphabet onto the standard one and
/// restore `=` padding so both standard and URL-safe bodies decode the same way.
fn normalize_base64(input: &str) -> String {
    let mut normalized = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_whitespace() {
            continue;
        }
        match ch {
            '-' => normalized.push('+'),
            '_' => normalized.push('/'),
            _ => normalized.push(ch),
        }
    }
    let remainder = normalized.len() % 4;
    if remainder != 0 {
        for _ in 0..(4 - remainder) {
            normalized.push('=');
        }
    }
    normalized
}

fn decode_base64(input: &str) -> Option<String> {
    let normalized = normalize_base64(input);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(normalized.as_bytes())
        .ok()?;
    String::from_utf8(bytes).ok()
}

fn has_uri_marker(text: &str) -> bool {
    URI_MARKERS.iter().any(|marker| text.contains(marker))
}

/// Whole-body cascade: decode only when the result looks like a subscription.
fn expand_body(body: &str) -> String {
    match decode_base64(body) {
        Some(decoded) if has_uri_marker(&decoded) => decoded,
        _ => body.to_string(),
    }
}

fn whitelisted_params(query: &str) -> Value {
    let mut config = Map::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.split_once('=') {
            Some((key, value)) => (key, value),
            None => (pair, ""),
        };
        if PARAM_WHITELIST.contains(&key) {
            config.insert(key.to_string(), Value::String(value.to_string()));
        }
    }
    Value::Object(config)
}

fn split_host_port(authority: &str) -> Option<(String, u16)> {
    let (host, port) = authority.rsplit_once(':')?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() {
        return None;
    }
    Some((host.to_string(), port.parse::<u16>().ok()?))
}

fn split_fragment(input: &str) -> (&str, String) {
    match input.split_once('#') {
        Some((head, remarks)) => (head, remarks.to_string()),
        None => (input, String::new()),
    }
}

/// vless/trojan: `uuid|password@host:port?query#remarks`.
fn parse_standard(protocol: &str, rest: &str, link: &str) -> Option<ParsedProfile> {
    let (head, remarks) = split_fragment(rest);
    let (authority, query) = match head.split_once('?') {
        Some((authority, query)) => (authority, query),
        None => (head, ""),
    };
    let (_, host_port) = authority.rsplit_once('@')?;
    let (address, port) = split_host_port(host_port)?;
    Some(ParsedProfile {
        link: link.to_string(),
        remarks,
        protocol: protocol.to_string(),
        address,
        port,
        raw_config: whitelisted_params(query),
    })
}

/// vmess legacy: base64.JSON with `ps/add/port/...`.
fn parse_vmess(encoded: &str, link: &str) -> Option<ParsedProfile> {
    let json = decode_base64(encoded)?;
    let value: Value = serde_json::from_str(&json).ok()?;
    let object = value.as_object()?;
    let address = object.get("add")?.as_str()?.to_string();
    let port = match object.get("port")? {
        Value::String(port) => port.parse::<u16>().ok()?,
        Value::Number(port) => u16::try_from(port.as_u64()?).ok()?,
        _ => return None,
    };
    let remarks = object
        .get("ps")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let mut config = Map::new();
    for key in VMESS_FIELDS {
        if let Some(value) = object.get(key) {
            config.insert(key.to_string(), value.clone());
        }
    }
    Some(ParsedProfile {
        link: link.to_string(),
        remarks,
        protocol: "vmess".to_string(),
        address,
        port,
        raw_config: Value::Object(config),
    })
}

/// ss SIP002: `base64url(method:password)@host:port#remarks`.
/// ss legacy: `base64(method:password@host:port)#remarks`.
fn parse_ss(rest: &str, link: &str) -> Option<ParsedProfile> {
    let (head, remarks) = split_fragment(rest);
    let (head, query) = match head.split_once('?') {
        Some((head, query)) => (head, query),
        None => (head, ""),
    };

    let (method, host_port): (String, String) = if head.contains('@') {
        let (userinfo, host_port) = head.rsplit_once('@')?;
        let decoded = decode_base64(userinfo)?;
        let (method, _) = decoded.split_once(':')?;
        (method.to_string(), host_port.to_string())
    } else {
        let decoded = decode_base64(head)?;
        let (userinfo, host_port) = decoded.rsplit_once('@')?;
        let (method, _) = userinfo.split_once(':')?;
        (method.to_string(), host_port.to_string())
    };
    let (address, port) = split_host_port(&host_port)?;

    let mut config = whitelisted_params(query);
    if let Value::Object(ref mut object) = config {
        object.insert("method".to_string(), Value::String(method));
    }
    Some(ParsedProfile {
        link: link.to_string(),
        remarks,
        protocol: "ss".to_string(),
        address,
        port,
        raw_config: config,
    })
}

/// Decode a subscription body. Pure: no DB, no network, no logging of bodies.
///
/// Cascades whole-body base64 (standard and URL-safe, stripped whitespace and
/// padding), then splits lines, trims, skips empties and deduplicates exact
/// links while preserving order. Malformed or unknown links are counted in
/// `bad_lines` and skipped.
pub fn decode_subscription(body: &str) -> DecodeOutcome {
    let text = expand_body(body);
    let mut profiles = Vec::new();
    let mut seen = HashSet::new();
    let mut bad_lines = 0usize;

    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if !seen.insert(line.to_string()) {
            continue;
        }
        let parsed = if let Some(rest) = line.strip_prefix("vless://") {
            parse_standard("vless", rest, line)
        } else if let Some(rest) = line.strip_prefix("trojan://") {
            parse_standard("trojan", rest, line)
        } else if let Some(rest) = line.strip_prefix("vmess://") {
            parse_vmess(rest, line)
        } else if let Some(rest) = line.strip_prefix("ss://") {
            parse_ss(rest, line)
        } else {
            None
        };

        match parsed {
            Some(profile) => profiles.push(profile),
            None => bad_lines += 1,
        }
    }

    DecodeOutcome {
        profiles,
        bad_lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn decode_one(link: &str) -> ParsedProfile {
        let outcome = decode_subscription(link);
        assert_eq!(outcome.bad_lines, 0);
        assert_eq!(outcome.profiles.len(), 1);
        outcome.profiles.into_iter().next().unwrap()
    }

    #[test]
    fn decode_plain_vless() {
        let p = decode_one(
            "vless://id@example.com:443?security=reality&sni=cdn.com&fp=chrome&pbk=abc&sid=de&type=raw#MyNode",
        );
        assert_eq!(p.protocol, "vless");
        assert_eq!(p.address, "example.com");
        assert_eq!(p.port, 443);
        assert_eq!(p.raw_config["security"], "reality");
        assert_eq!(p.raw_config["pbk"], "abc");
        assert_eq!(p.remarks, "MyNode");
    }

    #[test]
    fn decode_vmess_legacy_base64() {
        let json = r#"{"ps":"VMessNode","add":"vm.example.com","port":"8443","id":"uuid","aid":"0","scy":"auto","net":"ws","host":"h.example.com","path":"/p","tls":"tls","sni":"s.example.com"}"#;
        let encoded = base64::engine::general_purpose::STANDARD.encode(json);
        let p = decode_one(&format!("vmess://{encoded}"));
        assert_eq!(p.protocol, "vmess");
        assert_eq!(p.remarks, "VMessNode");
        assert_eq!(p.address, "vm.example.com");
        assert_eq!(p.port, 8443);
        assert_eq!(p.raw_config["net"], "ws");
    }

    #[test]
    fn decode_ss_sip002_and_legacy() {
        let sip_userinfo =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("aes-256-gcm:pass");
        let sip = format!("ss://{sip_userinfo}@ss.example.com:8388#Sip002");
        let legacy_inner = "aes-128-gcm:pass2@legacy.example.com:8389";
        let legacy = format!(
            "ss://{}#Legacy",
            base64::engine::general_purpose::STANDARD.encode(legacy_inner)
        );

        let outcome = decode_subscription(&format!("{sip}\n{legacy}"));
        assert_eq!(outcome.bad_lines, 0);
        assert_eq!(outcome.profiles.len(), 2);
        for p in &outcome.profiles {
            assert_eq!(p.protocol, "ss");
            assert!(p.raw_config["method"]
                .as_str()
                .is_some_and(|m| !m.is_empty()));
        }
        assert_eq!(outcome.profiles[0].address, "ss.example.com");
        assert_eq!(outcome.profiles[0].port, 8388);
        assert_eq!(outcome.profiles[0].remarks, "Sip002");
        assert_eq!(outcome.profiles[1].address, "legacy.example.com");
        assert_eq!(outcome.profiles[1].port, 8389);
        assert_eq!(outcome.profiles[1].remarks, "Legacy");
    }

    #[test]
    fn decode_trojan_and_reality_params() {
        let p = decode_one("trojan://pass@example.com:443?security=tls&sni=x&allowInsecure=1#T");
        assert_eq!(p.protocol, "trojan");
        assert_eq!(p.address, "example.com");
        assert_eq!(p.port, 443);
        assert_eq!(p.raw_config["security"], "tls");
        assert_eq!(p.raw_config["sni"], "x");
        assert_eq!(p.raw_config["allowInsecure"], "1");
        assert_eq!(p.remarks, "T");
    }

    #[test]
    fn bad_lines_skipped_valid_kept() {
        let a = "vless://a@example.com:443#One";
        let b = "trojan://p@example.org:443#Two";
        let outcome = decode_subscription(&format!("{a}\ngarbage line\n{b}"));
        assert_eq!(outcome.bad_lines, 1);
        assert_eq!(outcome.profiles.len(), 2);
        assert_eq!(outcome.profiles[0].link, a);
        assert_eq!(outcome.profiles[1].link, b);
    }

    #[test]
    fn decode_urlsafe_unpadded_base64() {
        let a = "vless://id@example.com:443#One";
        let b = "trojan://pass@example.org:8443#Two>";
        let plain = format!("{a}\n{b}");
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(plain.as_bytes());
        assert!(!encoded.contains('='), "fixture must be unpadded");
        assert!(
            encoded.contains('-') || encoded.contains('_'),
            "fixture must exercise the url-safe alphabet"
        );

        let outcome = decode_subscription(&encoded);
        assert_eq!(outcome.bad_lines, 0);
        assert_eq!(outcome.profiles.len(), 2);
        assert_eq!(outcome.profiles[0].link, a);
        assert_eq!(outcome.profiles[1].link, b);
    }

    #[test]
    fn dedup_exact_links() {
        let link = "vless://id@example.com:443#Dup";
        let outcome = decode_subscription(&format!("{link}\n{link}\n{link}"));
        assert_eq!(outcome.bad_lines, 0);
        assert_eq!(outcome.profiles.len(), 1);
        assert_eq!(outcome.profiles[0].link, link);
    }
}
