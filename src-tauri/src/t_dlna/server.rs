use super::{Media, Source, http, network, protocol, publication, scope, upnp};
use futures_util::{StreamExt, future::join_all, stream};
use once_cell::sync::Lazy;
use serde::Serialize;
use std::{
    net::{IpAddr, Ipv4Addr},
    sync::{
        Arc, Mutex, RwLock,
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
}

#[derive(Clone, Serialize)]
pub struct PublishedNetwork {
    id: String,
    name: String,
    address: String,
}

#[derive(Clone, Default, Serialize)]
pub struct Published {
    networks: Vec<PublishedNetwork>,
    errors: Vec<String>,
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
    interfaces: Vec<network::Interface>,
    endpoints: RwLock<Vec<publication::Endpoint>>,
    published: tokio::sync::watch::Sender<Option<Result<Published, String>>>,
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
    interface_id: Option<String>,
) -> Result<Shared, String> {
    let interfaces = tokio::task::spawn_blocking(network::interfaces)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|interface| interface_id.as_ref().is_none_or(|id| id == &interface.id))
        .collect::<Vec<_>>();
    if interfaces.is_empty() {
        return Err("没有可发布的局域网 IPv4 网卡".into());
    }
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
        interfaces,
        endpoints: RwLock::new(Vec::new()),
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
    };
    tauri::async_runtime::spawn(run(listener, server, window.app_handle().clone()));
    Ok(result)
}

pub async fn activate(owner: &str, id: &str) -> Result<Published, String> {
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
    let result = tokio::time::timeout(Duration::from_secs(30), async {
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

pub fn status(owner: &str, id: &str) -> Result<Published, String> {
    let active = ACTIVE.lock().map_err(|_| "共享服务不可用")?;
    let server = active
        .as_ref()
        .filter(|server| server.id == id && server.owner == owner && !server.cancel.is_cancelled())
        .ok_or("媒体共享已结束，请重新开启")?;
    server
        .published
        .borrow()
        .clone()
        .ok_or_else(|| "媒体共享尚未发布".to_owned())?
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
    let results = stream::iter(server.interfaces.clone())
        .map(|interface| {
            let server = server.clone();
            async move {
                let name = interface.name.clone();
                publication::Publication::start(
                    interface,
                    &server.id,
                    &server.name,
                    server.port,
                    server.update_id.load(Ordering::Acquire),
                    &server.cancel,
                )
                .await
                .map_err(|error| format!("{name}：{error}"))
            }
        })
        .buffer_unordered(4)
        .collect::<Vec<_>>()
        .await;
    let mut publications = Vec::new();
    let mut report = Published::default();
    for result in results {
        match result {
            Ok(publication) => publications.push(publication),
            Err(error) => report.errors.push(error),
        }
    }
    publications.sort_by_key(|publication| {
        server
            .interfaces
            .iter()
            .position(|interface| interface.id == publication.interface.id)
    });
    let mut watchdog = tokio::time::interval(Duration::from_secs(2));
    loop {
        report.networks = publications
            .iter()
            .map(|publication| PublishedNetwork {
                id: publication.interface.id.clone(),
                name: publication.interface.name.clone(),
                address: publication.endpoint.address.to_string(),
            })
            .collect();
        if let Ok(mut endpoints) = server.endpoints.write() {
            *endpoints = publications
                .iter()
                .map(|publication| publication.endpoint.clone())
                .collect();
        }
        if publications.is_empty() {
            server
                .published
                .send_replace(Some(Err(if report.errors.is_empty() {
                    "媒体发布进程已退出".into()
                } else {
                    report.errors.join("；")
                })));
            server.cancel.cancel();
            break;
        }
        server.published.send_replace(Some(Ok(report.clone())));
        let update = tokio::select! {
            _=server.cancel.cancelled()=>break,
            _=server.updates.notified()=>Some(server.update_id.load(Ordering::Acquire)),
            _=watchdog.tick()=>None,
        };
        let results = join_all(publications.iter_mut().map(|publication| async move {
            if !publication.is_running() {
                Err("媒体发布进程已退出".into())
            } else if let Some(update) = update {
                publication.update(update).await
            } else {
                Ok(())
            }
        }))
        .await;
        let mut stopped = Vec::new();
        for (index, result) in results.into_iter().enumerate().rev() {
            if let Err(error) = result {
                report
                    .errors
                    .push(format!("{}：{error}", publications[index].interface.name));
                stopped.push(publications.remove(index));
            }
        }
        join_all(stopped.iter_mut().map(publication::Publication::close)).await;
    }
    join_all(publications.iter_mut().map(publication::Publication::close)).await;
    if let Ok(mut endpoints) = server.endpoints.write() {
        endpoints.clear();
    }
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
                    "description.xml" => server.endpoints.read().ok().and_then(|endpoints| {
                        endpoints
                            .iter()
                            .find(|endpoint| IpAddr::V4(endpoint.address) == local.ip())
                            .map(|endpoint| {
                                upnp::description(
                                    &server.id,
                                    &server.name,
                                    &endpoint.event_base,
                                    &format!("http://{local}/{}", server.id),
                                )
                            })
                    }),
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
    use super::super::native;
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
            interfaces: network::interfaces(),
            endpoints: RwLock::new(vec![publication::Endpoint {
                address: native::address().unwrap(),
                event_base: native::endpoint().unwrap(),
            }]),
            published: tokio::sync::watch::channel(None).0,
            catalogue,
            stream_catalogue,
            prepare,
            verify: |_, _| Ok(()),
        })
    }

    async fn start_fixture() -> (String, Arc<Server>, tokio::task::JoinHandle<()>) {
        start_fixture_with_publication(false).await
    }

    async fn start_fixture_with_publication(
        real: bool,
    ) -> (String, Arc<Server>, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let address = std::net::SocketAddrV4::new(native::address().unwrap(), port);
        let state = fixture(port);
        let server = state.clone();
        let task = tokio::spawn(async move {
            let publication = if real {
                tokio::spawn(publish(server.clone()))
            } else {
                tokio::spawn(publish_fixture(server.clone()))
            };
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

    fn endpoint(server: &Server) -> publication::Endpoint {
        server.endpoints.read().unwrap()[0].clone()
    }

    async fn publish_fixture(server: Arc<Server>) {
        if !server.active.load(Ordering::Acquire) {
            tokio::select! { _=server.cancel.cancelled()=>return, _=server.activated.notified()=>{} }
        }
        let description = format!(
            "http://{}:{}/{}/description.xml",
            endpoint(&server).address,
            server.port,
            server.id
        );
        let mut device = native::Device::publish(
            description,
            server.id.clone(),
            upnp::PROTOCOLS,
            server.update_id.clone(),
            false,
        )
        .await
        .unwrap();
        server
            .published
            .send_replace(Some(Ok(Published::default())));
        loop {
            tokio::select! {
                _=server.cancel.cancelled()=>break,
                _=server.updates.notified()=>{let _=device.notify(server.update_id.load(Ordering::Acquire));}
            }
        }
        device.close().await;
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
        let socket = tokio::net::UdpSocket::bind((endpoint(&server).address, 0))
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
                        endpoint(&server).event_base,
                        server.id
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
        let other = if endpoint(&server).address != Ipv4Addr::new(10, 255, 255, 254) {
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
                        endpoint(&server).event_base,
                        server.id
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
        let listener = TcpListener::bind((endpoint(&server).address, 0))
            .await
            .unwrap();
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
                    endpoint(&server).event_base,
                    server.id
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
                        endpoint(&server).event_base,
                        server.id
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

    #[tokio::test]
    async fn description_and_media_urls_follow_the_connection_interface_and_ignore_host_header() {
        let (_base, server, task) = start_fixture().await;
        server
            .endpoints
            .write()
            .unwrap()
            .push(publication::Endpoint {
                address: Ipv4Addr::LOCALHOST,
                event_base: "http://127.0.0.1:49111".into(),
            });
        activate_fixture(&server).await;
        let client = protocol::client().unwrap();
        for address in [endpoint(&server).address, Ipv4Addr::LOCALHOST] {
            let base = format!("http://{address}:{}/{}", server.port, server.id);
            let descriptor = client
                .get(format!("{base}/description.xml"))
                .header("Host", "8.8.8.8:1234")
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap();
            assert!(descriptor.contains(&format!(
                "<controlURL>{base}/control/content-directory</controlURL>"
            )));
            let expected = server
                .endpoints
                .read()
                .unwrap()
                .iter()
                .find(|endpoint| endpoint.address == address)
                .unwrap()
                .event_base
                .clone();
            assert!(descriptor.contains(&format!(
                "<eventSubURL>{expected}/{}/events/content-directory</eventSubURL>",
                server.id
            )));
            assert!(!descriptor.contains("8.8.8.8"));
            let page = browse(&client, &base, "1", "0", "1", "BrowseDirectChildren").await;
            let document = protocol::xml(&page).unwrap();
            let didl = protocol::xml(
                document
                    .descendant("BrowseResponse")
                    .unwrap()
                    .value("Result"),
            )
            .unwrap();
            let uri = didl.descendant("item").unwrap().value("res");
            assert!(uri.starts_with(&format!("{base}/media/")));
            assert_eq!(client.get(uri).send().await.unwrap().status(), 200);
        }
        server.cancel.cancel();
        task.await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires a built Lap executable in LAP_DLNA_TEST_WORKER and multicast networking"]
    async fn all_interfaces_advertise_browse_notify_and_withdraw_using_their_own_addresses() {
        let exe = std::env::var_os("LAP_DLNA_TEST_WORKER")
            .expect("set LAP_DLNA_TEST_WORKER to the built Lap executable");
        let (base, server, task) = start_fixture_with_publication(true).await;
        activate_fixture(&server).await;
        let report = server.published.borrow().clone().unwrap().unwrap();
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.networks.len(), server.interfaces.len());
        let client = protocol::client().unwrap();
        let mut locations = Vec::new();
        for interface in &server.interfaces {
            let mut command =
                network::worker_command(exe.clone().into(), "--lap-dlna-discover", &interface.id);
            let output = command
                .arg(upnp::MEDIA_SERVER)
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .await
                .unwrap();
            let found =
                serde_json::from_slice::<Result<Vec<(String, Ipv4Addr)>, String>>(&output.stdout)
                    .unwrap()
                    .unwrap();
            let expected = report
                .networks
                .iter()
                .find(|network| network.id == interface.id)
                .unwrap();
            let mut descriptor = None;
            for (location, _) in found {
                let xml = client
                    .get(&location)
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap();
                if xml.contains(&format!("uuid:{}", server.id)) {
                    assert_eq!(
                        reqwest::Url::parse(&location).unwrap().host_str().unwrap(),
                        expected.address
                    );
                    locations.push(location);
                    descriptor = Some(xml);
                    break;
                }
            }
            let descriptor = descriptor.unwrap_or_else(|| {
                panic!("{} could not discover its own publication", interface.name)
            });
            let document = protocol::xml(&descriptor).unwrap();
            let service = document.descendant("service").unwrap();
            let expected_base =
                format!("http://{}:{}/{}", expected.address, server.port, server.id);
            assert_eq!(
                service.value("controlURL"),
                format!("{expected_base}/control/content-directory")
            );
            assert!(
                service
                    .value("eventSubURL")
                    .starts_with(&format!("http://{}:", expected.address))
            );
            let page = browse(
                &client,
                &expected_base,
                "1",
                "0",
                "1",
                "BrowseDirectChildren",
            )
            .await;
            let document_page = protocol::xml(&page).unwrap();
            let didl = protocol::xml(
                document_page
                    .descendant("BrowseResponse")
                    .unwrap()
                    .value("Result"),
            )
            .unwrap();
            let uri = didl.descendant("item").unwrap().value("res");
            assert!(uri.starts_with(&format!("{expected_base}/media/")));
            let media = client
                .get(uri)
                .header("Range", "bytes=5-")
                .send()
                .await
                .unwrap();
            assert_eq!(media.status(), 206);
            assert_eq!(media.bytes().await.unwrap().as_ref(), b"preview-1");
            let listener = TcpListener::bind((expected.address.parse::<Ipv4Addr>().unwrap(), 0))
                .await
                .unwrap();
            let callback_url = format!("<http://{}/events>", listener.local_addr().unwrap());
            let (sender, mut notifications) = tokio::sync::mpsc::unbounded_channel();
            let callback = tokio::spawn(async move {
                loop {
                    let Ok((stream, _)) = listener.accept().await else {
                        break;
                    };
                    let sender = sender.clone();
                    tokio::spawn(http::serve(stream, move |request| {
                        let sender = sender.clone();
                        async move {
                            let _ = sender.send(request);
                            http::reply("200 OK", "text/plain", b"", &[], false)
                        }
                    }));
                }
            });
            let response = client
                .request(
                    reqwest::Method::from_bytes(b"SUBSCRIBE").unwrap(),
                    service.value("eventSubURL"),
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
            let initial = tokio::time::timeout(Duration::from_secs(5), notifications.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(initial.header("seq"), Some("0"));
            let update = server.update_id.fetch_add(1, Ordering::AcqRel) + 1;
            server.updates.notify_one();
            let changed = tokio::time::timeout(Duration::from_secs(5), notifications.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(changed.header("seq"), Some("1"));
            assert!(
                String::from_utf8(changed.body)
                    .unwrap()
                    .contains(&format!("<SystemUpdateID>{update}</SystemUpdateID>"))
            );
            assert_eq!(
                client
                    .request(
                        reqwest::Method::from_bytes(b"UNSUBSCRIBE").unwrap(),
                        service.value("eventSubURL")
                    )
                    .header("SID", sid)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                200
            );
            callback.abort();
            println!("MULTI_NETWORK_OK {} {}", interface.name, expected.address);
        }
        server.cancel.cancel();
        task.await.unwrap();
        assert!(
            client
                .get(format!("{base}/media/1.jpg"))
                .send()
                .await
                .is_err()
        );
        for location in locations {
            assert!(client.get(location).send().await.is_err());
        }
        for interface in &server.interfaces {
            let mut command =
                network::worker_command(exe.clone().into(), "--lap-dlna-discover", &interface.id);
            let output = command
                .arg(upnp::MEDIA_SERVER)
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .await
                .unwrap();
            let found =
                serde_json::from_slice::<Result<Vec<(String, Ipv4Addr)>, String>>(&output.stdout)
                    .unwrap()
                    .unwrap();
            for (location, _) in found {
                if let Ok(response) = client.get(location).send().await {
                    assert!(
                        !response
                            .text()
                            .await
                            .unwrap()
                            .contains(&format!("uuid:{}", server.id))
                    );
                }
            }
        }
        println!("MULTI_NETWORK_ALL_PUBLICATIONS_WITHDRAWN");
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
