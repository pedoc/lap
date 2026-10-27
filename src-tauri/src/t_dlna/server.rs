use super::{Media, Source, http, protocol, scope, upnp};
use once_cell::sync::Lazy;
use serde::Serialize;
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{Listener, Manager};
use tokio::{
    net::{TcpListener, TcpStream, UdpSocket},
    sync::{Notify, Semaphore},
};
use tokio_util::sync::CancellationToken;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shared {
    session_id: String,
    count: usize,
    scope_name: String,
    server_name: String,
}

#[derive(Clone)]
struct Subscription {
    callback: reqwest::Url,
    peer: Ipv4Addr,
    expires: Instant,
    directory: bool,
    sequence: u32,
}

struct Server {
    id: String,
    owner: String,
    name: String,
    scope: Arc<scope::Snapshot>,
    port: u16,
    active: AtomicBool,
    activated: Notify,
    cancel: CancellationToken,
    closed: CancellationToken,
    cache: tokio::sync::Mutex<Option<Arc<Media>>>,
    subscriptions: Mutex<HashMap<String, Subscription>>,
    updates: Notify,
    update_id: AtomicU32,
    catalogue: upnp::Catalogue,
    stream_catalogue: upnp::StreamCatalogue,
    prepare: fn(&str, i64) -> Result<Media, String>,
    verify: fn(&str, &Source) -> Result<(), String>,
}

static ACTIVE: Lazy<Mutex<Option<Arc<Server>>>> = Lazy::new(|| Mutex::new(None));
const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);

fn multicast() -> Result<UdpSocket, String> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )
    .map_err(|_| "无法创建 DLNA 发现服务")?;
    socket
        .set_reuse_address(true)
        .map_err(|_| "无法配置 DLNA 发现服务")?;
    socket
        .bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, 1900)).into())
        .map_err(|_| "无法监听 DLNA 发现端口 1900，请检查其他媒体服务和防火墙")?;
    socket
        .join_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED)
        .map_err(|_| "无法加入 DLNA 局域网发现组")?;
    socket
        .set_multicast_ttl_v4(2)
        .map_err(|_| "无法配置 DLNA 发现服务")?;
    socket
        .set_nonblocking(true)
        .map_err(|_| "无法配置 DLNA 发现服务")?;
    UdpSocket::from_std(socket.into()).map_err(|_| "无法启动 DLNA 发现服务".into())
}

pub async fn start(
    window: tauri::WebviewWindow,
    library_id: String,
    selection: scope::Scope,
) -> Result<Shared, String> {
    let scope = Arc::new(
        tokio::task::spawn_blocking(move || scope::resolve(&library_id, selection))
            .await
            .map_err(|e| e.to_string())??,
    );
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))
        .await
        .map_err(|_| "无法启动 DLNA 媒体目录服务")?;
    let port = listener
        .local_addr()
        .map_err(|_| "无法读取媒体目录服务端口")?
        .port();
    let socket = multicast()?;
    let server = Arc::new(Server {
        id: uuid::Uuid::new_v4().to_string(),
        owner: window.label().to_owned(),
        name: format!(
            "Lap · {}",
            scope.title.chars().take(100).collect::<String>()
        ),
        scope,
        port,
        active: AtomicBool::new(false),
        activated: Notify::new(),
        cancel: CancellationToken::new(),
        closed: CancellationToken::new(),
        cache: tokio::sync::Mutex::new(None),
        subscriptions: Mutex::new(HashMap::new()),
        updates: Notify::new(),
        update_id: AtomicU32::new(1),
        catalogue: scope::page,
        stream_catalogue: scope::stream_page,
        prepare: super::prepare_media,
        verify: super::verify_source,
    });
    {
        let mut active = ACTIVE.lock().map_err(|_| "共享服务不可用")?;
        if active.is_some() {
            return Err("已有电视浏览共享，请先停止当前共享".into());
        }
        *active = Some(server.clone());
    }
    let result = Shared {
        session_id: server.id.clone(),
        count: server.scope.file_ids.len(),
        scope_name: server.scope.title.clone(),
        server_name: server.name.clone(),
    };
    tauri::async_runtime::spawn(run(listener, socket, server, window.app_handle().clone()));
    Ok(result)
}

pub fn activate(owner: &str, id: &str) -> Result<(), String> {
    let server = ACTIVE
        .lock()
        .map_err(|_| "共享服务不可用")?
        .as_ref()
        .filter(|server| server.id == id && server.owner == owner && !server.cancel.is_cancelled())
        .cloned()
        .ok_or("共享会话已结束")?;
    server.active.store(true, Ordering::Release);
    server.activated.notify_one();
    Ok(())
}

pub async fn disconnect(owner: &str, id: &str) -> Result<(), String> {
    let server = ACTIVE
        .lock()
        .map_err(|_| "共享服务不可用")?
        .as_ref()
        .filter(|server| server.id == id && server.owner == owner)
        .cloned();
    if let Some(server) = server {
        server.cancel.cancel();
        tokio::time::timeout(Duration::from_secs(8), server.closed.cancelled())
            .await
            .map_err(|_| "媒体访问已撤销，停止发现通知超时")?;
    }
    Ok(())
}

pub fn release_owner(owner: &str) {
    if let Ok(active) = ACTIVE.lock() {
        if let Some(server) = active.as_ref().filter(|server| server.owner == owner) {
            server.cancel.cancel();
        }
    }
}

async fn route_address(peer: Ipv4Addr) -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await.ok()?;
    socket.connect((peer, 9)).await.ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(address) => Some(address),
        _ => None,
    }
}

async fn notify_presence(socket: &UdpSocket, server: &Server, alive: bool) {
    let Some(address) = route_address(GROUP).await else {
        return;
    };
    let location = format!(
        "http://{address}:{}/{}/description.xml",
        server.port, server.id
    );
    for (target, usn) in upnp::targets(&server.id) {
        if alive && server.cancel.is_cancelled() {
            break;
        }
        let text = upnp::advertisement(&target, &usn, &location, alive);
        let _ = tokio::time::timeout(
            Duration::from_millis(250),
            socket.send_to(text.as_bytes(), (GROUP, 1900)),
        )
        .await;
    }
}

async fn discovery(socket: UdpSocket, server: Arc<Server>) {
    if !server.active.load(Ordering::Acquire) {
        tokio::select! {_=server.cancel.cancelled()=>return,_=server.activated.notified()=>{}}
    }
    if server.cancel.is_cancelled() {
        return;
    }
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    let mut buffer = [0u8; 8192];
    loop {
        tokio::select! {
            _=server.cancel.cancelled()=>break,
            _=interval.tick()=>notify_presence(&socket,&server,true).await,
            packet=socket.recv_from(&mut buffer)=>{
                let Ok((length,peer))=packet else {break;};
                let IpAddr::V4(peer_ip)=peer.ip() else {continue;};
                if !protocol::lan_address(peer_ip) {continue;}
                let targets=upnp::search_targets(&buffer[..length],&server.id);
                if targets.is_empty() {continue;}
                let Some(address)=route_address(peer_ip).await else {continue;};
                let location=format!("http://{address}:{}/{}/description.xml",server.port,server.id);
                for (target,usn) in targets {
                    let text=upnp::search_response(&target,&usn,&location);
                    let _=tokio::time::timeout(Duration::from_millis(250),socket.send_to(text.as_bytes(),peer)).await;
                }
            }
        }
    }
    notify_presence(&socket, &server, false).await;
}

async fn events(server: Arc<Server>) {
    let Ok(client) = protocol::client() else {
        return;
    };
    let Ok(method) = reqwest::Method::from_bytes(b"NOTIFY") else {
        return;
    };
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    loop {
        tokio::select! {
            _=server.cancel.cancelled()=>break,
            _=interval.tick()=>{if let Ok(mut subscriptions)=server.subscriptions.lock(){subscriptions.retain(|_,sub|sub.expires>Instant::now());}continue;},
            _=server.updates.notified()=>{},
        }
        let batch = if let Ok(mut subscriptions) = server.subscriptions.lock() {
            subscriptions.retain(|_, sub| sub.expires > Instant::now());
            subscriptions
                .iter_mut()
                .map(|(sid, sub)| {
                    let result = (sid.clone(), sub.clone());
                    sub.sequence = if sub.sequence == u32::MAX {
                        1
                    } else {
                        sub.sequence + 1
                    };
                    result
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        for (sid, sub) in batch {
            let notification = client
                .request(method.clone(), sub.callback)
                .timeout(Duration::from_secs(3))
                .header("Content-Type", "text/xml; charset=\"utf-8\"")
                .header("NT", "upnp:event")
                .header("NTS", "upnp:propchange")
                .header("SID", sid)
                .header("SEQ", sub.sequence.to_string())
                .body(upnp::event(
                    sub.directory,
                    server.update_id.load(Ordering::Acquire),
                ))
                .send();
            tokio::select! {_=server.cancel.cancelled()=>return,_=notification=>{}}
        }
    }
}

async fn run(listener: TcpListener, socket: UdpSocket, server: Arc<Server>, app: tauri::AppHandle) {
    let discovery = tauri::async_runtime::spawn(discovery(socket, server.clone()));
    let event_task = tauri::async_runtime::spawn(events(server.clone()));
    let listeners = [
        "media-lifecycle-changed",
        "files-deleted",
        "media-title-changed",
        "index_finished",
        "album-accessibility-changed",
    ]
    .into_iter()
    .map(|event| {
        let server = server.clone();
        app.listen_any(event, move |_| {
            server.update_id.fetch_add(1, Ordering::AcqRel);
            server.updates.notify_one();
        })
    })
    .collect::<Vec<_>>();
    let permits = Arc::new(Semaphore::new(8));
    let mut watchdog = tokio::time::interval(Duration::from_secs(2));
    let prepared = Instant::now();
    loop {
        tokio::select! {
            _=server.cancel.cancelled()=>break,
            _=watchdog.tick()=>{
                if app.get_webview_window(&server.owner).is_none() || (!server.active.load(Ordering::Acquire) && prepared.elapsed()>Duration::from_secs(30)) {break;}
                let library=server.scope.library.clone();
                let check=tokio::select! {_=server.cancel.cancelled()=>break,result=tokio::task::spawn_blocking(move||crate::t_cmds::with_current_library(&library,||Ok(())))=>result};
                if !matches!(check,Ok(Ok(()))) {break;}
            },
            connection=listener.accept()=>{
                let Ok((stream,peer))=connection else {break;};
                let IpAddr::V4(peer_ip)=peer.ip() else {continue;};
                if !protocol::lan_address(peer_ip) {continue;}
                let Ok(permit)=permits.clone().try_acquire_owned() else {continue;}; let server=server.clone();
                tauri::async_runtime::spawn(async move {
                    let _permit=permit;
                    tokio::select! {_=server.cancel.cancelled()=>{},_=handle(stream,peer_ip,&server)=>{}}
                });
            }
        }
    }
    server.cancel.cancel();
    drop(listener);
    for id in listeners {
        app.unlisten(id);
    }
    let _ = discovery.await;
    let _ = event_task.await;
    *server.cache.lock().await = None;
    if let Ok(mut active) = ACTIVE.lock() {
        if active.as_ref().is_some_and(|active| active.id == server.id) {
            *active = None;
        }
    }
    server.closed.cancel();
}

fn callback(value: &str, peer: Ipv4Addr) -> Option<reqwest::Url> {
    let value = value.strip_prefix('<')?.strip_suffix('>')?;
    let url = reqwest::Url::parse(value).ok()?;
    (url.scheme() == "http"
        && url.host_str()?.parse::<Ipv4Addr>().ok()? == peer
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none())
    .then_some(url)
}

async fn subscribe(
    stream: &mut TcpStream,
    request: &http::Request,
    peer: Ipv4Addr,
    server: &Server,
    directory: bool,
) -> std::io::Result<()> {
    let result = (|| -> Option<(String, u64)> {
        let seconds = match request.header("timeout") {
            Some("Second-infinite") | None => 300,
            Some(value) => value
                .strip_prefix("Second-")?
                .parse::<u64>()
                .ok()?
                .clamp(1, 1800),
        };
        let mut subscriptions = server.subscriptions.lock().ok()?;
        subscriptions.retain(|_, sub| sub.expires > Instant::now());
        if let Some(sid) = request.header("sid") {
            if request.header("callback").is_some() || request.header("nt").is_some() {
                return None;
            }
            let sub = subscriptions
                .get_mut(sid)
                .filter(|sub| sub.peer == peer && sub.directory == directory)?;
            if request.method == "UNSUBSCRIBE" {
                subscriptions.remove(sid);
            } else {
                sub.expires = Instant::now() + Duration::from_secs(seconds);
            }
            Some((sid.to_owned(), seconds))
        } else {
            if request.method != "SUBSCRIBE"
                || request.header("nt") != Some("upnp:event")
                || subscriptions.len() >= 16
            {
                return None;
            }
            let callback = callback(request.header("callback")?, peer)?;
            let sid = format!("uuid:{}", uuid::Uuid::new_v4());
            subscriptions.insert(
                sid.clone(),
                Subscription {
                    callback,
                    peer,
                    expires: Instant::now() + Duration::from_secs(seconds),
                    directory,
                    sequence: 0,
                },
            );
            Some((sid, seconds))
        }
    })();
    if let Some((sid, seconds)) = result {
        http::reply(
            stream,
            "200 OK",
            "text/plain",
            b"",
            &[("SID", sid), ("TIMEOUT", format!("Second-{seconds}"))],
            false,
        )
        .await?;
        if request.method == "SUBSCRIBE" {
            server.updates.notify_one();
        }
        Ok(())
    } else {
        http::reply(
            stream,
            "412 Precondition Failed",
            "text/plain",
            b"",
            &[],
            false,
        )
        .await
    }
}

async fn prepared_media(server: &Server, id: i64, path: &str) -> Result<Arc<Media>, String> {
    if !server.scope.contains(id) {
        return Err("Not shared".into());
    }
    let mut cache = server.cache.lock().await;
    if let Some(media) = cache.as_ref().filter(|media| media.source.file_id == id) {
        let source = media.source.clone();
        let library = server.scope.library.clone();
        let verify = server.verify;
        if matches!(
            tokio::task::spawn_blocking(move || verify(&library, &source)).await,
            Ok(Ok(()))
        ) {
            return Ok(media.clone());
        }
    }
    let library = server.scope.library.clone();
    let prepare = server.prepare;
    let mut media = tokio::task::spawn_blocking(move || prepare(&library, id))
        .await
        .map_err(|e| e.to_string())??;
    media.path = path.to_owned();
    let media = Arc::new(media);
    *cache = Some(media.clone());
    Ok(media)
}

fn spool_browse(
    snapshot: &scope::Snapshot,
    spec: &upnp::BrowseSpec,
    base: &str,
    update: u32,
    catalogue: upnp::StreamCatalogue,
    cancel: &CancellationToken,
) -> Result<(std::fs::File, u64), String> {
    use std::io::{Seek, Write};
    let mut file = tempfile::tempfile().map_err(|e| e.to_string())?;
    {
        let mut writer = std::io::BufWriter::with_capacity(64 * 1024, &mut file);
        writer
            .write_all(upnp::browse_start().as_bytes())
            .map_err(|e| e.to_string())?;
        let (returned, total) =
            catalogue(snapshot, spec.start, spec.count, &spec.sort, &mut |entry| {
                if cancel.is_cancelled() {
                    return Err("Sharing cancelled".into());
                }
                writer
                    .write_all(upnp::escaped_item(&entry, base).as_bytes())
                    .map_err(|e| e.to_string())
            })?;
        writer
            .write_all(upnp::browse_finish(returned, total, update).as_bytes())
            .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
    }
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    file.seek(std::io::SeekFrom::Start(0))
        .map_err(|e| e.to_string())?;
    Ok((file, length))
}

async fn handle(mut stream: TcpStream, peer: Ipv4Addr, server: &Server) -> std::io::Result<()> {
    let request = match http::read(&mut stream).await {
        Ok(request) => request,
        Err(_) => {
            return http::reply(
                &mut stream,
                "400 Bad Request",
                "text/plain",
                b"",
                &[],
                false,
            )
            .await;
        }
    };
    if !server.active.load(Ordering::Acquire) || server.cancel.is_cancelled() {
        return http::reply(&mut stream, "404 Not Found", "text/plain", b"", &[], false).await;
    }
    let prefix = format!("/{}/", server.id);
    let Some(path) = request
        .target
        .split('?')
        .next()
        .and_then(|path| path.strip_prefix(&prefix))
    else {
        return http::reply(&mut stream, "404 Not Found", "text/plain", b"", &[], false).await;
    };
    let head = request.method == "HEAD";
    if matches!(request.method.as_str(), "GET" | "HEAD") {
        let document = match path {
            "description.xml" => Some(upnp::description(&server.id, &server.name)),
            "content-directory.xml" => Some(upnp::scpd(true)),
            "connection-manager.xml" => Some(upnp::scpd(false)),
            _ => None,
        };
        if let Some(document) = document {
            return http::reply(
                &mut stream,
                "200 OK",
                "text/xml; charset=\"utf-8\"",
                document.as_bytes(),
                &[],
                head,
            )
            .await;
        }
        if let Some(id) = path
            .strip_prefix("media/")
            .and_then(|value| value.split_once('.'))
            .filter(|(_, extension)| {
                !extension.is_empty()
                    && extension.len() < 12
                    && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
            })
            .and_then(|(id, _)| id.parse::<i64>().ok())
            .filter(|id| server.scope.contains(*id))
        {
            if let Ok(media) = prepared_media(server, id, &request.target).await {
                return super::send_media(
                    &mut stream,
                    &request.method,
                    request.header("range"),
                    &server.scope.library,
                    media,
                    server.verify,
                    || !server.cancel.is_cancelled(),
                )
                .await;
            }
        }
    } else if request.method == "POST"
        && matches!(
            path,
            "control/content-directory" | "control/connection-manager"
        )
    {
        let directory = path.ends_with("content-directory");
        let base = format!("http://{}/{}", stream.local_addr()?, server.id);
        let scope = server.scope.clone();
        let catalogue = server.catalogue;
        let update = server.update_id.load(Ordering::Acquire);
        if directory {
            if let Some(spec) = upnp::streamed_browse(&request) {
                let code = match spec {
                    Ok(spec) => {
                        let catalogue = server.stream_catalogue;
                        let cancel = server.cancel.clone();
                        match tokio::task::spawn_blocking(move || {
                            spool_browse(&scope, &spec, &base, update, catalogue, &cancel)
                        })
                        .await
                        {
                            Ok(Ok((file, length))) => {
                                return http::reply_file(&mut stream, file, length).await;
                            }
                            _ => 501,
                        }
                    }
                    Err(code) => code,
                };
                let response = upnp::fault(code);
                return http::reply(
                    &mut stream,
                    response.status,
                    "text/xml; charset=\"utf-8\"",
                    response.body.as_bytes(),
                    &[],
                    false,
                )
                .await;
            }
        }
        let response = tokio::task::spawn_blocking(move || {
            upnp::control(&request, directory, &scope, &base, update, catalogue)
        })
        .await
        .map_err(std::io::Error::other)?;
        return http::reply(
            &mut stream,
            response.status,
            "text/xml; charset=\"utf-8\"",
            response.body.as_bytes(),
            &[],
            false,
        )
        .await;
    } else if matches!(request.method.as_str(), "SUBSCRIBE" | "UNSUBSCRIBE")
        && matches!(
            path,
            "events/content-directory" | "events/connection-manager"
        )
    {
        return subscribe(
            &mut stream,
            &request,
            peer,
            server,
            path.ends_with("content-directory"),
        )
        .await;
    }
    http::reply(&mut stream, "404 Not Found", "text/plain", b"", &[], head).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn catalogue(
        snapshot: &scope::Snapshot,
        start: u32,
        count: u32,
        sort: &str,
        item: Option<i64>,
    ) -> Result<scope::Page, String> {
        scope::page_on(
            &scope::tests::database(6),
            snapshot,
            start,
            count,
            sort,
            item,
        )
    }

    fn stream_catalogue(
        snapshot: &scope::Snapshot,
        start: u32,
        count: u32,
        sort: &str,
        emit: &mut dyn FnMut(scope::Entry) -> Result<(), String>,
    ) -> Result<(usize, usize), String> {
        scope::stream_page_on(
            &scope::tests::database(6),
            snapshot,
            start,
            count,
            sort,
            emit,
        )
    }
    fn prepare(_: &str, id: i64) -> Result<Media, String> {
        Ok(Media {
            source: Source {
                file_id: id,
                path: PathBuf::from("test-only"),
                size: 20,
                modified: None,
                orientation: 1,
                rotate: 0,
            },
            title: format!("photo-{id}"),
            mime: "image/jpeg",
            path: String::new(),
            bytes: Some(format!("jpeg-preview-{id}").into_bytes()),
        })
    }
    fn fixture(port: u16) -> Arc<Server> {
        let conn = scope::tests::database(6);
        let scope = Arc::new(
            scope::resolve_on(
                &conn,
                "test",
                scope::Scope::Selection {
                    file_ids: vec![1, 2],
                },
            )
            .unwrap(),
        );
        Arc::new(Server {
            id: "test-share".into(),
            owner: "test-window".into(),
            name: "Lap · 测试".into(),
            scope,
            port,
            active: AtomicBool::new(false),
            activated: Notify::new(),
            cancel: CancellationToken::new(),
            closed: CancellationToken::new(),
            cache: tokio::sync::Mutex::new(None),
            subscriptions: Mutex::new(HashMap::new()),
            updates: Notify::new(),
            update_id: AtomicU32::new(1),
            catalogue,
            stream_catalogue,
            prepare,
            verify: |_, _| Ok(()),
        })
    }

    async fn start_fixture() -> (String, Arc<Server>, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let state = fixture(address.port());
        let server = state.clone();
        let task = tokio::spawn(async move {
            let notifications = tokio::spawn(events(server.clone()));
            loop {
                tokio::select! {
                    _=server.cancel.cancelled()=>break,
                    connection=listener.accept()=>{let (stream,_)=connection.unwrap();let server=server.clone();tokio::spawn(async move {tokio::select! {_=server.cancel.cancelled()=>{},result=handle(stream,Ipv4Addr::LOCALHOST,&server)=>{result.unwrap();}}});}
                }
            }
            drop(listener);
            let _ = notifications.await;
        });
        (format!("http://{address}/test-share"), state, task)
    }

    async fn browse(
        client: &reqwest::Client,
        base: &str,
        object: &str,
        start: &str,
        count: &str,
        flag: &str,
    ) -> String {
        let body = protocol::envelope(
            upnp::CONTENT_DIRECTORY,
            "Browse",
            &[
                ("ObjectID", object),
                ("BrowseFlag", flag),
                ("Filter", "*"),
                ("StartingIndex", start),
                ("RequestedCount", count),
                ("SortCriteria", ""),
            ],
        );
        client
            .post(format!("{base}/control/content-directory"))
            .header(
                "SOAPAction",
                format!("\"{}#Browse\"", upnp::CONTENT_DIRECTORY),
            )
            .body(body)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn television_can_discover_directory_browse_only_shared_items_and_fetch_the_advertised_media_url()
     {
        let (base, server, task) = start_fixture().await;
        let client = protocol::client().unwrap();
        assert_eq!(
            client
                .get(format!("{base}/description.xml"))
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
        server.active.store(true, Ordering::Release);
        let descriptor = client
            .get(format!("{base}/description.xml"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(descriptor.contains(upnp::MEDIA_SERVER));
        assert!(descriptor.contains("content-directory.xml"));
        let root = browse(&client, &base, "0", "0", "10", "BrowseDirectChildren").await;
        assert_eq!(
            protocol::xml(&root)
                .unwrap()
                .descendant("BrowseResponse")
                .unwrap()
                .value("NumberReturned"),
            "1"
        );
        let page = browse(&client, &base, "1", "0", "1", "BrowseDirectChildren").await;
        let document = protocol::xml(&page).unwrap();
        let response = document.descendant("BrowseResponse").unwrap();
        assert_eq!(response.value("NumberReturned"), "1");
        assert_eq!(response.value("TotalMatches"), "2");
        let didl = protocol::xml(response.value("Result")).unwrap();
        let uri = didl.descendant("item").unwrap().value("res");
        let media = client
            .get(uri)
            .header("Range", "bytes=5-")
            .send()
            .await
            .unwrap();
        assert_eq!(media.status(), 206);
        assert_eq!(media.bytes().await.unwrap().as_ref(), b"preview-1");
        assert_eq!(
            client
                .get(format!("{base}/media/3.jpg"))
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
        assert!(
            browse(&client, &base, "i3", "0", "1", "BrowseMetadata")
                .await
                .contains("<errorCode>701</errorCode>")
        );
        let page = browse(&client, &base, "1", "1", "1", "BrowseDirectChildren").await;
        assert!(
            protocol::xml(&page)
                .unwrap()
                .descendant("BrowseResponse")
                .unwrap()
                .value("Result")
                .contains("i2")
        );
        let all = browse(&client, &base, "1", "0", "0", "BrowseDirectChildren").await;
        assert_eq!(
            protocol::xml(&all)
                .unwrap()
                .descendant("BrowseResponse")
                .unwrap()
                .value("NumberReturned"),
            "2"
        );
        server.cancel.cancel();
        task.await.unwrap();
        assert!(client.get(uri).send().await.is_err());
    }

    #[tokio::test]
    async fn event_subscriptions_send_initial_state_and_reject_callbacks_on_other_hosts() {
        let (base, server, task) = start_fixture().await;
        server.active.store(true, Ordering::Release);
        let client = protocol::client().unwrap();
        let method = reqwest::Method::from_bytes(b"SUBSCRIBE").unwrap();
        assert_eq!(
            client
                .request(method.clone(), format!("{base}/events/content-directory"))
                .header("NT", "upnp:event")
                .header("CALLBACK", "<http://8.8.8.8/events>")
                .send()
                .await
                .unwrap()
                .status(),
            412
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let callback_url = format!("<http://{}/events>", listener.local_addr().unwrap());
        let callback = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = http::read(&mut stream).await.unwrap();
            http::reply(&mut stream, "200 OK", "text/plain", b"", &[], false)
                .await
                .unwrap();
            request
        });
        let response = client
            .request(method, format!("{base}/events/content-directory"))
            .header("NT", "upnp:event")
            .header("CALLBACK", callback_url)
            .header("TIMEOUT", "Second-30")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let sid = response
            .headers()
            .get("SID")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let notification = tokio::time::timeout(Duration::from_secs(3), callback)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(notification.method, "NOTIFY");
        assert_eq!(notification.header("sid"), Some(sid.as_str()));
        assert_eq!(notification.header("seq"), Some("0"));
        assert!(
            String::from_utf8(notification.body)
                .unwrap()
                .contains("<SystemUpdateID>1</SystemUpdateID>")
        );
        assert_eq!(
            client
                .request(
                    reqwest::Method::from_bytes(b"UNSUBSCRIBE").unwrap(),
                    format!("{base}/events/content-directory")
                )
                .header("SID", sid)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        assert!(server.subscriptions.lock().unwrap().is_empty());
        server.cancel.cancel();
        task.await.unwrap();
    }

    #[test]
    fn unlimited_browse_streams_the_complete_album_and_keeps_requested_page_counts_exact() {
        use std::io::Read;
        fn large_catalogue(
            snapshot: &scope::Snapshot,
            start: u32,
            count: u32,
            sort: &str,
            emit: &mut dyn FnMut(scope::Entry) -> Result<(), String>,
        ) -> Result<(usize, usize), String> {
            scope::stream_page_on(
                &scope::tests::database(1005),
                snapshot,
                start,
                count,
                sort,
                emit,
            )
        }
        let conn = scope::tests::database(1005);
        let snapshot =
            scope::resolve_on(&conn, "test", scope::Scope::Album { album_id: 1 }).unwrap();
        let spec = upnp::BrowseSpec {
            start: 0,
            count: 0,
            sort: String::new(),
        };
        let cancel = CancellationToken::new();
        let (mut file, length) = spool_browse(
            &snapshot,
            &spec,
            "http://192.168.1.2/share",
            1,
            large_catalogue,
            &cancel,
        )
        .unwrap();
        let mut xml = String::new();
        file.read_to_string(&mut xml).unwrap();
        assert_eq!(xml.len() as u64, length);
        assert_eq!(xml.matches("&lt;item ").count(), 1005);
        assert!(xml.contains("<NumberReturned>1005</NumberReturned>"));
        let spec = upnp::BrowseSpec {
            start: 100,
            count: 250,
            sort: String::new(),
        };
        let (mut file, _) = spool_browse(
            &snapshot,
            &spec,
            "http://192.168.1.2/share",
            1,
            large_catalogue,
            &cancel,
        )
        .unwrap();
        xml.clear();
        file.read_to_string(&mut xml).unwrap();
        assert_eq!(xml.matches("&lt;item ").count(), 250);
        assert!(xml.contains("<TotalMatches>1005</TotalMatches>"));
        cancel.cancel();
        assert!(
            spool_browse(
                &snapshot,
                &spec,
                "http://192.168.1.2/share",
                1,
                large_catalogue,
                &cancel
            )
            .is_err()
        );
    }
}
