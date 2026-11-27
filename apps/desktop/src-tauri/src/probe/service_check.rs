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
        Ok(proxy) => proxy.no_proxy(None),
        Err(_) => return false,
    };
    let client = match reqwest::Client::builder()
        .no_proxy()
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
        let proxy = spawn_mock_proxy(&[], false);
        let urls = urls(&["/ok"]);
        assert!(check_service_profile(&urls, &proxy, Duration::from_millis(2000)).await);
    }

    #[tokio::test]
    async fn empty_urls_never_pass() {
        let proxy = spawn_mock_proxy(&[], false);
        let urls: Vec<String> = Vec::new();
        assert!(!check_service_profile(&urls, &proxy, Duration::from_millis(2000)).await);
    }
}
