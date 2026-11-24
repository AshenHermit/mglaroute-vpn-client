use std::time::Duration;

use reqwest::header::USER_AGENT;

/// Fetch timeout mandated for subscription downloads.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Canonical client for subscription fetches: environment proxies are ignored
/// so a stale `HTTP_PROXY`/`HTTPS_PROXY` cannot break a local or direct feed.
pub fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(FETCH_TIMEOUT)
        .build()
        .expect("build subscription http client")
}

/// Fetch a subscription body as text. The caller owns the client (build it with
/// [`build_client`]) and decides whether to pass a stored User-Agent. Never log
/// the URL, headers or body — they can carry credentials.
pub async fn fetch_subscription(
    client: &reqwest::Client,
    url: &str,
    user_agent: Option<&str>,
) -> reqwest::Result<String> {
    let mut request = client.get(url).timeout(FETCH_TIMEOUT);
    if let Some(user_agent) = user_agent {
        request = request.header(USER_AGENT, user_agent);
    }
    request.send().await?.error_for_status()?.text().await
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::ffi::OsString;
    use std::future::Future;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;
    use std::thread;

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const BODY: &str = "vless://id@example.com:443#One";

    fn run_async<F: Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(future)
    }

    fn serve_once(status_line: &str, body: &str) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local listener");
        let addr = listener.local_addr().expect("local addr");
        let body = body.to_string();
        let status_line = status_line.to_string();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf).expect("read request");
            let response = format!(
                "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).expect("write response");
        });
        (format!("http://{addr}/sub"), handle)
    }

    /// Point every proxy env var at a dead address so a proxy-aware client fails.
    /// Serialized with a mutex because env vars are process-global.
    fn with_dead_proxy_env(f: impl FnOnce()) {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let keys = [
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
            "NO_PROXY",
            "no_proxy",
        ];
        let saved: Vec<(&'static str, Option<OsString>)> =
            keys.iter().map(|key| (*key, env::var_os(key))).collect();
        for key in keys {
            env::set_var(key, "http://127.0.0.1:9");
        }
        env::set_var("NO_PROXY", "");
        env::set_var("no_proxy", "");
        f();
        for (key, value) in saved {
            match value {
                Some(value) => env::set_var(key, value),
                None => env::remove_var(key),
            }
        }
    }

    #[test]
    fn fetch_uses_no_system_proxy() {
        with_dead_proxy_env(|| {
            let (url, handle) = serve_once("200 OK", BODY);
            let fetched = run_async(async {
                let client = build_client();
                fetch_subscription(&client, &url, None)
                    .await
                    .expect("fetch through no-proxy client")
            });
            handle.join().expect("server thread");
            assert_eq!(fetched, BODY);
        });
    }

    #[test]
    fn fetch_http_error_returns_err() {
        let (url, handle) = serve_once("404 Not Found", "missing");
        let result = run_async(async {
            let client = build_client();
            fetch_subscription(&client, &url, None).await
        });
        handle.join().expect("server thread");
        assert!(result.is_err());
    }
}
