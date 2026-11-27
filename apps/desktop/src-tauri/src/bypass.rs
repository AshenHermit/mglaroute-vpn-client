//! Built-in bypass for local and private networks.
//!
//! LAN addresses and loopback only exist on the user's own network. Sending
//! them through a remote proxy node breaks local services: the node answers
//! with errors such as Cloudflare's "plain HTTP request was sent to HTTPS port".
//! These ranges are always bypassed, independently of user-configured
//! `proxy_exclusions`, and in both places that decide routing:
//!
//! - [`WININET_BYPASS`] entries go into `ProxyOverride`;
//! - [`XRAY_BYPASS_IPS`] CIDRs go into the xray `direct` routing rule.

/// WinInet `ProxyOverride` entries, in the registry's semicolon-list syntax.
/// `<local>` is prepended separately by `override_string`.
pub const WININET_BYPASS: &[&str] = &[
    "localhost",
    "127.*",
    "10.*",
    "172.16.*",
    "172.17.*",
    "172.18.*",
    "172.19.*",
    "172.20.*",
    "172.21.*",
    "172.22.*",
    "172.23.*",
    "172.24.*",
    "172.25.*",
    "172.26.*",
    "172.27.*",
    "172.28.*",
    "172.29.*",
    "172.30.*",
    "172.31.*",
    "192.168.*",
];

/// Xray routing `ip` entries. Explicit CIDRs avoid a `geoip.dat` dependency —
/// the client bundles the xray binary only.
pub const XRAY_BYPASS_IPS: &[&str] = &[
    "127.0.0.0/8",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "169.254.0.0/16",
    "::1/128",
    "fc00::/7",
    "fe80::/10",
];
