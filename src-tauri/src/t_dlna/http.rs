use std::{collections::HashMap, io, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

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
}

fn invalid() -> io::Error {
    io::ErrorKind::InvalidData.into()
}

async fn more(stream: &mut TcpStream, bytes: &mut Vec<u8>, maximum: usize) -> io::Result<()> {
    let mut buffer = [0u8; 4096];
    let count = stream.read(&mut buffer).await?;
    if count == 0 || bytes.len() + count > maximum {
        return Err(invalid());
    }
    bytes.extend_from_slice(&buffer[..count]);
    Ok(())
}

pub async fn read(stream: &mut TcpStream) -> io::Result<Request> {
    tokio::time::timeout(Duration::from_secs(10), read_inner(stream)).await?
}

async fn read_inner(stream: &mut TcpStream) -> io::Result<Request> {
    let mut bytes = Vec::new();
    let end = loop {
        if let Some(end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            break end;
        }
        if bytes.len() > 16 * 1024 {
            return Err(invalid());
        }
        more(stream, &mut bytes, 256 * 1024 + 32 * 1024).await?;
    };
    if end > 16 * 1024 {
        return Err(invalid());
    }
    let text = std::str::from_utf8(&bytes[..end]).map_err(|_| invalid())?;
    let mut lines = text.split("\r\n");
    let parts = lines
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>();
    if parts.len() != 3 || !parts[2].starts_with("HTTP/1.") || !parts[1].starts_with('/') {
        return Err(invalid());
    }
    let (method, target) = (parts[0].to_owned(), parts[1].to_owned());
    let mut headers = HashMap::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or_else(invalid)?;
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() || headers.insert(name, value.trim().to_owned()).is_some() {
            return Err(invalid());
        }
    }
    let mut body = bytes.split_off(end + 4);
    let length = headers
        .get("content-length")
        .map(|value| value.parse::<usize>().map_err(|_| invalid()))
        .transpose()?;
    if let Some(encoding) = headers.get("transfer-encoding") {
        if length.is_some() || !encoding.eq_ignore_ascii_case("chunked") {
            return Err(invalid());
        }
        let mut decoded = Vec::new();
        let mut position = 0;
        loop {
            let line_end = loop {
                if let Some(end) = body[position..]
                    .windows(2)
                    .position(|value| value == b"\r\n")
                {
                    break position + end;
                }
                if body.len() - position > 1024 {
                    return Err(invalid());
                }
                more(stream, &mut body, 256 * 1024 + 32 * 1024).await?;
            };
            let line = std::str::from_utf8(&body[position..line_end]).map_err(|_| invalid())?;
            let size = usize::from_str_radix(line.split(';').next().unwrap_or_default(), 16)
                .map_err(|_| invalid())?;
            position = line_end + 2;
            if size == 0 {
                break;
            }
            if size > 256 * 1024 || decoded.len() + size > 256 * 1024 {
                return Err(invalid());
            }
            while body.len() < position + size + 2 {
                more(stream, &mut body, 256 * 1024 + 32 * 1024).await?;
            }
            if &body[position + size..position + size + 2] != b"\r\n" {
                return Err(invalid());
            }
            decoded.extend_from_slice(&body[position..position + size]);
            position += size + 2;
        }
        body = decoded;
    } else {
        let length = length.unwrap_or(0);
        if length > 256 * 1024 {
            return Err(invalid());
        }
        while body.len() < length {
            more(stream, &mut body, 256 * 1024 + 4096).await?;
        }
        body.truncate(length);
    }
    Ok(Request {
        method,
        target,
        headers,
        body,
    })
}

pub async fn reply(
    stream: &mut TcpStream,
    status: &str,
    mime: &str,
    body: &[u8],
    extra: &[(&str, String)],
    head: bool,
) -> io::Result<()> {
    let mut headers = vec![
        ("Content-Type", mime.to_owned()),
        ("Content-Length", body.len().to_string()),
        (
            "Server",
            format!("Lap/{} UPnP/1.0", env!("CARGO_PKG_VERSION")),
        ),
    ];
    headers.extend_from_slice(extra);
    super::headers(stream, status, &headers).await?;
    if !head {
        tokio::time::timeout(Duration::from_secs(30), stream.write_all(body)).await??;
    }
    Ok(())
}

pub async fn reply_file(
    stream: &mut TcpStream,
    file: std::fs::File,
    length: u64,
) -> io::Result<()> {
    super::headers(
        stream,
        "200 OK",
        &[
            ("Content-Type", "text/xml; charset=\"utf-8\"".into()),
            ("Content-Length", length.to_string()),
            (
                "Server",
                format!("Lap/{} UPnP/1.0", env!("CARGO_PKG_VERSION")),
            ),
        ],
    )
    .await?;
    let mut file = tokio::fs::File::from_std(file);
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        tokio::time::timeout(Duration::from_secs(30), stream.write_all(&buffer[..count])).await??;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    async fn parse(text: &str) -> io::Result<Request> {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            read(&mut stream).await
        });
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_all(text.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();
        server.await.unwrap()
    }

    #[tokio::test]
    async fn soap_body_supports_content_length_and_chunked_clients_and_rejects_ambiguous_lengths() {
        let request =
            parse("POST /control HTTP/1.1\r\nContent-Length: 6\r\nSOAPAction: test\r\n\r\n<xml/>")
                .await
                .unwrap();
        assert_eq!(request.body, b"<xml/>");
        assert_eq!(request.header("soapaction"), Some("test"));
        assert_eq!(parse("POST /control HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n3\r\n<xm\r\n3\r\nl/>\r\n0\r\n\r\n").await.unwrap().body,b"<xml/>");
        for text in [
            "POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nx",
            "POST / HTTP/1.1\r\nContent-Length: 2\r\nTransfer-Encoding: chunked\r\n\r\nxx",
            "POST / HTTP/1.1\r\nContent-Length: 9999999\r\n\r\n",
            "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n10000000\r\n",
        ] {
            assert!(parse(text).await.is_err());
        }
    }
}
