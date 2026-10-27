use bytes::Bytes;
use futures_util::{Stream, TryStreamExt};
use http_body_util::{BodyExt, Full, Limited, StreamBody, combinators::UnsyncBoxBody};
use hyper::{body::Frame, service::service_fn};
use hyper_util::rt::{TokioIo, TokioTimer};
use std::{collections::HashMap, future::Future, io, time::Duration};
use tokio::net::TcpStream;
use tokio_util::io::ReaderStream;

pub type Body = UnsyncBoxBody<Bytes, io::Error>;
pub type Response = hyper::Response<Body>;

pub struct Request {
    pub method: String,
    pub target: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}
impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }
    async fn collect(request: hyper::Request<hyper::body::Incoming>) -> Result<Self, ()> {
        let (parts, body) = request.into_parts();
        if parts.uri.scheme().is_some() || parts.uri.authority().is_some() {
            return Err(());
        }
        let mut headers = HashMap::new();
        for (name, value) in &parts.headers {
            // These headers select an action, resource, or subscription at the access boundary.
            if headers.contains_key(name.as_str())
                && matches!(
                    name.as_str(),
                    "soapaction" | "range" | "sid" | "callback" | "nt" | "timeout"
                )
            {
                return Err(());
            }
            headers.insert(name.to_string(), value.to_str().map_err(|_| ())?.to_owned());
        }
        let body = tokio::time::timeout(
            Duration::from_secs(10),
            Limited::new(body, 256 * 1024).collect(),
        )
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?
        .to_bytes()
        .to_vec();
        Ok(Self {
            method: parts.method.to_string(),
            target: parts
                .uri
                .path_and_query()
                .map(|value| value.as_str())
                .unwrap_or("/")
                .to_owned(),
            headers,
            body,
        })
    }
}

pub async fn serve<F, Fut>(stream: TcpStream, handler: F) -> io::Result<()>
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = io::Result<Response>> + Send,
{
    let mut stream = tokio_io_timeout::TimeoutStream::new(stream);
    stream.set_read_timeout(Some(Duration::from_secs(10)));
    stream.set_write_timeout(Some(Duration::from_secs(30)));
    let handler = std::sync::Arc::new(handler);
    let service = service_fn(move |request| {
        let request = Request::collect(request);
        let handler = handler.clone();
        async move {
            match request.await {
                Ok(request) => handler(request).await,
                Err(()) => reply("400 Bad Request", "text/plain", b"", &[], false),
            }
        }
    });
    hyper::server::conn::http1::Builder::new()
        .keep_alive(false)
        .max_buf_size(16 * 1024)
        .timer(TokioTimer::new())
        .header_read_timeout(Duration::from_secs(10))
        .serve_connection(TokioIo::new(Box::pin(stream)), service)
        .await
        .map_err(io::Error::other)
}

pub fn stream_body<S>(stream: S) -> Body
where
    S: Stream<Item = io::Result<Bytes>> + Send + 'static,
{
    StreamBody::new(stream.map_ok(Frame::data)).boxed_unsync()
}
pub fn response(status: &str, values: &[(&str, String)], body: Body) -> io::Result<Response> {
    let status = status
        .split_whitespace()
        .next()
        .ok_or(io::ErrorKind::InvalidInput)?;
    let mut response = hyper::Response::builder()
        .status(status)
        .header("Connection", "close")
        .header("Cache-Control", "no-store");
    for (key, value) in values {
        response = response.header(*key, value);
    }
    response.body(body).map_err(io::Error::other)
}
pub fn empty() -> Body {
    Full::new(Bytes::new())
        .map_err(|never| match never {})
        .boxed_unsync()
}
pub fn reply(
    status: &str,
    mime: &str,
    body: &[u8],
    extra: &[(&str, String)],
    head: bool,
) -> io::Result<Response> {
    let mut headers = vec![
        ("Content-Type", mime.to_owned()),
        ("Content-Length", body.len().to_string()),
        (
            "Server",
            format!("Lap/{} UPnP/1.0", env!("CARGO_PKG_VERSION")),
        ),
    ];
    headers.extend_from_slice(extra);
    let body = if head {
        empty()
    } else {
        Full::new(Bytes::copy_from_slice(body))
            .map_err(|never| match never {})
            .boxed_unsync()
    };
    response(status, &headers, body)
}
pub fn reply_file(file: std::fs::File, length: u64) -> io::Result<Response> {
    response(
        "200 OK",
        &[
            ("Content-Type", "text/xml; charset=\"utf-8\"".into()),
            ("Content-Length", length.to_string()),
            (
                "Server",
                format!("Lap/{} UPnP/1.0", env!("CARGO_PKG_VERSION")),
            ),
        ],
        stream_body(ReaderStream::new(tokio::fs::File::from_std(file))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    async fn exchange(text: &[u8]) -> Vec<u8> {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let _ = serve(stream, |request| async move {
                reply("200 OK", "text/xml", &request.body, &[], false)
            })
            .await;
        });
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_all(text).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();
        response
    }
    #[tokio::test]
    async fn hyper_decodes_soap_bodies_and_rejects_malformed_or_oversized_requests() {
        for request in [
            "POST /control HTTP/1.1\r\nHost: tv\r\nContent-Length: 6\r\nSOAPAction: test\r\n\r\n<xml/>",
            "POST /control HTTP/1.1\r\nHost: tv\r\nTransfer-Encoding: chunked\r\n\r\n3\r\n<xm\r\n3\r\nl/>\r\n0\r\n\r\n",
        ] {
            let response = exchange(request.as_bytes()).await;
            assert!(response.starts_with(b"HTTP/1.1 200"));
            assert!(response.ends_with(b"<xml/>"));
        }
        let mut oversized =
            b"POST / HTTP/1.1\r\nHost: tv\r\nContent-Length: 262145\r\n\r\n".to_vec();
        oversized.resize(oversized.len() + 262145, b'x');
        for request in [
            b"POST / HTTP/1.1\r\nHost: tv\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nx".to_vec(),
            b"POST / HTTP/1.1\r\nHost: tv\r\nSOAPAction: one\r\nSOAPAction: two\r\nContent-Length: 0\r\n\r\n".to_vec(),
            oversized,
        ] {
            let response = exchange(&request).await;
            assert!(!response.starts_with(b"HTTP/1.1 200"));
        }
    }
}
