use super::native;
use reqwest::{Client, Url};
use std::collections::HashMap;
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

pub fn discovery_location(location: &str, sender: Ipv4Addr) -> Option<Url> {
    if !lan_address(sender) {
        return None;
    }
    let url = Url::parse(location).ok()?;
    local_url(&url, sender).then_some(url)
}

pub(super) struct XmlDocument<'a>(roxmltree::Document<'a>);
impl<'a> XmlDocument<'a> {
    pub(super) fn descendant(&self, name: &str) -> Option<roxmltree::Node<'_, 'a>> {
        self.0
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == name)
    }
    pub(super) fn child(&self, name: &str) -> Option<roxmltree::Node<'_, 'a>> {
        self.0.root().child(name)
    }
}
pub(super) trait XmlElement<'a, 'input: 'a> {
    fn child(&self, name: &str) -> Option<roxmltree::Node<'a, 'input>>;
    fn value(&self, name: &str) -> &'a str;
}
impl<'a, 'input: 'a> XmlElement<'a, 'input> for roxmltree::Node<'a, 'input> {
    fn child(&self, name: &str) -> Option<Self> {
        self.children()
            .find(|node| node.is_element() && node.tag_name().name() == name)
    }
    fn value(&self, name: &str) -> &'a str {
        self.child(name)
            .and_then(|node| node.text())
            .unwrap_or("")
            .trim()
    }
}
pub(super) fn xml(text: &str) -> Result<XmlDocument<'_>, String> {
    if text.len() > MAX_XML {
        return Err("电视返回的数据过大".into());
    }
    let document = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 8192,
            ..Default::default()
        },
    )
    .map_err(|_| "电视返回了无效的 XML")?;
    if document
        .descendants()
        .any(|node| node.ancestors().count() > 32)
    {
        return Err("电视返回的 XML 结构过大".into());
    }
    Ok(XmlDocument(document))
}

pub fn parse_renderer(text: &str, location: &Url, address: Ipv4Addr) -> Result<Renderer, String> {
    if !local_url(location, address) {
        return Err("电视地址不匹配".into());
    }
    let document = xml(text)?;
    let root = document.child("root").ok_or("电视设备描述无效")?;
    let (device, service) = root
        .descendants()
        .filter(|node| {
            node.is_element()
                && node.tag_name().name() == "device"
                && node
                    .value("deviceType")
                    .starts_with("urn:schemas-upnp-org:device:MediaRenderer:")
        })
        .find_map(|device| {
            device
                .child("serviceList")
                .and_then(|list| {
                    list.children().find(|service| {
                        service.is_element()
                            && service.tag_name().name() == "service"
                            && service
                                .value("serviceType")
                                .strip_prefix(AV_TRANSPORT)
                                .and_then(|version| version.parse::<u32>().ok())
                                .is_some_and(|version| version > 0)
                    })
                })
                .map(|service| (device, service))
        })
        .ok_or("设备未提供 DLNA 播放服务")?;
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

pub fn envelope(service: &str, action: &str, arguments: &[(&str, &str)]) -> Result<String, String> {
    let (name, response) = action
        .strip_suffix("Response")
        .map(|name| (name, true))
        .unwrap_or((action, false));
    let body = native::document(name, service, arguments, response)?;
    // libupnp builds the action XML. The media-directory handler streams this
    // standard wrapper so an unpaginated large album need not become a DOM.
    let body = body
        .strip_prefix("<?xml version=\"1.0\"?>")
        .unwrap_or(&body)
        .trim();
    Ok(format!(
        "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body>{body}</s:Body></s:Envelope>"
    ))
}

pub async fn action(
    renderer: &Renderer,
    name: &str,
    arguments: &[(&str, &str)],
) -> Result<(), String> {
    query(renderer, name, arguments).await.map(|_| ())
}

pub(super) async fn query(
    renderer: &Renderer,
    name: &str,
    arguments: &[(&str, &str)],
) -> Result<HashMap<String, String>, String> {
    let text = native::action(
        renderer.control_url.to_string(),
        renderer.service_type.clone(),
        name.to_owned(),
        arguments
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect(),
    )
    .await
    .map_err(|error| format!("电视未返回有效的 {name} 播放服务响应（{error}）"))?;
    let document = xml(&text)?;
    let response_name = format!("{name}Response");
    let response = document
        .descendant(&response_name)
        .ok_or_else(|| format!("电视未返回有效的 {name} 播放服务响应"))?;
    Ok(response
        .children()
        .filter(|node| node.is_element())
        .map(|node| {
            (
                node.tag_name().name().to_owned(),
                node.text().unwrap_or("").trim().to_owned(),
            )
        })
        .collect())
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
    let ranges = http_range::HttpRange::parse(header, size).map_err(|_| ())?;
    if ranges.len() != 1 || ranges[0].length == 0 {
        return Err(());
    }
    Ok((ranges[0].start, ranges[0].length, true))
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
        let address = Ipv4Addr::new(192, 168, 1, 10);
        assert!(discovery_location("http://192.168.1.10:8080/root.xml", address).is_some());
        for value in [
            "http://192.168.1.11/root.xml",
            "http://127.0.0.1/root.xml",
            "http://example.com/root.xml",
            "https://192.168.1.10/root.xml",
            "http://user:pass@192.168.1.10/root.xml",
            "http://192.168.1.10/root.xml#fragment",
        ] {
            assert!(discovery_location(value, address).is_none(), "{value}");
        }
        assert!(discovery_location("http://8.8.8.8/root.xml", Ipv4Addr::new(8, 8, 8, 8)).is_none());
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
        let body = body.unwrap();
        let parsed = xml(&body).unwrap();
        assert_eq!(
            parsed
                .descendant("CurrentURI")
                .unwrap()
                .text()
                .unwrap_or(""),
            uri
        );
        assert_eq!(
            parsed
                .descendant("CurrentURIMetaData")
                .unwrap()
                .text()
                .unwrap_or(""),
            metadata
        );
        assert_eq!(
            xml(&metadata)
                .unwrap()
                .descendant("title")
                .unwrap()
                .text()
                .unwrap_or(""),
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
                    "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><s:Fault><detail><UPnPError xmlns=\"urn:schemas-upnp-org:control-1-0\"><errorCode>701</errorCode></UPnPError></detail></s:Fault></s:Body></s:Envelope>".to_owned()
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
                            "HTTP/1.1 {}\r\nContent-Type: text/xml; charset=\"utf-8\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
            action(&renderer, name, &arguments).await.unwrap();
        }
        assert!(
            action(&renderer, "Pause", &[("InstanceID", "0")])
                .await
                .unwrap_err()
                .contains("701")
        );
        assert!(
            action(&renderer, "Stop", &[("InstanceID", "0")])
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
            assert!(
                request.contains(&format!("<{name}")) || request.contains(&format!("<u:{name}"))
            );
        }
    }
}
