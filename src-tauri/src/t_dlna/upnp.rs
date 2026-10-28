use super::protocol::XmlElement;
use super::{
    http::Request,
    protocol,
    scope::{Entry, Page, Snapshot},
};

pub const MEDIA_SERVER: &str = "urn:schemas-upnp-org:device:MediaServer:1";
pub const CONTENT_DIRECTORY: &str = "urn:schemas-upnp-org:service:ContentDirectory:1";
pub const CONNECTION_MANAGER: &str = "urn:schemas-upnp-org:service:ConnectionManager:1";
pub const PROTOCOLS: &str = "http-get:*:image/jpeg:*,http-get:*:video/mp4:*,http-get:*:video/quicktime:*,http-get:*:video/x-matroska:*,http-get:*:video/x-msvideo:*,http-get:*:video/webm:*,http-get:*:video/mpeg:*,http-get:*:video/mp2t:*,http-get:*:video/vnd.dlna.mpeg-tts:*,http-get:*:video/x-ms-wmv:*";
pub type Catalogue = fn(&Snapshot, u32, u32, &str, Option<i64>) -> Result<Page, String>;
pub type StreamCatalogue = fn(
    &Snapshot,
    u32,
    u32,
    &str,
    &mut dyn FnMut(Entry) -> Result<(), String>,
) -> Result<(usize, usize), String>;
pub const DIDL_OPEN: &str = "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\">";

fn escape(value: &str) -> String {
    let value = value
        .chars()
        .filter(|character| {
            matches!(character, '\t' | '\n' | '\r')
                || (*character >= ' ' && *character != '\u{fffe}' && *character != '\u{ffff}')
        })
        .collect::<String>();
    quick_xml::escape::escape(value).into_owned()
}

pub fn description(id: &str, title: &str, event_base: &str, media_base: &str) -> String {
    let media_base = escape(media_base);
    format!(
        "<?xml version=\"1.0\"?><root xmlns=\"urn:schemas-upnp-org:device-1-0\"><specVersion><major>1</major><minor>0</minor></specVersion><device><deviceType>{MEDIA_SERVER}</deviceType><friendlyName>{}</friendlyName><manufacturer>Lap</manufacturer><modelName>Lap Media Server</modelName><modelNumber>{}</modelNumber><UDN>uuid:{id}</UDN><serviceList><service><serviceType>{CONTENT_DIRECTORY}</serviceType><serviceId>urn:upnp-org:serviceId:ContentDirectory</serviceId><SCPDURL>{media_base}/content-directory.xml</SCPDURL><controlURL>{media_base}/control/content-directory</controlURL><eventSubURL>{event_base}/{id}/events/content-directory</eventSubURL></service><service><serviceType>{CONNECTION_MANAGER}</serviceType><serviceId>urn:upnp-org:serviceId:ConnectionManager</serviceId><SCPDURL>{media_base}/connection-manager.xml</SCPDURL><controlURL>{media_base}/control/connection-manager</controlURL><eventSubURL>{event_base}/{id}/events/connection-manager</eventSubURL></service></serviceList></device></root>",
        escape(title),
        env!("CARGO_PKG_VERSION")
    )
}

pub fn scpd(content_directory: bool) -> String {
    let mut actions = String::new();
    let mut variables: std::collections::BTreeMap<&str, (&str, bool)> =
        std::collections::BTreeMap::new();
    let mut action = |name: &str, arguments: &[(&str, &str, &'static str, &'static str, bool)]| {
        actions.push_str(&format!("<action><name>{name}</name><argumentList>"));
        for (name, direction, state, datatype, evented) in arguments {
            variables.insert(state, (datatype, *evented));
            actions.push_str(&format!("<argument><name>{name}</name><direction>{direction}</direction><relatedStateVariable>{state}</relatedStateVariable></argument>"));
        }
        actions.push_str("</argumentList></action>");
    };
    if content_directory {
        action(
            "Browse",
            &[
                ("ObjectID", "in", "A_ARG_TYPE_ObjectID", "string", false),
                ("BrowseFlag", "in", "A_ARG_TYPE_BrowseFlag", "string", false),
                ("Filter", "in", "A_ARG_TYPE_Filter", "string", false),
                ("StartingIndex", "in", "A_ARG_TYPE_Index", "ui4", false),
                ("RequestedCount", "in", "A_ARG_TYPE_Count", "ui4", false),
                (
                    "SortCriteria",
                    "in",
                    "A_ARG_TYPE_SortCriteria",
                    "string",
                    false,
                ),
                ("Result", "out", "A_ARG_TYPE_Result", "string", false),
                ("NumberReturned", "out", "A_ARG_TYPE_Count", "ui4", false),
                ("TotalMatches", "out", "A_ARG_TYPE_Count", "ui4", false),
                ("UpdateID", "out", "A_ARG_TYPE_UpdateID", "ui4", false),
            ],
        );
        action(
            "GetSearchCapabilities",
            &[("SearchCaps", "out", "SearchCapabilities", "string", false)],
        );
        action(
            "GetSortCapabilities",
            &[("SortCaps", "out", "SortCapabilities", "string", false)],
        );
        action(
            "GetSystemUpdateID",
            &[("Id", "out", "SystemUpdateID", "ui4", true)],
        );
    } else {
        action(
            "GetProtocolInfo",
            &[
                ("Source", "out", "SourceProtocolInfo", "string", true),
                ("Sink", "out", "SinkProtocolInfo", "string", true),
            ],
        );
        action(
            "GetCurrentConnectionIDs",
            &[(
                "ConnectionIDs",
                "out",
                "CurrentConnectionIDs",
                "string",
                true,
            )],
        );
        action(
            "GetCurrentConnectionInfo",
            &[
                ("ConnectionID", "in", "A_ARG_TYPE_ConnectionID", "i4", false),
                ("RcsID", "out", "A_ARG_TYPE_RcsID", "i4", false),
                (
                    "AVTransportID",
                    "out",
                    "A_ARG_TYPE_AVTransportID",
                    "i4",
                    false,
                ),
                (
                    "ProtocolInfo",
                    "out",
                    "A_ARG_TYPE_ProtocolInfo",
                    "string",
                    false,
                ),
                (
                    "PeerConnectionManager",
                    "out",
                    "A_ARG_TYPE_ConnectionManager",
                    "string",
                    false,
                ),
                (
                    "PeerConnectionID",
                    "out",
                    "A_ARG_TYPE_ConnectionID",
                    "i4",
                    false,
                ),
                ("Direction", "out", "A_ARG_TYPE_Direction", "string", false),
                (
                    "Status",
                    "out",
                    "A_ARG_TYPE_ConnectionStatus",
                    "string",
                    false,
                ),
            ],
        );
    }
    let states=variables.iter().map(|(name,(datatype,evented))| {
        let values=if *name=="A_ARG_TYPE_BrowseFlag" { "<allowedValueList><allowedValue>BrowseMetadata</allowedValue><allowedValue>BrowseDirectChildren</allowedValue></allowedValueList>" } else { "" };
        format!("<stateVariable sendEvents=\"{}\"><name>{name}</name><dataType>{datatype}</dataType>{values}</stateVariable>",if *evented {"yes"} else {"no"})
    }).collect::<String>();
    format!(
        "<?xml version=\"1.0\"?><scpd xmlns=\"urn:schemas-upnp-org:service-1-0\"><specVersion><major>1</major><minor>0</minor></specVersion><actionList>{actions}</actionList><serviceStateTable>{states}</serviceStateTable></scpd>"
    )
}

fn container(id: &str, parent: &str, title: &str, count: usize) -> String {
    format!(
        "<container id=\"{id}\" parentID=\"{parent}\" restricted=\"1\" searchable=\"0\" childCount=\"{count}\"><dc:title>{}</dc:title><upnp:class>object.container.storageFolder</upnp:class></container>",
        escape(title)
    )
}

pub fn media_path(id: i64, mime: &str) -> String {
    let extension = match mime {
        "image/jpeg" => "jpg",
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        "video/x-matroska" => "mkv",
        "video/x-msvideo" => "avi",
        "video/webm" => "webm",
        "video/mpeg" => "mpg",
        "video/mp2t" => "ts",
        "video/vnd.dlna.mpeg-tts" => "m2ts",
        "video/x-ms-wmv" => "wmv",
        _ => "bin",
    };
    format!("media/{id}.{extension}")
}

pub(super) fn item(entry: &Entry, base: &str) -> String {
    let mut attributes = String::new();
    if entry.video {
        attributes.push_str(&format!(" size=\"{}\"", entry.size));
        if entry.width > 0 && entry.height > 0 {
            attributes.push_str(&format!(" resolution=\"{}x{}\"", entry.width, entry.height));
        }
        if entry.duration > 0 {
            attributes.push_str(&format!(
                " duration=\"{}:{:02}:{:02}.000\"",
                entry.duration / 3600,
                entry.duration / 60 % 60,
                entry.duration % 60
            ));
        }
    }
    let class = if entry.video {
        "object.item.videoItem"
    } else {
        "object.item.imageItem.photo"
    };
    format!(
        "<item id=\"i{}\" parentID=\"1\" restricted=\"1\"><dc:title>{}</dc:title><upnp:class>{class}</upnp:class><res protocolInfo=\"http-get:*:{}:*\"{attributes}>{}</res></item>",
        entry.id,
        escape(&entry.title),
        entry.mime,
        escape(&format!("{base}/{}", media_path(entry.id, entry.mime)))
    )
}

fn didl(items: &str) -> String {
    format!("{DIDL_OPEN}{items}</DIDL-Lite>")
}

pub struct Response {
    pub status: &'static str,
    pub body: String,
}

pub(super) fn fault(code: u32) -> Response {
    let description = match code {
        401 => "Invalid Action",
        402 => "Invalid Args",
        701 => "No Such Object",
        706 => "Invalid Connection Reference",
        709 => "Unsupported Sort Criteria",
        _ => "Action Failed",
    };
    Response {
        status: "500 Internal Server Error",
        body: format!(
            "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><s:Fault><faultcode>s:Client</faultcode><faultstring>UPnPError</faultstring><detail><UPnPError xmlns=\"urn:schemas-upnp-org:control-1-0\"><errorCode>{code}</errorCode><errorDescription>{description}</errorDescription></UPnPError></detail></s:Fault></s:Body></s:Envelope>"
        ),
    }
}

pub fn control(
    request: &Request,
    content_directory: bool,
    snapshot: &Snapshot,
    base: &str,
    update_id: u32,
    catalogue: Catalogue,
) -> Response {
    match control_inner(
        request,
        content_directory,
        snapshot,
        base,
        update_id,
        catalogue,
    ) {
        Ok(body) => Response {
            status: "200 OK",
            body,
        },
        Err(code) => fault(code),
    }
}

fn control_inner(
    request: &Request,
    directory: bool,
    snapshot: &Snapshot,
    base: &str,
    update_id: u32,
    catalogue: Catalogue,
) -> Result<String, u32> {
    let service = if directory {
        CONTENT_DIRECTORY
    } else {
        CONNECTION_MANAGER
    };
    let (requested_service, action) = request
        .header("soapaction")
        .ok_or(401u32)?
        .trim_matches('"')
        .rsplit_once('#')
        .ok_or(401u32)?;
    if requested_service != service {
        return Err(401);
    }
    let document = protocol::xml(std::str::from_utf8(&request.body).map_err(|_| 402u32)?)
        .map_err(|_| 402u32)?;
    let arguments = document
        .descendant("Body")
        .and_then(|body| body.child(action))
        .ok_or(402u32)?;
    let outputs: Vec<(&str, String)> = match (directory, action) {
        (true, "GetSearchCapabilities") => vec![("SearchCaps", String::new())],
        (true, "GetSortCapabilities") => vec![("SortCaps", "dc:title".into())],
        (true, "GetSystemUpdateID") => vec![("Id", update_id.to_string())],
        (true, "Browse") => {
            let flag = arguments.value("BrowseFlag");
            if !matches!(flag, "BrowseMetadata" | "BrowseDirectChildren") {
                return Err(402);
            }
            let start = arguments
                .value("StartingIndex")
                .parse::<u32>()
                .map_err(|_| 402u32)?;
            let count = arguments
                .value("RequestedCount")
                .parse::<u32>()
                .map_err(|_| 402u32)?;
            let sort = arguments.value("SortCriteria");
            if !matches!(sort, "" | "dc:title" | "+dc:title" | "-dc:title") {
                return Err(709);
            }
            let object = arguments.value("ObjectID");
            let (items, returned, total) = match (object, flag) {
                ("0", "BrowseMetadata") => (container("0", "-1", "Lap", 1), 1, 1),
                ("0", "BrowseDirectChildren") => {
                    let page = catalogue(snapshot, 0, 1, "", None).map_err(|_| 501u32)?;
                    if start > 0 {
                        (String::new(), 0, 1)
                    } else {
                        (container("1", "0", &snapshot.title, page.total), 1, 1)
                    }
                }
                ("1", "BrowseMetadata") => {
                    let page = catalogue(snapshot, 0, 1, "", None).map_err(|_| 501u32)?;
                    (container("1", "0", &snapshot.title, page.total), 1, 1)
                }
                ("1", "BrowseDirectChildren") => {
                    let page = catalogue(snapshot, start, count, sort, None).map_err(|_| 501u32)?;
                    (
                        page.entries.iter().map(|entry| item(entry, base)).collect(),
                        page.entries.len(),
                        page.total,
                    )
                }
                (_, "BrowseMetadata") => {
                    let id = object
                        .strip_prefix('i')
                        .and_then(|id| id.parse::<i64>().ok())
                        .filter(|id| snapshot.contains(*id))
                        .ok_or(701u32)?;
                    let page = catalogue(snapshot, 0, 1, "", Some(id)).map_err(|_| 501u32)?;
                    let entry = page.entries.first().ok_or(701u32)?;
                    (item(entry, base), 1, 1)
                }
                _ => return Err(701),
            };
            vec![
                ("Result", didl(&items)),
                ("NumberReturned", returned.to_string()),
                ("TotalMatches", total.to_string()),
                ("UpdateID", update_id.to_string()),
            ]
        }
        (false, "GetProtocolInfo") => vec![("Source", PROTOCOLS.into()), ("Sink", String::new())],
        (false, "GetCurrentConnectionIDs") => vec![("ConnectionIDs", "0".into())],
        (false, "GetCurrentConnectionInfo") => {
            if arguments.value("ConnectionID") != "0" {
                return Err(706);
            }
            vec![
                ("RcsID", "-1".into()),
                ("AVTransportID", "-1".into()),
                ("ProtocolInfo", "http-get:*:*:*".into()),
                ("PeerConnectionManager", String::new()),
                ("PeerConnectionID", "-1".into()),
                ("Direction", "Output".into()),
                ("Status", "OK".into()),
            ]
        }
        _ => return Err(401),
    };
    let outputs = outputs
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect::<Vec<_>>();
    protocol::envelope(service, &format!("{action}Response"), &outputs).map_err(|_| 501u32)
}

pub struct BrowseSpec {
    pub start: u32,
    pub count: u32,
    pub sort: String,
}

pub fn streamed_browse(request: &Request) -> Option<Result<BrowseSpec, u32>> {
    if request.header("soapaction")?.trim_matches('"') != format!("{CONTENT_DIRECTORY}#Browse") {
        return None;
    }
    let parse = || -> Result<Option<BrowseSpec>, u32> {
        let document = protocol::xml(std::str::from_utf8(&request.body).map_err(|_| 402u32)?)
            .map_err(|_| 402u32)?;
        let args = document
            .descendant("Body")
            .and_then(|body| body.child("Browse"))
            .ok_or(402u32)?;
        if args.value("ObjectID") != "1" || args.value("BrowseFlag") != "BrowseDirectChildren" {
            return Ok(None);
        }
        let start = args
            .value("StartingIndex")
            .parse::<u32>()
            .map_err(|_| 402u32)?;
        let count = args
            .value("RequestedCount")
            .parse::<u32>()
            .map_err(|_| 402u32)?;
        let sort = args.value("SortCriteria");
        if !matches!(sort, "" | "dc:title" | "+dc:title" | "-dc:title") {
            return Err(709);
        }
        Ok((count == 0 || count > 200).then(|| BrowseSpec {
            start,
            count,
            sort: sort.to_owned(),
        }))
    };
    match parse() {
        Ok(Some(spec)) => Some(Ok(spec)),
        Ok(None) => None,
        Err(code) => Some(Err(code)),
    }
}

pub fn browse_start() -> String {
    format!(
        "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:BrowseResponse xmlns:u=\"{CONTENT_DIRECTORY}\"><Result>{}",
        escape(DIDL_OPEN)
    )
}

pub fn browse_finish(returned: usize, total: usize, update_id: u32) -> String {
    format!(
        "{}</Result><NumberReturned>{returned}</NumberReturned><TotalMatches>{total}</TotalMatches><UpdateID>{update_id}</UpdateID></u:BrowseResponse></s:Body></s:Envelope>",
        escape("</DIDL-Lite>")
    )
}

pub fn escaped_item(entry: &Entry, base: &str) -> String {
    escape(&item(entry, base))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_server_description_advertises_its_library_and_sdk_event_endpoints() {
        let description = description(
            "test",
            "Lap & 家庭",
            "http://192.168.1.2:49152",
            "http://192.168.1.2:12345/test",
        );
        assert!(description.contains("Lap &amp; 家庭"));
        assert!(description.contains("http://192.168.1.2:49152/test/events/content-directory"));
        assert!(description.contains(
            "<controlURL>http://192.168.1.2:12345/test/control/content-directory</controlURL>"
        ));
        protocol::xml(&description).unwrap();
        protocol::xml(&scpd(true)).unwrap();
        protocol::xml(&scpd(false)).unwrap();
    }

    #[test]
    fn photo_metadata_does_not_advertise_the_original_raw_size_for_a_generated_jpeg() {
        let entry = Entry {
            id: 1,
            title: "RAW & 照片".into(),
            video: false,
            mime: "image/jpeg",
            size: 80_000_000,
            width: 8000,
            height: 6000,
            duration: 0,
        };
        let xml = item(&entry, "http://192.168.1.2/share");
        assert!(!xml.contains("size="));
        assert!(!xml.contains("resolution="));
        assert!(xml.contains("RAW &amp; 照片"));
        assert!(xml.contains("/share/media/1.jpg"));
        let mut entry = entry;
        entry.title = "bad\u{1} name & photo".into();
        protocol::xml(&didl(&item(&entry, "http://192.168.1.2/share"))).unwrap();
    }
}
