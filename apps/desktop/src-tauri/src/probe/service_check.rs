use std::time::Duration;

/// Check every URL in a service profile through the same explicit proxy.
/// `true` only when the list is non-empty and every URL returns 2xx.
/// Empty list, timeout, error, or any non-2xx yields `false`. No URLs are logged.
pub async fn check_service_profile(urls: &[String], proxy_url: &str, timeout: Duration) -> bool {
    if urls.is_empty() {
        return false;
    }

    // `.proxy()` disables reqwest's automatic environment/system proxy detection,
    // so every request goes only through the explicit proxy. No direct fallback.
    let proxy = match reqwest::Proxy::all(proxy_url) {
        Ok(proxy) => proxy,
        Err(_) => return false,
    };
    let client = match reqwest::Client::builder()
        .proxy(proxy)
        .timeout(timeout)
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };

    let checks = urls.iter().map(|url| {
        let client = client.clone();
        async move {
            match client.get(url).send().await {
                Ok(response) => response.status().is_success(),
                Err(_) => false,
            }
        }
    });

    futures::future::join_all(checks)
        .await
        .into_iter()
        .all(|ok| ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::test_support::spawn_mock_proxy;

    fn urls(paths: &[&str]) -> Vec<String> {
        paths
            .iter()
            .map(|p| format!("http://service.example{}", p))
            .collect()
    }

    #[tokio::test]
    async fn profile_fails_when_any_url_non_2xx() {
        let proxy = spawn_mock_proxy(&["/bad"], false);
        let urls = urls(&["/ok", "/bad"]);
        assert!(!check_service_profile(&urls, &proxy, Duration::from_millis(2000)).await);
    }

    #[tokio::test]
    async fn service_check_uses_explicit_proxy() {
        // Environment proxies point at a dead address; the local mock must win.
        let saved = [
            ("HTTPS_PROXY", std::env::var("HTTPS_PROXY").ok()),
            ("HTTP_PROXY", std::env::var("HTTP_PROXY").ok()),
            ("ALL_PROXY", std::env::var("ALL_PROXY").ok()),
        ];
        std::env::set_var("HTTPS_PROXY", "http://127.0.0.1:1");
        std::env::set_var("HTTP_PROXY", "http://127.0.0.1:1");
        std::env::set_var("ALL_PROXY", "http://127.0.0.1:1");

        let proxy = spawn_mock_proxy(&[], false);
        let urls = urls(&["/ok"]);
        let passed = check_service_profile(&urls, &proxy, Duration::from_millis(2000)).await;

        for (key, value) in saved {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        assert!(
            passed,
            "explicit proxy must be used, not environment proxies"
        );
    }

    #[tokio::test]
    async fn empty_urls_never_pass() {
        let proxy = spawn_mock_proxy(&[], false);
        let urls: Vec<String> = Vec::new();
        assert!(!check_service_profile(&urls, &proxy, Duration::from_millis(2000)).await);
    }
}
