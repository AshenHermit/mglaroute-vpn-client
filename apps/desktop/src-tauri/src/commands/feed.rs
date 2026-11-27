use serde::{Deserialize, Serialize};
use tauri::State;

use super::{AppState, CommandError};
use crate::subscription::fetch::{build_client, fetch_subscription};

pub const FEED_SERVICES_URL: &str =
    "https://api.mglaroute.ashen-hermit.space/api/feed/services";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeedServicePayload {
    pub id: String,
    pub name: String,
}

async fn fetch_feed_services(
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<FeedServicePayload>, CommandError> {
    let body = fetch_subscription(client, url, None)
        .await
        .map_err(|_| CommandError::Fetch)?;
    serde_json::from_str(&body).map_err(|_| CommandError::Fetch)
}

#[tauri::command]
pub async fn feed_services(
    _state: State<'_, AppState>,
) -> Result<Vec<FeedServicePayload>, CommandError> {
    let client = build_client();
    fetch_feed_services(&client, FEED_SERVICES_URL).await
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;

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
        (format!("http://{addr}/services"), handle)
    }

    #[test]
    fn parses_public_service_rows() {
        let (url, handle) = serve_once("200 OK", r#"[{"id":"svc-1","name":"YouTube"}]"#);
        let rows = run_async(async {
            let client = build_client();
            fetch_feed_services(&client, &url).await.expect("parse services")
        });
        handle.join().expect("server thread");
        assert_eq!(rows, vec![FeedServicePayload { id: "svc-1".into(), name: "YouTube".into() }]);
    }

    #[test]
    fn http_error_maps_to_fetch() {
        let (url, handle) = serve_once("404 Not Found", "missing");
        let result = run_async(async {
            let client = build_client();
            fetch_feed_services(&client, &url).await
        });
        handle.join().expect("server thread");
        assert_eq!(result.err(), Some(CommandError::Fetch));
    }

    #[test]
    fn malformed_body_maps_to_fetch() {
        let (url, handle) = serve_once("200 OK", "not json");
        let result = run_async(async {
            let client = build_client();
            fetch_feed_services(&client, &url).await
        });
        handle.join().expect("server thread");
        assert_eq!(result.err(), Some(CommandError::Fetch));
    }
}
