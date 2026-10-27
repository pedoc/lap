use super::{Media, Source, http, native, protocol, scope, upnp};
use once_cell::sync::Lazy;
use serde::Serialize;
use std::{
    net::{IpAddr, Ipv4Addr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{Listener, Manager};
use tokio::{
    net::{TcpListener, TcpStream},
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
    address: String,
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
    updates: Notify,
    update_id: Arc<AtomicU32>,
    address: Ipv4Addr,
    event_base: String,
    published: tokio::sync::watch::Sender<Option<Result<(), String>>>,
    catalogue: upnp::Catalogue,
    stream_catalogue: upnp::StreamCatalogue,
    prepare: fn(&str, i64) -> Result<Media, String>,
    verify: fn(&str, &Source) -> Result<(), String>,
}

static ACTIVE: Lazy<Mutex<Option<Arc<Server>>>> = Lazy::new(|| Mutex::new(None));
pub async fn start(
    window: tauri::WebviewWindow,
    library_id: String,
    selection: scope::Scope,
    order: Option<scope::PlaybackOrder>,
) -> Result<Shared, String> {
    let scope = Arc::new(
        tokio::task::spawn_blocking(move || scope::resolve(&library_id, selection, order))
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
    let (address, event_base) =
        tokio::task::spawn_blocking(|| Ok::<_, String>((native::address()?, native::endpoint()?)))
            .await
            .map_err(|e| e.to_string())??;
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
        updates: Notify::new(),
        update_id: Arc::new(AtomicU32::new(1)),
        address,
        event_base,
        published: tokio::sync::watch::channel(None).0,
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
        address: server.address.to_string(),
    };
    tauri::async_runtime::spawn(run(listener, server, window.app_handle().clone()));
    Ok(result)
}

pub async fn activate(owner: &str, id: &str) -> Result<(), String> {
    let server = ACTIVE
        .lock()
        .map_err(|_| "共享服务不可用")?
        .as_ref()
        .filter(|server| server.id == id && server.owner == owner && !server.cancel.is_cancelled())
        .cloned()
        .ok_or("共享会话已结束")?;
    let mut published = server.published.subscribe();
    server.active.store(true, Ordering::Release);
    server.activated.notify_one();
    let result = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Some(result) = published.borrow_and_update().clone() {
                return result;
            }
            published
                .changed()
                .await
                .map_err(|_| "共享服务已关闭".to_owned())?;
        }
    })
    .await
    .map_err(|_| "媒体服务器注册超时".to_owned())
    .and_then(|result| result);
    if result.is_err() {
        server.cancel.cancel();
    }
    result
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

async fn publish(server: Arc<Server>) {
    if !server.active.load(Ordering::Acquire) {
        tokio::select! { _=server.cancel.cancelled()=>return, _=server.activated.notified()=>{} }
    }
    if server.cancel.is_cancelled() {
        return;
    }
    let description = format!(
        "http://{}:{}/{}/description.xml",
        server.address, server.port, server.id
    );
    let device = native::Device::publish(
        description,
        server.id.clone(),
        upnp::PROTOCOLS,
        server.update_id.clone(),
    );
    let mut device = match tokio::select! { _=server.cancel.cancelled()=>return, result=device=>result }
    {
        Ok(device) => {
            server.published.send_replace(Some(Ok(())));
            device
        }
        Err(error) => {
            server.published.send_replace(Some(Err(error)));
            server.cancel.cancel();
            return;
        }
    };
    loop {
        tokio::select! {
            _=server.cancel.cancelled()=>break,
            _=server.updates.notified()=>{ let _=device.notify(server.update_id.load(Ordering::Acquire)); }
        }
    }
    device.close().await;
}

async fn run(listener: TcpListener, server: Arc<Server>, app: tauri::AppHandle) {
    let publication = tauri::async_runtime::spawn(publish(server.clone()));
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
                    tokio::select! {_=server.cancel.cancelled()=>{},_=handle(stream,peer_ip,server.clone())=>{}}
                });
            }
        }
    }
    server.cancel.cancel();
    drop(listener);
    for id in listeners {
        app.unlisten(id);
    }
    let _ = publication.await;
    *server.cache.lock().await = None;
    if let Ok(mut active) = ACTIVE.lock() {
        if active.as_ref().is_some_and(|active| active.id == server.id) {
            *active = None;
        }
    }
    server.closed.cancel();
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

async fn handle(stream: TcpStream, _peer: Ipv4Addr, server: Arc<Server>) -> std::io::Result<()> {
    let local = stream.local_addr()?;
    http::serve(stream, move |request| {
        let server = server.clone();
        async move {
            if !server.active.load(Ordering::Acquire) || server.cancel.is_cancelled() {
                return http::reply("404 Not Found", "text/plain", b"", &[], false);
            }
            let prefix = format!("/{}/", server.id);
            let Some(path) = request
                .target
                .split('?')
                .next()
                .and_then(|path| path.strip_prefix(&prefix))
            else {
                return http::reply("404 Not Found", "text/plain", b"", &[], false);
            };
            let head = request.method == "HEAD";
            if matches!(request.method.as_str(), "GET" | "HEAD") {
                let document = match path {
                    "description.xml" => Some(upnp::description(
                        &server.id,
                        &server.name,
                        &server.event_base,
                    )),
                    "content-directory.xml" => Some(upnp::scpd(true)),
                    "connection-manager.xml" => Some(upnp::scpd(false)),
                    _ => None,
                };
                if let Some(document) = document {
                    return http::reply(
                        "200 OK",
                        "text/xml; charset=\"utf-8\"",
                        document.as_bytes(),
                        &[],
                        head,
                    );
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
                    if let Ok(media) = prepared_media(&server, id, &request.target).await {
                        return super::send_media(
                            &request.method,
                            request.header("range"),
                            &server.scope.library,
                            media,
                            server.verify,
                            {
                                let server = server.clone();
                                move || !server.cancel.is_cancelled()
                            },
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
                let base = format!("http://{local}/{}", server.id);
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
                                        return http::reply_file(file, length);
                                    }
                                    _ => 501,
                                }
                            }
                            Err(code) => code,
                        };
                        let response = upnp::fault(code);
                        return http::reply(
                            response.status,
                            "text/xml; charset=\"utf-8\"",
                            response.body.as_bytes(),
                            &[],
                            false,
                        );
                    }
                }
                let response = tokio::task::spawn_blocking(move || {
                    upnp::control(&request, directory, &scope, &base, update, catalogue)
                })
                .await
                .map_err(std::io::Error::other)?;
                return http::reply(
                    response.status,
                    "text/xml; charset=\"utf-8\"",
                    response.body.as_bytes(),
                    &[],
                    false,
                );
            }
            http::reply("404 Not Found", "text/plain", b"", &[], head)
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::super::protocol::XmlElement;
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
            id: uuid::Uuid::new_v4().to_string(),
            owner: "test-window".into(),
            name: "Lap · 测试".into(),
            scope,
            port,
            active: AtomicBool::new(false),
            activated: Notify::new(),
            cancel: CancellationToken::new(),
            closed: CancellationToken::new(),
            cache: tokio::sync::Mutex::new(None),
            updates: Notify::new(),
            update_id: Arc::new(AtomicU32::new(1)),
            address: native::address().unwrap(),
            event_base: native::endpoint().unwrap(),
            published: tokio::sync::watch::channel(None).0,
            catalogue,
            stream_catalogue,
            prepare,
            verify: |_, _| Ok(()),
        })
    }

    async fn start_fixture() -> (String, Arc<Server>, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let address = std::net::SocketAddrV4::new(native::address().unwrap(), port);
        let state = fixture(port);
        let server = state.clone();
        let task = tokio::spawn(async move {
            let publication = tokio::spawn(publish(server.clone()));
            loop {
                tokio::select! {
                    _=server.cancel.cancelled()=>break,
                    connection=listener.accept()=>{let (stream,_)=connection.unwrap();let server=server.clone();tokio::spawn(async move {tokio::select! {_=server.cancel.cancelled()=>{},result=handle(stream,Ipv4Addr::LOCALHOST,server.clone())=>{result.unwrap();}}});}
                }
            }
            drop(listener);
            let _ = publication.await;
        });
        (format!("http://{address}/{}", state.id), state, task)
    }

    async fn activate_fixture(server: &Server) {
        let mut published = server.published.subscribe();
        server.active.store(true, Ordering::Release);
        server.activated.notify_one();
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let Some(result) = published.borrow_and_update().clone() {
                    return result;
                }
                published.changed().await.unwrap();
            }
        })
        .await
        .unwrap()
        .unwrap();
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
        )
        .unwrap();
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
        activate_fixture(&server).await;
        let socket = tokio::net::UdpSocket::bind((server.address, 0))
            .await
            .unwrap();
        let search = format!(
            "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: uuid:{}\r\n\r\n",
            server.id
        );
        socket
            .send_to(search.as_bytes(), (Ipv4Addr::new(239, 255, 255, 250), 1900))
            .await
            .unwrap();
        let mut packet = [0u8; 8192];
        let (length, _) =
            tokio::time::timeout(Duration::from_secs(4), socket.recv_from(&mut packet))
                .await
                .unwrap()
                .unwrap();
        let response = String::from_utf8_lossy(&packet[..length]);
        assert!(
            response
                .to_ascii_lowercase()
                .contains(&format!("location: {base}/description.xml").to_ascii_lowercase())
        );
        assert!(response.contains(&format!("uuid:{}", server.id)));
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
        let (_base, server, task) = start_fixture().await;
        activate_fixture(&server).await;
        let client = protocol::client().unwrap();
        let method = reqwest::Method::from_bytes(b"SUBSCRIBE").unwrap();
        assert_eq!(
            client
                .request(
                    method.clone(),
                    format!(
                        "{}/{}/events/content-directory",
                        server.event_base, server.id
                    )
                )
                .header("NT", "upnp:event")
                .header("CALLBACK", "<http://8.8.8.8/events>")
                .send()
                .await
                .unwrap()
                .status(),
            412
        );
        let other = if server.address != Ipv4Addr::new(10, 255, 255, 254) {
            "10.255.255.254"
        } else {
            "10.255.255.253"
        };
        assert_eq!(
            client
                .request(
                    method.clone(),
                    format!(
                        "{}/{}/events/content-directory",
                        server.event_base, server.id
                    )
                )
                .header("NT", "upnp:event")
                .header("CALLBACK", format!("<http://{other}/events>"))
                .send()
                .await
                .unwrap()
                .status(),
            412
        );
        let listener = TcpListener::bind((server.address, 0)).await.unwrap();
        let callback_url = format!("<http://{}/events>", listener.local_addr().unwrap());
        let callback = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let sender = Arc::new(Mutex::new(Some(sender)));
            http::serve(stream, move |request| {
                let sender = sender.clone();
                async move {
                    if let Some(sender) = sender.lock().unwrap().take() {
                        let _ = sender.send(request);
                    }
                    http::reply("200 OK", "text/plain", b"", &[], false)
                }
            })
            .await
            .unwrap();
            receiver.await.unwrap()
        });
        let response = client
            .request(
                method,
                format!(
                    "{}/{}/events/content-directory",
                    server.event_base, server.id
                ),
            )
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
                    format!(
                        "{}/{}/events/content-directory",
                        server.event_base, server.id
                    )
                )
                .header("SID", sid)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
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
