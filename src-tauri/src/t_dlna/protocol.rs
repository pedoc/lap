use quick_xml::{Reader, events::Event};
use reqwest::{Client, Url};
use std::{net::Ipv4Addr, time::Duration};

pub const AV_TRANSPORT: &str = "urn:schemas-upnp-org:service:AVTransport:";
const MAX_XML: usize = 512 * 1024;

#[derive(Clone, Debug)]
pub struct Renderer {
    pub name: String,
    pub address: Ipv4Addr,
    pub control_url: Url,
    pub service_type: String,
}

pub fn client() -> Result<Client, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|_| "无法创建电视连接".into())
}

pub fn lan_address(address: Ipv4Addr) -> bool {
    address.is_private() || address.is_link_local()
}

fn local_url(url: &Url, address: Ipv4Addr) -> bool {
    url.scheme() == "http"
        && url
            .host_str()
            .and_then(|host| host.parse::<Ipv4Addr>().ok())
            == Some(address)
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
}

pub fn discovery_location(packet: &[u8], sender: Ipv4Addr) -> Option<Url> {
    if !lan_address(sender) {
        return None;
    }
    let text = std::str::from_utf8(packet).ok()?;
    let mut lines = text.lines();
    let status = lines.next()?.split_whitespace().collect::<Vec<_>>();
    if status.len() < 2 || !status[0].starts_with("HTTP/1.") || status[1] != "200" {
        return None;
    }
    let location = lines.find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case("location")
            .then_some(value.trim())
    })?;
    let url = Url::parse(location).ok()?;
    local_url(&url, sender).then_some(url)
}

#[derive(Default, Debug)]
pub(super) struct XmlNode {
    name: String,
    text: String,
    children: Vec<XmlNode>,
}

impl XmlNode {
    pub(super) fn child(&self, name: &str) -> Option<&Self> {
        self.children.iter().find(|node| node.name == name)
    }

    pub(super) fn value(&self, name: &str) -> &str {
        self.child(name).map(|node| node.text.trim()).unwrap_or("")
    }

    pub(super) fn descendant(&self, name: &str) -> Option<&Self> {
        if self.name == name {
            return Some(self);
        }
        self.children.iter().find_map(|node| node.descendant(name))
    }
}

pub(super) fn xml(text: &str) -> Result<XmlNode, String> {
    if text.len() > MAX_XML {
        return Err("电视返回的数据过大".into());
    }
    let mut reader = Reader::from_str(text);
    reader.config_mut().expand_empty_elements = true;
    let mut stack = vec![XmlNode::default()];
    let mut count = 0;
    loop {
        match reader.read_event().map_err(|_| "电视返回了无效的 XML")? {
            Event::Start(start) => {
                count += 1;
                if stack.len() >= 32 || count > 4096 {
                    return Err("电视返回的 XML 结构过大".into());
                }
                stack.push(XmlNode {
                    name: String::from_utf8_lossy(start.local_name().as_ref()).into_owned(),
                    ..Default::default()
                });
            }
            Event::End(_) => {
                if stack.len() <= 1 {
                    return Err("电视返回了无效的 XML".into());
                }
                let node = stack.pop().ok_or("电视返回了无效的 XML")?;
                stack
                    .last_mut()
                    .ok_or("电视返回了无效的 XML")?
                    .children
                    .push(node);
            }
            Event::Text(value) => stack
                .last_mut()
                .ok_or("电视返回了无效的 XML")?
                .text
                .push_str(&value.decode().map_err(|_| "电视 XML 编码无效")?),
            Event::CData(value) => stack
                .last_mut()
                .ok_or("电视返回了无效的 XML")?
                .text
                .push_str(&value.decode().map_err(|_| "电视 XML 编码无效")?),
            Event::GeneralRef(value) => {
                let character = if let Some(character) =
                    value.resolve_char_ref().map_err(|_| "电视 XML 字符无效")?
                {
                    character
                } else {
                    match value.decode().map_err(|_| "电视 XML 字符无效")?.as_ref() {
                        "amp" => '&',
                        "lt" => '<',
                        "gt" => '>',
                        "quot" => '"',
                        "apos" => '\'',
                        _ => return Err("电视 XML 包含不支持的实体".into()),
                    }
                };
                stack
                    .last_mut()
                    .ok_or("电视返回了无效的 XML")?
                    .text
                    .push(character);
            }
            Event::DocType(_) => return Err("电视 XML 包含不支持的文档类型".into()),
            Event::Eof => break,
            _ => {}
        }
    }
    if stack.len() != 1 {
        return Err("电视返回了无效的 XML".into());
    }
    stack.pop().ok_or_else(|| "电视返回了无效的 XML".into())
}

fn renderer_node(node: &XmlNode) -> Option<(&XmlNode, &XmlNode)> {
    if node.name == "device"
        && node
            .value("deviceType")
            .starts_with("urn:schemas-upnp-org:device:MediaRenderer:")
    {
        if let Some(service) = node.child("serviceList").and_then(|list| {
            list.children.iter().find(|service| {
                service.name == "service" && service.value("serviceType").starts_with(AV_TRANSPORT)
            })
        }) {
            return Some((node, service));
        }
    }
    node.children.iter().find_map(renderer_node)
}

pub fn parse_renderer(text: &str, location: &Url, address: Ipv4Addr) -> Result<Renderer, String> {
    if !local_url(location, address) {
        return Err("电视地址不匹配".into());
    }
    let document = xml(text)?;
    let root = document.child("root").ok_or("电视设备描述无效")?;
    let (device, service) = renderer_node(root).ok_or("设备未提供 DLNA 播放服务")?;
    let base = if root.value("URLBase").is_empty() {
        location.clone()
    } else {
        Url::parse(root.value("URLBase")).map_err(|_| "电视基础地址无效")?
    };
    let control_url = base
        .join(service.value("controlURL"))
        .map_err(|_| "电视控制地址无效")?;
    if service.value("controlURL").is_empty()
        || !local_url(&base, address)
        || !local_url(&control_url, address)
    {
        return Err("电视控制地址不匹配".into());
    }
    Ok(Renderer {
        name: if device.value("friendlyName").is_empty() {
            address.to_string()
        } else {
            device.value("friendlyName").to_owned()
        },
        address,
        control_url,
        service_type: service.value("serviceType").to_owned(),
    })
}

pub async fn response_text(mut response: reqwest::Response) -> Result<String, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "读取电视响应失败")? {
        if bytes.len() + chunk.len() > MAX_XML {
            return Err("电视返回的数据过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| "电视响应编码无效".into())
}

pub fn envelope(service: &str, action: &str, arguments: &[(&str, &str)]) -> String {
    let arguments = arguments
        .iter()
        .map(|(key, value)| format!("<{key}>{}</{key}>", quick_xml::escape::escape(*value)))
        .collect::<String>();
    format!(
        "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{action} xmlns:u=\"{}\">{arguments}</u:{action}></s:Body></s:Envelope>",
        quick_xml::escape::escape(service)
    )
}

pub async fn action(
    client: &Client,
    renderer: &Renderer,
    name: &str,
    arguments: &[(&str, &str)],
) -> Result<(), String> {
    query(client, renderer, name, arguments).await.map(|_| ())
}

pub(super) async fn query(
    client: &Client,
    renderer: &Renderer,
    name: &str,
    arguments: &[(&str, &str)],
) -> Result<XmlNode, String> {
    let response = client
        .post(renderer.control_url.clone())
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .header(
            "SOAPAction",
            format!("\"{}#{name}\"", renderer.service_type),
        )
        .body(envelope(&renderer.service_type, name, arguments))
        .send()
        .await
        .map_err(|_| "无法连接电视，请检查局域网连接与投屏接收功能")?;
    let status = response.status();
    let text = response_text(response).await?;
    let document = xml(&text).ok();
    if !status.is_success()
        || document
            .as_ref()
            .and_then(|node| node.descendant("Fault").map(|_| ()))
            .is_some()
    {
        let code = document
            .as_ref()
            .and_then(|node| node.descendant("errorCode").map(|code| code.text.clone()))
            .unwrap_or_else(|| status.as_u16().to_string());
        return Err(format!(
            "电视拒绝 {name} 请求（{code}）。请检查接收端支持的操作和媒体格式。"
        ));
    }
    let response_name = format!("{name}Response");
    if document
        .as_ref()
        .and_then(|node| node.descendant("Body"))
        .and_then(|body| body.child(&response_name))
        .is_none()
    {
        return Err(format!("电视未返回有效的 {name} 播放服务响应"));
    }
    document.ok_or_else(|| "电视播放服务响应无效".into())
}

pub fn metadata(title: &str, url: &str, mime: &str, size: u64) -> String {
    let class = if mime.starts_with("image/") {
        "object.item.imageItem.photo"
    } else {
        "object.item.videoItem"
    };
    format!(
        "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\"><item id=\"0\" parentID=\"-1\" restricted=\"1\"><dc:title>{}</dc:title><upnp:class>{class}</upnp:class><res protocolInfo=\"http-get:*:{mime}:*\" size=\"{size}\">{}</res></item></DIDL-Lite>",
        quick_xml::escape::escape(title),
        quick_xml::escape::escape(url)
    )
}

pub fn byte_range(header: Option<&str>, size: u64) -> Result<(u64, u64, bool), ()> {
    let Some(header) = header else {
        return Ok((0, size, false));
    };
    let value = header.strip_prefix("bytes=").ok_or(())?;
    if size == 0 || value.contains(',') {
        return Err(());
    }
    let (start, end) = value.split_once('-').ok_or(())?;
    let (start, end) = if start.is_empty() {
        let suffix = end.parse::<u64>().map_err(|_| ())?;
        if suffix == 0 {
            return Err(());
        }
        (size.saturating_sub(suffix), size - 1)
    } else {
        let start = start.parse::<u64>().map_err(|_| ())?;
        let end = if end.is_empty() {
            size - 1
        } else {
            end.parse::<u64>().map_err(|_| ())?.min(size - 1)
        };
        (start, end)
    };
    if start >= size || end < start {
        return Err(());
    }
    Ok((start, end - start + 1, true))
}

pub fn video_mime(path: &std::path::Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "mp4" | "m4v" => Some("video/mp4"),
        "mov" => Some("video/quicktime"),
        "mkv" => Some("video/x-matroska"),
        "avi" => Some("video/x-msvideo"),
        "webm" => Some("video/webm"),
        "mpeg" | "mpg" => Some("video/mpeg"),
        "ts" => Some("video/mp2t"),
        "m2ts" | "mts" => Some("video/vnd.dlna.mpeg-tts"),
        "wmv" => Some("video/x-ms-wmv"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    fn description() -> String {
        "<root xmlns=\"urn:schemas-upnp-org:device-1-0\"><device><deviceType>urn:schemas-upnp-org:device:MediaServer:1</deviceType><friendlyName>Wrong device</friendlyName><deviceList><device><deviceType>urn:schemas-upnp-org:device:MediaRenderer:2</deviceType><friendlyName>客厅 &amp; TV &#x31;</friendlyName><serviceList><service><serviceType>urn:schemas-upnp-org:service:AVTransport:3</serviceType><controlURL>/control?x=1&amp;y=2</controlURL></service></serviceList></device></deviceList></device></root>".into()
    }

    #[test]
    fn discovery_only_accepts_literal_lan_urls_from_the_responding_device() {
        let sender = Ipv4Addr::new(192, 168, 1, 10);
        let packet = |url: &str| format!("HTTP/1.1 200 OK\r\nLoCaTiOn: {url}\r\n\r\n");
        assert_eq!(
            discovery_location(
                packet("http://192.168.1.10:8080/root.xml").as_bytes(),
                sender
            )
            .unwrap()
            .port(),
            Some(8080)
        );
        for url in [
            "http://192.168.1.11/root",
            "http://127.0.0.1/root",
            "http://example.com/root",
            "https://192.168.1.10/root",
            "http://user@192.168.1.10/root",
            "file:///photos/a.jpg",
        ] {
            assert!(
                discovery_location(packet(url).as_bytes(), sender).is_none(),
                "{url}"
            );
        }
        assert!(
            discovery_location(
                packet("http://8.8.8.8/root").as_bytes(),
                Ipv4Addr::new(8, 8, 8, 8)
            )
            .is_none()
        );
    }

    #[test]
    fn renderer_uses_its_own_name_service_version_and_relative_control_url() {
        let address = Ipv4Addr::new(192, 168, 1, 10);
        let location = Url::parse("http://192.168.1.10:8080/device/root.xml").unwrap();
        let renderer = parse_renderer(&description(), &location, address).unwrap();
        assert_eq!(renderer.name, "客厅 & TV 1");
        assert_eq!(
            renderer.service_type,
            "urn:schemas-upnp-org:service:AVTransport:3"
        );
        assert_eq!(
            renderer.control_url.as_str(),
            "http://192.168.1.10:8080/control?x=1&y=2"
        );
        let relative = description().replace("/control?x=1&amp;y=2", "control");
        assert_eq!(
            parse_renderer(&relative, &location, address)
                .unwrap()
                .control_url
                .path(),
            "/device/control"
        );
    }

    #[test]
    fn description_cannot_redirect_control_off_the_discovered_device() {
        let address = Ipv4Addr::new(192, 168, 1, 10);
        let location = Url::parse("http://192.168.1.10/root.xml").unwrap();
        for control in [
            "http://192.168.1.11/control",
            "http://127.0.0.1/control",
            "//example.com/control",
        ] {
            assert!(
                parse_renderer(
                    &description().replace("/control?x=1&amp;y=2", control),
                    &location,
                    address
                )
                .is_err()
            );
        }
        assert!(
            parse_renderer(
                &description().replacen(
                    "<device>",
                    "<URLBase>http://8.8.8.8/</URLBase><device>",
                    1
                ),
                &location,
                address
            )
            .is_err()
        );
        assert!(
            xml("<!DOCTYPE root [<!ENTITY data SYSTEM 'file:///secret'>]><root>&data;</root>")
                .is_err()
        );
        assert!(xml(&format!("{}{}", "<a>".repeat(40), "</a>".repeat(40))).is_err());
        assert!(xml(&" ".repeat(MAX_XML + 1)).is_err());
    }

    #[test]
    fn soap_escapes_nested_metadata_without_losing_unicode_or_uri_characters() {
        let uri = "http://192.168.1.2/media/a.mp4?x=1&y=2";
        let metadata = metadata("旅行 <2026> & 家人", uri, "video/mp4", 100);
        let body = envelope(
            "urn:schemas-upnp-org:service:AVTransport:1",
            "SetAVTransportURI",
            &[
                ("InstanceID", "0"),
                ("CurrentURI", uri),
                ("CurrentURIMetaData", &metadata),
            ],
        );
        let parsed = xml(&body).unwrap();
        assert_eq!(parsed.descendant("CurrentURI").unwrap().text, uri);
        assert_eq!(
            parsed.descendant("CurrentURIMetaData").unwrap().text,
            metadata
        );
        assert_eq!(
            xml(&metadata).unwrap().descendant("title").unwrap().text,
            "旅行 <2026> & 家人"
        );
    }

    #[test]
    fn byte_ranges_handle_seeking_suffixes_bounds_and_invalid_requests() {
        assert_eq!(byte_range(None, 10), Ok((0, 10, false)));
        assert_eq!(byte_range(Some("bytes=2-5"), 10), Ok((2, 4, true)));
        assert_eq!(byte_range(Some("bytes=7-"), 10), Ok((7, 3, true)));
        assert_eq!(byte_range(Some("bytes=-3"), 10), Ok((7, 3, true)));
        assert_eq!(byte_range(Some("bytes=-99"), 10), Ok((0, 10, true)));
        assert_eq!(byte_range(Some("bytes=7-99"), 10), Ok((7, 3, true)));
        for value in [
            "bytes=10-",
            "bytes=5-2",
            "bytes=-0",
            "bytes=0-1,4-5",
            "bytes=abc-",
            "items=0-2",
            "bytes=18446744073709551616-",
        ] {
            assert!(byte_range(Some(value), 10).is_err(), "{value}");
        }
        assert!(byte_range(Some("bytes=0-"), 0).is_err());
        assert_eq!(byte_range(None, 0), Ok((0, 0, false)));
    }

    #[tokio::test]
    async fn standard_receiver_observes_transport_actions_and_reports_soap_faults() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            for index in 0..6 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut buffer = [0; 4096];
                    let count = socket.read(&mut buffer).await.unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&buffer[..count]);
                    if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&request[..end]);
                        let length: usize = headers
                            .lines()
                            .filter_map(|line| line.split_once(':'))
                            .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                            .unwrap()
                            .1
                            .trim()
                            .parse()
                            .unwrap();
                        if request.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8(request).unwrap());
                let body = if index == 4 {
                    "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><s:Fault><detail><UPnPError><errorCode>701</errorCode></UPnPError></detail></s:Fault></s:Body></s:Envelope>".to_owned()
                } else if index == 5 {
                    "<html>Not a SOAP service</html>".to_owned()
                } else {
                    let name = ["SetAVTransportURI", "Play", "Pause", "Stop"][index];
                    format!(
                        "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><u:{name}Response xmlns:u=\"{AV_TRANSPORT}1\"/></s:Body></s:Envelope>"
                    )
                };
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            if index == 4 {
                                "500 Internal Server Error"
                            } else {
                                "200 OK"
                            },
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
            requests
        });
        let renderer = Renderer {
            name: "Test TV".into(),
            address: Ipv4Addr::LOCALHOST,
            control_url: Url::parse(&format!("http://{address}/control")).unwrap(),
            service_type: format!("{AV_TRANSPORT}1"),
        };
        let client = client().unwrap();
        for name in ["SetAVTransportURI", "Play", "Pause", "Stop"] {
            let mut arguments = vec![("InstanceID", "0")];
            if name == "SetAVTransportURI" {
                arguments.extend([
                    ("CurrentURI", "http://192.168.1.2/a.mp4"),
                    ("CurrentURIMetaData", ""),
                ]);
            } else if name == "Play" {
                arguments.push(("Speed", "1"));
            }
            action(&client, &renderer, name, &arguments).await.unwrap();
        }
        assert!(
            action(&client, &renderer, "Pause", &[("InstanceID", "0")])
                .await
                .unwrap_err()
                .contains("701")
        );
        assert!(
            action(&client, &renderer, "Stop", &[("InstanceID", "0")])
                .await
                .unwrap_err()
                .contains("未返回有效")
        );
        let requests = server.await.unwrap();
        for (request, name) in requests.iter().zip([
            "SetAVTransportURI",
            "Play",
            "Pause",
            "Stop",
            "Pause",
            "Stop",
        ]) {
            assert!(request.to_ascii_lowercase().contains(
                &format!("soapaction: \"{}#{name}\"", renderer.service_type).to_ascii_lowercase()
            ));
            assert!(request.contains(&format!("<u:{name}")));
        }
    }
}
