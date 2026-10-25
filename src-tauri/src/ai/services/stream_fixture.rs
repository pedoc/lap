//! Loopback SSE responses with real packet timing, never a paid model.
use serde_json::Value;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};

pub(super) fn fixture(frames: Vec<(Duration, String)>) -> (String, thread::JoinHandle<Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/gateway/v1/", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "missing SSE SDK request");
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("SSE fixture accept failed: {error}"),
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket.set_nodelay(true).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut bytes = Vec::new();
        let end = loop {
            let mut buffer = [0; 4096];
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0, "SSE client closed before request headers");
            bytes.extend_from_slice(&buffer[..count]);
            if let Some(position) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
                break position + 4;
            }
            assert!(bytes.len() < 64 * 1024, "oversized fixture headers");
        };
        let header = String::from_utf8_lossy(&bytes[..end]);
        assert!(header.starts_with("POST /gateway/v1/chat/completions HTTP/"));
        let length = header
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .map(|(_, value)| value.trim().parse::<usize>().unwrap())
            .expect("SDK JSON requests declare their length");
        assert!(length <= 32 * 1024 * 1024, "oversized fixture request");
        while bytes.len() < end + length {
            let mut buffer = [0; 4096];
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0, "SSE client closed before request body");
            bytes.extend_from_slice(&buffer[..count]);
        }
        let request = serde_json::from_slice(&bytes[end..end + length]).unwrap();
        let response = b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n";
        if socket.write_all(response).is_err() {
            return request;
        }
        for (delay, frame) in frames {
            if !delay.is_zero() {
                thread::sleep(delay);
            }
            // Cancellation closes the stream normally; it must not panic this worker.
            if socket.write_all(frame.as_bytes()).is_err() || socket.flush().is_err() {
                break;
            }
        }
        request
    });
    (base, worker)
}
