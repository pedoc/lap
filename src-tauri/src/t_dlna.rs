mod protocol;

use futures_util::{StreamExt, stream};
use once_cell::sync::Lazy;
use protocol::Renderer;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant, SystemTime},
};
use tauri::Manager;
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    sync::Semaphore,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DlnaDevice {
    id: String,
    name: String,
    address: String,
}

struct Discovered {
    device: DlnaDevice,
    renderer: Renderer,
    found: Instant,
}

#[derive(Default)]
struct Discovery {
    request_id: String,
    devices: Vec<Discovered>,
}

#[derive(Default)]
struct Registry {
    devices: HashMap<String, Discovery>,
    session: Option<Arc<Session>>,
}

static REGISTRY: Lazy<Mutex<Registry>> = Lazy::new(|| Mutex::new(Registry::default()));

struct Session {
    id: String,
    owner: String,
    library: String,
    renderer: Renderer,
    base_url: String,
    media: RwLock<Option<Arc<Media>>>,
    operation: tokio::sync::Mutex<()>,
    cancel: CancellationToken,
    closed: CancellationToken,
}

#[derive(Clone, Debug, PartialEq)]
struct Source {
    file_id: i64,
    path: PathBuf,
    size: u64,
    modified: Option<SystemTime>,
}

struct Media {
    source: Source,
    title: String,
    mime: &'static str,
    path: String,
    bytes: Option<Vec<u8>>,
}

impl Media {
    fn size(&self) -> u64 {
        self.bytes
            .as_ref()
            .map_or(self.source.size, |bytes| bytes.len() as u64)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CastResult {
    title: String,
    kind: &'static str,
}

#[tauri::command]
pub async fn dlna_discover(window: tauri::WebviewWindow) -> Result<Vec<DlnaDevice>, String> {
    let owner = window.label().to_owned();
    let request_id = uuid::Uuid::new_v4().to_string();
    REGISTRY
        .lock()
        .map_err(|_| "投屏服务不可用")?
        .devices
        .entry(owner.clone())
        .or_default()
        .request_id = request_id.clone();
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .await
        .map_err(|_| "无法搜索局域网电视")?;
    socket
        .set_multicast_ttl_v4(2)
        .map_err(|_| "无法搜索局域网电视")?;
    for target in [
        "urn:schemas-upnp-org:device:MediaRenderer:1",
        "urn:schemas-upnp-org:service:AVTransport:1",
    ] {
        let request = format!(
            "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: {target}\r\n\r\n"
        );
        socket
            .send_to(
                request.as_bytes(),
                (Ipv4Addr::new(239, 255, 255, 250), 1900),
            )
            .await
            .map_err(|_| "无法发送局域网设备搜索请求")?;
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let mut packet = [0u8; 8192];
    let mut locations = Vec::new();
    let mut seen = HashSet::new();
    while locations.len() < 32 && tokio::time::Instant::now() < deadline {
        let Ok(Ok((length, sender))) =
            tokio::time::timeout_at(deadline, socket.recv_from(&mut packet)).await
        else {
            break;
        };
        let IpAddr::V4(address) = sender.ip() else {
            continue;
        };
        if let Some(location) = protocol::discovery_location(&packet[..length], address) {
            if seen.insert(location.to_string()) {
                locations.push((location, address));
            }
        }
    }
    let client = protocol::client()?;
    let mut renderers = stream::iter(locations)
        .map(|(location, address)| {
            let client = client.clone();
            async move {
                let response = client
                    .get(location.clone())
                    .send()
                    .await
                    .ok()?
                    .error_for_status()
                    .ok()?;
                let text = protocol::response_text(response).await.ok()?;
                protocol::parse_renderer(&text, &location, address).ok()
            }
        })
        .buffer_unordered(6)
        .filter_map(|renderer| async move { renderer })
        .collect::<Vec<_>>()
        .await;
    renderers.sort_by(|a, b| a.name.cmp(&b.name));
    renderers.dedup_by(|a, b| a.control_url == b.control_url);
    let discovered = renderers
        .into_iter()
        .map(|renderer| Discovered {
            device: DlnaDevice {
                id: uuid::Uuid::new_v4().to_string(),
                name: renderer.name.clone(),
                address: renderer.address.to_string(),
            },
            renderer,
            found: Instant::now(),
        })
        .collect::<Vec<_>>();
    let devices = discovered.iter().map(|item| item.device.clone()).collect();
    let mut registry = REGISTRY.lock().map_err(|_| "投屏服务不可用")?;
    if window
        .app_handle()
        .get_webview_window(window.label())
        .is_some()
    {
        update_discovery(&mut registry, &owner, &request_id, discovered);
    } else {
        registry.devices.remove(&owner);
    }
    Ok(devices)
}

fn update_discovery(
    registry: &mut Registry,
    owner: &str,
    request_id: &str,
    devices: Vec<Discovered>,
) {
    if let Some(discovery) = registry
        .devices
        .get_mut(owner)
        .filter(|discovery| discovery.request_id == request_id)
    {
        discovery.devices = devices;
    }
}

#[tauri::command]
pub async fn dlna_connect(
    window: tauri::WebviewWindow,
    library_id: String,
    device_id: String,
) -> Result<String, String> {
    let owner = window.label().to_owned();
    let renderer = REGISTRY
        .lock()
        .map_err(|_| "投屏服务不可用")?
        .devices
        .get(&owner)
        .and_then(|devices| {
            devices.devices.iter().find(|item| {
                item.device.id == device_id && item.found.elapsed() < Duration::from_secs(180)
            })
        })
        .map(|item| item.renderer.clone())
        .ok_or("设备已过期，请重新搜索电视")?;
    let library = library_id.clone();
    tokio::task::spawn_blocking(move || crate::t_cmds::with_current_library(&library, || Ok(())))
        .await
        .map_err(|e| e.to_string())??;
    let route = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .await
        .map_err(|_| "无法连接局域网")?;
    route
        .connect((renderer.address, 9))
        .await
        .map_err(|_| "无法确定电视所在的网络")?;
    let address = route
        .local_addr()
        .map_err(|_| "无法确定本机局域网地址")?
        .ip();
    let listener = TcpListener::bind((address, 0))
        .await
        .map_err(|_| "无法启动投屏媒体服务")?;
    let port = listener
        .local_addr()
        .map_err(|_| "无法读取投屏服务端口")?
        .port();
    let session = Arc::new(Session {
        id: uuid::Uuid::new_v4().to_string(),
        owner,
        library: library_id,
        renderer,
        base_url: format!("http://{address}:{port}"),
        media: RwLock::new(None),
        operation: tokio::sync::Mutex::new(()),
        cancel: CancellationToken::new(),
        closed: CancellationToken::new(),
    });
    {
        let mut registry = REGISTRY.lock().map_err(|_| "投屏服务不可用")?;
        if registry.session.is_some() {
            return Err("已有投屏会话，请先停止当前投屏".into());
        }
        registry.session = Some(session.clone());
    }
    let id = session.id.clone();
    tauri::async_runtime::spawn(serve(listener, session, window.app_handle().clone()));
    Ok(id)
}

fn session(owner: &str, id: &str) -> Result<Arc<Session>, String> {
    REGISTRY
        .lock()
        .map_err(|_| "投屏服务不可用")?
        .session
        .as_ref()
        .filter(|session| {
            session.owner == owner && session.id == id && !session.cancel.is_cancelled()
        })
        .cloned()
        .ok_or_else(|| "投屏会话已结束，请重新连接电视".into())
}

fn load_source(library: &str, file_id: i64) -> Result<(Source, crate::t_sqlite::AFile), String> {
    crate::t_cmds::with_current_library(library, || {
        if crate::t_media_lifecycle::is_private_file(file_id)? {
            return Err("隐藏或已删除的媒体不能投屏".into());
        }
        let file = crate::t_sqlite::AFile::get_file_info(file_id)?.ok_or("媒体已不存在")?;
        if file.cloud_only {
            return Err("请先将原文件下载到本机再投屏".into());
        }
        let path = PathBuf::from(file.file_path.as_deref().ok_or("媒体路径不可用")?);
        if crate::t_utils::is_cloud_only_path(&path.to_string_lossy())
            || crate::t_media_lifecycle::is_recovery_path(&path)
        {
            return Err("媒体原文件不可用或不在可共享范围内".into());
        }
        let path = path.canonicalize().map_err(|_| "媒体原文件不可用")?;
        let metadata = path.metadata().map_err(|_| "无法读取媒体原文件")?;
        if !metadata.is_file() {
            return Err("媒体原文件不可用".into());
        }
        Ok((
            Source {
                file_id,
                path,
                size: metadata.len(),
                modified: metadata.modified().ok(),
            },
            file,
        ))
    })
}

fn verify_source(library: &str, source: &Source) -> Result<(), String> {
    if load_source(library, source.file_id)?.0 != *source {
        return Err("媒体已变化，请重新选择".into());
    }
    Ok(())
}

fn prepare_photo(
    path: &std::path::Path,
    raw: bool,
    orientation: i32,
    rotate: i32,
) -> Result<Vec<u8>, String> {
    let path = path.to_string_lossy();
    let preview = if raw {
        crate::t_image::get_raw_thumbnail(
            &path,
            orientation,
            3840,
            crate::t_raw_display::RawDisplayOptions::default(),
        )?
    } else {
        crate::t_image::get_image_thumbnail(&path, orientation, 3840)?
    }
    .ok_or("无法生成这张照片的投屏画面")?;
    if preview.len() > 64 * 1024 * 1024 {
        return Err("照片预览过大，无法投屏".into());
    }
    let mut image = image::load_from_memory(&preview).map_err(|_| "无法读取照片预览")?;
    image = match rotate.rem_euclid(360) {
        90 => image.rotate90(),
        180 => image.rotate180(),
        270 => image.rotate270(),
        _ => image,
    };
    if image.width() > 3840 || image.height() > 2160 {
        image = image.thumbnail(3840, 2160);
    }
    crate::t_jpeg::encode_rgb8(&image.to_rgb8(), 90).map_err(|_| "无法编码投屏照片".into())
}

fn prepare_media(library: &str, file_id: i64) -> Result<Media, String> {
    let (source, file) = load_source(library, file_id)?;
    let title = file
        .title
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(file.name);
    let (mime, bytes) = match file.file_type {
        Some(2) => (
            protocol::video_mime(&source.path)
                .ok_or("暂不支持此视频容器，请先转换为电视支持的 MP4 视频")?,
            None,
        ),
        Some(1 | 3) => {
            let bytes = prepare_photo(
                &source.path,
                file.file_type == Some(3),
                file.e_orientation.unwrap_or(1) as i32,
                file.rotate.unwrap_or(0),
            )?;
            ("image/jpeg", Some(bytes))
        }
        _ => return Err("只能投屏照片或视频".into()),
    };
    verify_source(library, &source)?;
    let extension = if bytes.is_some() {
        "jpg"
    } else {
        source
            .path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("mp4")
    };
    let path = format!("/media/{}.{}", uuid::Uuid::new_v4(), extension);
    Ok(Media {
        source,
        title,
        mime,
        path,
        bytes,
    })
}

#[tauri::command]
pub async fn dlna_cast(
    window: tauri::WebviewWindow,
    session_id: String,
    file_id: i64,
) -> Result<CastResult, String> {
    let session = session(window.label(), &session_id)?;
    let _operation = session.operation.lock().await;
    let library = session.library.clone();
    let media = tokio::select! {
        _ = session.cancel.cancelled() => return Err("投屏已取消".into()),
        result = tokio::task::spawn_blocking(move || prepare_media(&library, file_id)) => Arc::new(result.map_err(|e| e.to_string())??),
    };
    let client = protocol::client()?;
    let url = format!("{}{}", session.base_url, media.path);
    let metadata = protocol::metadata(&media.title, &url, media.mime, media.size());
    let replacing = session
        .media
        .write()
        .map_err(|_| "投屏服务不可用")?
        .replace(media.clone())
        .is_some();
    let result = tokio::select! {
        _ = session.cancel.cancelled() => Err("投屏已取消".into()),
        result = async {
            if replacing { let _ = protocol::action(&client, &session.renderer, "Stop", &[("InstanceID", "0")]).await; }
            protocol::action(&client, &session.renderer, "SetAVTransportURI", &[("InstanceID", "0"), ("CurrentURI", &url), ("CurrentURIMetaData", &metadata)]).await?;
            protocol::action(&client, &session.renderer, "Play", &[("InstanceID", "0"), ("Speed", "1")]).await
        } => result,
    };
    if result.is_err() {
        *session.media.write().map_err(|_| "投屏服务不可用")? = None;
    }
    result?;
    Ok(CastResult {
        title: media.title.clone(),
        kind: if media.bytes.is_some() {
            "image"
        } else {
            "video"
        },
    })
}

#[tauri::command]
pub async fn dlna_control(
    window: tauri::WebviewWindow,
    session_id: String,
    command: String,
) -> Result<(), String> {
    if !matches!(command.as_str(), "Play" | "Pause") {
        return Err("不支持的投屏操作".into());
    }
    let session = session(window.label(), &session_id)?;
    let _operation = session.operation.lock().await;
    if session
        .media
        .read()
        .map_err(|_| "投屏服务不可用")?
        .is_none()
    {
        return Err("请先选择媒体播放".into());
    }
    let client = protocol::client()?;
    let mut arguments = vec![("InstanceID", "0")];
    if command == "Play" {
        arguments.push(("Speed", "1"));
    }
    tokio::select! {
        _ = session.cancel.cancelled() => Err("投屏已取消".into()),
        result = protocol::action(&client, &session.renderer, &command, &arguments) => result,
    }
}

#[tauri::command]
pub async fn dlna_disconnect(
    window: tauri::WebviewWindow,
    session_id: String,
) -> Result<(), String> {
    let session = REGISTRY
        .lock()
        .map_err(|_| "投屏服务不可用")?
        .session
        .as_ref()
        .filter(|session| session.owner == window.label() && session.id == session_id)
        .cloned();
    if let Some(session) = session {
        session.cancel.cancel();
        tokio::time::timeout(Duration::from_secs(12), session.closed.cancelled())
            .await
            .map_err(|_| "媒体共享已关闭，电视停止响应超时")?;
    }
    Ok(())
}

pub fn release_owner(owner: &str) {
    if let Ok(mut registry) = REGISTRY.lock() {
        registry.devices.remove(owner);
        if let Some(session) = registry
            .session
            .as_ref()
            .filter(|session| session.owner == owner)
        {
            session.cancel.cancel();
        }
    }
}

async fn serve(listener: TcpListener, session: Arc<Session>, app: tauri::AppHandle) {
    let connections = Arc::new(Semaphore::new(4));
    let mut watchdog = tokio::time::interval(Duration::from_secs(2));
    loop {
        tokio::select! {
            _ = session.cancel.cancelled() => break,
            _ = watchdog.tick() => {
                if app.get_webview_window(&session.owner).is_none() { break; }
                let library = session.library.clone();
                let source = session.media.read().ok().and_then(|media| media.as_ref().map(|media| media.source.clone()));
                let check = tokio::select! {
                    _ = session.cancel.cancelled() => break,
                    result = tokio::task::spawn_blocking(move || {
                    if let Some(source) = source { verify_source(&library, &source) }
                    else { crate::t_cmds::with_current_library(&library, || Ok(())) }
                    }) => result,
                };
                if !matches!(check, Ok(Ok(()))) { break; }
            }
            connection = listener.accept() => {
                let Ok((stream, peer)) = connection else { break; };
                if peer.ip() != IpAddr::V4(session.renderer.address) { continue; }
                let Ok(permit) = connections.clone().try_acquire_owned() else { continue; };
                let session = session.clone();
                tauri::async_runtime::spawn(async move {
                    let _permit = permit;
                    tokio::select! {
                        _ = session.cancel.cancelled() => {},
                        _ = serve_media(stream, &session, verify_source) => {},
                    }
                });
            }
        }
    }
    session.cancel.cancel();
    // Revoke media before waiting for a slow or disconnected television.
    if let Ok(mut media) = session.media.write() {
        *media = None;
    }
    drop(listener);
    let _operation = session.operation.lock().await;
    if let Ok(client) = protocol::client() {
        let _ = protocol::action(&client, &session.renderer, "Stop", &[("InstanceID", "0")]).await;
    }
    if let Ok(mut registry) = REGISTRY.lock() {
        if registry
            .session
            .as_ref()
            .is_some_and(|active| active.id == session.id)
        {
            registry.session = None;
        }
    }
    session.closed.cancel();
}

async fn headers(
    stream: &mut TcpStream,
    status: &str,
    values: &[(&str, String)],
) -> std::io::Result<()> {
    let mut text = format!("HTTP/1.1 {status}\r\nConnection: close\r\nCache-Control: no-store\r\n");
    for (key, value) in values {
        text.push_str(&format!("{key}: {value}\r\n"));
    }
    text.push_str("\r\n");
    tokio::time::timeout(Duration::from_secs(15), stream.write_all(text.as_bytes())).await??;
    Ok(())
}

async fn serve_media(
    mut stream: TcpStream,
    session: &Session,
    verify: fn(&str, &Source) -> Result<(), String>,
) -> std::io::Result<()> {
    let mut request = Vec::new();
    let read = async {
        let mut buffer = [0u8; 2048];
        while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            let count = stream.read(&mut buffer).await?;
            if count == 0 || request.len() + count > 16 * 1024 {
                return Err(std::io::ErrorKind::InvalidData.into());
            }
            request.extend_from_slice(&buffer[..count]);
        }
        Ok::<_, std::io::Error>(())
    };
    tokio::time::timeout(Duration::from_secs(5), read).await??;
    let request = String::from_utf8_lossy(&request);
    let mut lines = request.split("\r\n");
    let first = lines
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>();
    if first.len() != 3 || !matches!(first[0], "GET" | "HEAD") {
        return headers(
            &mut stream,
            "405 Method Not Allowed",
            &[
                ("Content-Length", "0".into()),
                ("Allow", "GET, HEAD".into()),
            ],
        )
        .await;
    }
    let media = session.media.read().ok().and_then(|media| media.clone());
    let Some(media) = media.filter(|media| media.path == first[1]) else {
        return headers(
            &mut stream,
            "404 Not Found",
            &[("Content-Length", "0".into())],
        )
        .await;
    };
    let library = session.library.clone();
    let source = media.source.clone();
    if !matches!(
        tokio::task::spawn_blocking(move || verify(&library, &source)).await,
        Ok(Ok(()))
    ) {
        return headers(
            &mut stream,
            "404 Not Found",
            &[("Content-Length", "0".into())],
        )
        .await;
    }
    let range = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("range"))
        .map(|(_, value)| value.trim());
    let size = media.size();
    let Ok((start, length, partial)) = protocol::byte_range(range, size) else {
        return headers(
            &mut stream,
            "416 Range Not Satisfiable",
            &[
                ("Content-Length", "0".into()),
                ("Content-Range", format!("bytes */{size}")),
            ],
        )
        .await;
    };
    let mut file = if media.bytes.is_none() {
        let file = tokio::fs::File::open(&media.source.path).await?;
        let metadata = file.metadata().await?;
        if metadata.len() != media.source.size || metadata.modified().ok() != media.source.modified
        {
            return headers(
                &mut stream,
                "404 Not Found",
                &[("Content-Length", "0".into())],
            )
            .await;
        }
        Some(file)
    } else {
        None
    };
    let mut values = vec![
        ("Content-Type", media.mime.into()),
        ("Content-Length", length.to_string()),
        ("Accept-Ranges", "bytes".into()),
        (
            "transferMode.dlna.org",
            if media.bytes.is_some() {
                "Interactive".into()
            } else {
                "Streaming".into()
            },
        ),
        (
            "contentFeatures.dlna.org",
            format!(
                "DLNA.ORG_OP=01;DLNA.ORG_CI={}",
                if media.bytes.is_some() { 1 } else { 0 }
            ),
        ),
    ];
    if partial {
        values.push((
            "Content-Range",
            format!("bytes {start}-{}/{size}", start + length - 1),
        ));
    }
    headers(
        &mut stream,
        if partial {
            "206 Partial Content"
        } else {
            "200 OK"
        },
        &values,
    )
    .await?;
    if first[0] == "HEAD" {
        return Ok(());
    }
    if let Some(bytes) = &media.bytes {
        tokio::time::timeout(
            Duration::from_secs(30),
            stream.write_all(&bytes[start as usize..(start + length) as usize]),
        )
        .await??;
    } else if let Some(file) = file.as_mut() {
        file.seek(std::io::SeekFrom::Start(start)).await?;
        let mut buffer = vec![0u8; 64 * 1024];
        let mut remaining = length;
        while remaining > 0 {
            if !session.media.read().ok().is_some_and(|current| {
                current
                    .as_ref()
                    .is_some_and(|current| current.path == media.path)
            }) {
                break;
            }
            let wanted = remaining.min(buffer.len() as u64) as usize;
            let count = file.read(&mut buffer[..wanted]).await?;
            if count == 0 {
                break;
            }
            tokio::time::timeout(Duration::from_secs(30), stream.write_all(&buffer[..count]))
                .await??;
            remaining -= count as u64;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn late_device_discovery_cannot_invalidate_newer_choices_or_restore_a_closed_window() {
        let device = || Discovered {
            device: DlnaDevice {
                id: "new-choice".into(),
                name: "TV".into(),
                address: "192.168.1.10".into(),
            },
            renderer: Renderer {
                name: "TV".into(),
                address: Ipv4Addr::new(192, 168, 1, 10),
                control_url: reqwest::Url::parse("http://192.168.1.10/control").unwrap(),
                service_type: format!("{}1", protocol::AV_TRANSPORT),
            },
            found: Instant::now(),
        };
        let mut registry = Registry::default();
        registry.devices.insert(
            "window".into(),
            Discovery {
                request_id: "new-search".into(),
                devices: Vec::new(),
            },
        );
        update_discovery(&mut registry, "window", "new-search", vec![device()]);
        update_discovery(&mut registry, "window", "old-search", Vec::new());
        assert_eq!(
            registry.devices["window"].devices[0].device.id,
            "new-choice"
        );
        registry.devices.remove("window");
        update_discovery(&mut registry, "window", "new-search", vec![device()]);
        assert!(!registry.devices.contains_key("window"));
    }

    #[test]
    fn photo_preparation_applies_orientation_and_catalog_rotation_without_writing_originals() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("photo.png");
        let image = image::RgbImage::from_fn(80, 40, |_, y| {
            if y < 20 {
                image::Rgb([240, 10, 10])
            } else {
                image::Rgb([10, 10, 240])
            }
        });
        image.save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let bytes = prepare_photo(&path, false, 6, 90).unwrap();
        assert!(bytes.starts_with(&[0xff, 0xd8]));
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgb8();
        assert_eq!(decoded.dimensions(), (80, 40));
        let pixel = decoded.get_pixel(10, 5);
        assert!(pixel[2] > 200 && pixel[0] < 40, "{pixel:?}");
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let rotated =
            image::load_from_memory(&prepare_photo(&path, false, 1, 90).unwrap()).unwrap();
        assert_eq!((rotated.width(), rotated.height()), (40, 80));
    }

    fn test_session(path: &std::path::Path, bytes: Option<Vec<u8>>) -> Arc<Session> {
        let metadata = path.metadata().unwrap();
        let mime = if bytes.is_some() {
            "image/jpeg"
        } else {
            "video/mp4"
        };
        Arc::new(Session {
            id: "test-session".into(),
            owner: "test-window".into(),
            library: "test-library".into(),
            renderer: Renderer {
                name: "Test TV".into(),
                address: Ipv4Addr::LOCALHOST,
                control_url: reqwest::Url::parse("http://127.0.0.1/control").unwrap(),
                service_type: format!("{}1", protocol::AV_TRANSPORT),
            },
            base_url: "http://127.0.0.1".into(),
            media: RwLock::new(Some(Arc::new(Media {
                source: Source {
                    file_id: 1,
                    path: path.to_owned(),
                    size: metadata.len(),
                    modified: metadata.modified().ok(),
                },
                title: "Test media".into(),
                mime,
                path: "/media/unguessable-ticket.mp4".into(),
                bytes,
            }))),
            operation: tokio::sync::Mutex::new(()),
            cancel: CancellationToken::new(),
            closed: CancellationToken::new(),
        })
    }

    async fn request(
        session: Arc<Session>,
        request: &str,
        verify: fn(&str, &Source) -> Result<(), String>,
    ) -> Vec<u8> {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::select! {
                _ = session.cancel.cancelled() => {},
                result = serve_media(stream, &session, verify) => { result.unwrap(); },
            }
        });
        let mut client = TcpStream::connect(address).await.unwrap();
        client.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();
        response
    }

    fn body(response: &[u8]) -> &[u8] {
        &response[response
            .windows(4)
            .position(|value| value == b"\r\n\r\n")
            .unwrap()
            + 4..]
    }

    #[tokio::test]
    async fn television_can_head_seek_and_stream_original_video_without_loading_it_all() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let bytes = (0..160_000)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        file.write_all(&bytes).unwrap();
        file.flush().unwrap();
        let session = test_session(file.path(), None);
        let response = request(
            session.clone(),
            "GET /media/unguessable-ticket.mp4 HTTP/1.1\r\nRange: bytes=500-140000\r\n\r\n",
            |_, _| Ok(()),
        )
        .await;
        assert!(response.starts_with(b"HTTP/1.1 206"));
        assert!(
            String::from_utf8_lossy(&response).contains("Content-Range: bytes 500-140000/160000")
        );
        assert_eq!(body(&response), &bytes[500..140001]);
        let response = request(
            session.clone(),
            "HEAD /media/unguessable-ticket.mp4 HTTP/1.1\r\nrange: bytes=-10\r\n\r\n",
            |_, _| Ok(()),
        )
        .await;
        assert!(String::from_utf8_lossy(&response).contains("Content-Length: 10\r\n"));
        assert!(body(&response).is_empty());
        let response = request(
            session,
            "GET /media/unguessable-ticket.mp4 HTTP/1.1\r\nRange: bytes=160000-\r\n\r\n",
            |_, _| Ok(()),
        )
        .await;
        assert!(response.starts_with(b"HTTP/1.1 416"));
        assert!(body(&response).is_empty());
    }

    #[tokio::test]
    async fn private_changed_or_revoked_media_and_guessed_paths_never_return_bytes() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"original").unwrap();
        file.flush().unwrap();
        let session = test_session(file.path(), None);
        for path in [
            "/media/wrong-ticket.mp4",
            "/../../original",
            "/media/unguessable-ticket.mp4?file=/original",
        ] {
            let response = request(
                session.clone(),
                &format!("GET {path} HTTP/1.1\r\n\r\n"),
                |_, _| Ok(()),
            )
            .await;
            assert!(response.starts_with(b"HTTP/1.1 404"));
            assert!(body(&response).is_empty());
        }
        let response = request(
            session.clone(),
            "GET /media/unguessable-ticket.mp4 HTTP/1.1\r\n\r\n",
            |_, _| Err("Media is now hidden or the library changed".into()),
        )
        .await;
        assert!(response.starts_with(b"HTTP/1.1 404"));
        assert!(body(&response).is_empty());
        file.write_all(b"changed").unwrap();
        file.flush().unwrap();
        let response = request(
            session.clone(),
            "GET /media/unguessable-ticket.mp4 HTTP/1.1\r\n\r\n",
            |_, _| Ok(()),
        )
        .await;
        assert!(response.starts_with(b"HTTP/1.1 404"));
        assert!(body(&response).is_empty());
        *session.media.write().unwrap() = None;
        let response = request(
            session,
            "GET /media/unguessable-ticket.mp4 HTTP/1.1\r\n\r\n",
            |_, _| Ok(()),
        )
        .await;
        assert!(response.starts_with(b"HTTP/1.1 404"));
        assert!(body(&response).is_empty());
    }

    #[tokio::test]
    async fn jpeg_previews_support_ranges_and_keep_the_original_file_unchanged() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"original image").unwrap();
        file.flush().unwrap();
        let preview = b"jpeg preview bytes".to_vec();
        let session = test_session(file.path(), Some(preview.clone()));
        let response = request(
            session,
            "GET /media/unguessable-ticket.mp4 HTTP/1.1\r\nRange: bytes=-5\r\n\r\n",
            |_, _| Ok(()),
        )
        .await;
        assert_eq!(body(&response), &preview[preview.len() - 5..]);
        assert!(String::from_utf8_lossy(&response).contains("Content-Type: image/jpeg"));
        assert_eq!(std::fs::read(file.path()).unwrap(), b"original image");
    }
}
