pub mod delay;
pub mod service_check;

pub use delay::measure_delay;
pub use service_check::check_service_profile;

#[cfg(test)]
pub(crate) mod test_support {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::{Duration, Instant};

    /// Local mock HTTP proxy on an ephemeral port.
    ///
    /// Reads the request line and replies `204` for normal paths, `302` for paths
    /// containing any string in `bad_paths`. When `silent`, accepts connections
    /// but never replies (simulates a stalled upstream). No URLs are logged.
    pub(crate) fn spawn_mock_proxy(bad_paths: &[&str], silent: bool) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let bad_paths: Vec<String> = bad_paths.iter().map(|s| s.to_string()).collect();
        listener.set_nonblocking(true).unwrap();

        thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut reader = BufReader::new(stream.try_clone().unwrap());
                        let mut request_line = String::new();
                        if reader.read_line(&mut request_line).is_err() {
                            continue;
                        }
                        if silent {
                            // Hold the connection open; client must time out.
                            thread::sleep(Duration::from_millis(600));
                            continue;
                        }
                        // Small delay so elapsed.as_millis() is provably positive.
                        thread::sleep(Duration::from_millis(5));
                        let status = if bad_paths.iter().any(|p| request_line.contains(p)) {
                            "302 Found"
                        } else {
                            "204 No Content"
                        };
                        let response = format!("HTTP/1.1 {}\r\nContent-Length: 0\r\n\r\n", status);
                        let _ = stream.write_all(response.as_bytes());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        format!("http://{}", addr)
    }
}
