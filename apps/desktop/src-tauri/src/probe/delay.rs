use std::time::{Duration, Instant};

/// Measure real delay by issuing two GETs through `proxy_url`, 100 ms apart.
/// Returns the smallest positive elapsed millisecond count, or `None` if every
/// attempt times out or errors. No URLs are logged.
pub async fn measure_delay(proxy_url: &str, ping_url: &str, timeout: Duration) -> Option<u64> {
    // `.proxy()` disables reqwest's automatic environment/system proxy detection,
    // so every request below goes only through the explicit proxy.
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(proxy_url).ok()?)
        .timeout(timeout)
        .build()
        .ok()?;

    let mut samples: Vec<u64> = Vec::new();
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let started = Instant::now();
        if client.get(ping_url).send().await.is_ok() {
            let elapsed_ms = started.elapsed().as_millis();
            if elapsed_ms > 0 {
                samples.push(elapsed_ms as u64);
            }
        }
    }

    samples.into_iter().min()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::test_support::spawn_mock_proxy;

    #[tokio::test]
    async fn delay_uses_min_positive_ms() {
        let proxy = spawn_mock_proxy(&[], false);
        let result = measure_delay(
            &proxy,
            "http://probe.example/generate_204",
            Duration::from_millis(2000),
        )
        .await;
        assert!(result.is_some(), "expected a positive delay sample");
        assert!(result.unwrap() > 0, "expected a positive delay sample");
    }

    #[tokio::test]
    async fn delay_timeout_returns_none() {
        let proxy = spawn_mock_proxy(&[], true);
        let result = measure_delay(
            &proxy,
            "http://probe.example/generate_204",
            Duration::from_millis(200),
        )
        .await;
        assert_eq!(result, None);
    }
}
