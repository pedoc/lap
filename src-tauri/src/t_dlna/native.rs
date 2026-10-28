//! Thin ownership boundary around the pinned Portable SDK for UPnP Devices.
//! HTTP media routing and the photo-library directory remain in Rust.
use once_cell::sync::{Lazy, OnceCell};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    net::Ipv4Addr,
    ptr::NonNull,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

unsafe extern "C" {
    fn lap_upnp_init(
        callback: unsafe extern "C" fn(*const c_char, *const u8),
        interface_name: *const c_char,
        port: u16,
        handle: *mut c_int,
    ) -> c_int;
    fn lap_upnp_address() -> *const c_char;
    fn lap_upnp_port() -> u16;
    fn lap_upnp_search(handle: c_int, target: *const c_char) -> c_int;
    fn lap_upnp_free(text: *mut c_char);
    fn lap_upnp_action_document(
        name: *const c_char,
        service: *const c_char,
        names: *const *const c_char,
        values: *const *const c_char,
        count: c_int,
        response: c_int,
    ) -> *mut c_char;
    fn lap_upnp_action(
        handle: c_int,
        url: *const c_char,
        service: *const c_char,
        name: *const c_char,
        names: *const *const c_char,
        values: *const *const c_char,
        count: c_int,
        output: *mut *mut c_char,
    ) -> c_int;
    fn lap_upnp_publish(
        description: *const c_char,
        protocols: *const c_char,
        update: unsafe extern "C" fn(*mut c_void) -> u32,
        cookie: *mut c_void,
        device: *mut *mut c_void,
        document: c_int,
    ) -> c_int;
    fn lap_upnp_notify(device: *mut c_void, udn: *const c_char, update: u32) -> c_int;
    fn lap_upnp_unpublish(device: *mut c_void);
}

static RESULTS: Lazy<Mutex<Vec<(String, Ipv4Addr)>>> = Lazy::new(|| Mutex::new(Vec::new()));
static SEARCH: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
// libupnp is process-wide. Retain its control point until process exit so a
// cancelled async operation can never race UpnpFinish or a freed callback cookie.
static CLIENT: OnceCell<c_int> = OnceCell::new();
static INTERFACE: OnceCell<String> = OnceCell::new();

pub(super) fn use_interface(id: String) -> Result<(), String> {
    if CLIENT.get().is_some() {
        return Err("UPnP 已在其他网络初始化".into());
    }
    INTERFACE.set(id).map_err(|_| "UPnP 网络已确定".into())
}

fn check(code: c_int) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("UPnP 协议库返回错误（{code}）"))
    }
}
fn string(value: &str) -> Result<CString, String> {
    CString::new(value).map_err(|_| "UPnP 参数包含无效字符".into())
}
fn client() -> Result<c_int, String> {
    CLIENT
        .get_or_try_init(|| {
            // SDK auto-selection can pick a VMware/host-only adapter. Use the
            // operating system's default interface and let the SDK bind SSDP to it.
            let interface = if let Some(id) = INTERFACE.get() {
                netdev::get_interfaces()
                    .into_iter()
                    .find(|interface| &interface.name == id)
                    .ok_or_else(|| "所选网卡已不可用".to_owned())?
            } else {
                netdev::get_default_interface()
                    .map_err(|error| format!("无法确定 DLNA 使用的网络接口：{error}"))?
            };
            if interface.ipv4.is_empty() {
                return Err("默认网络接口没有 IPv4 地址，无法搜索局域网电视".into());
            }
            let interface_name = string(&interface.name)?;
            // Discovery helpers coexist with the application's media server.
            // Reserve an unused port instead of sharing the SDK's default port.
            let port = std::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))
                .and_then(|listener| listener.local_addr())
                .map_err(|error| error.to_string())?
                .port();
            let mut handle = 0;
            check(unsafe {
                lap_upnp_init(discovered, interface_name.as_ptr(), port, &mut handle)
            })?;
            Ok(handle)
        })
        .copied()
}

unsafe extern "C" fn discovered(location: *const c_char, address: *const u8) {
    if location.is_null() || address.is_null() {
        return;
    }
    // Both pointers belong to the SDK's synchronous callback and are copied here.
    let location = unsafe { CStr::from_ptr(location) }
        .to_string_lossy()
        .into_owned();
    let address = unsafe { std::slice::from_raw_parts(address, 4) };
    let address = Ipv4Addr::new(address[0], address[1], address[2], address[3]);
    if super::protocol::discovery_location(&location, address).is_none() {
        return;
    }
    if let Ok(mut results) = RESULTS.lock() {
        if results.len() < 32 && !results.iter().any(|entry| entry.0 == location) {
            results.push((location, address));
        }
    }
}

pub async fn discover() -> Result<Vec<(String, Ipv4Addr)>, String> {
    discover_services(&[
        "urn:schemas-upnp-org:device:MediaRenderer:1",
        "urn:schemas-upnp-org:service:AVTransport:1",
    ])
    .await
}

pub(super) async fn discover_services(targets: &[&str]) -> Result<Vec<(String, Ipv4Addr)>, String> {
    let _search = SEARCH.lock().await;
    let handle = tokio::task::spawn_blocking(client)
        .await
        .map_err(|e| e.to_string())??;
    RESULTS.lock().map_err(|_| "设备搜索不可用")?.clear();
    for target in targets {
        let target = string(target)?;
        check(unsafe { lap_upnp_search(handle, target.as_ptr()) })?;
    }
    tokio::time::sleep(Duration::from_secs(3)).await;
    Ok(RESULTS.lock().map_err(|_| "设备搜索不可用")?.clone())
}

pub fn endpoint() -> Result<String, String> {
    client()?;
    let address = unsafe { lap_upnp_address() };
    if address.is_null() {
        return Err("UPnP 服务未选择局域网接口".into());
    }
    let address = unsafe { CStr::from_ptr(address) }
        .to_str()
        .map_err(|_| "UPnP 接口地址无效")?;
    Ok(format!("http://{address}:{}", unsafe { lap_upnp_port() }))
}
pub fn address() -> Result<Ipv4Addr, String> {
    client()?;
    let address = unsafe { lap_upnp_address() };
    if address.is_null() {
        return Err("UPnP 服务未选择局域网接口".into());
    }
    unsafe { CStr::from_ptr(address) }
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| "UPnP 接口地址无效".into())
}

struct Text(*mut c_char);
impl Text {
    fn copy(&self) -> Result<String, String> {
        if self.0.is_null() {
            return Err("UPnP 服务未返回有效数据".into());
        }
        unsafe { CStr::from_ptr(self.0) }
            .to_str()
            .map(str::to_owned)
            .map_err(|_| "UPnP 响应编码无效".into())
    }
}
impl Drop for Text {
    fn drop(&mut self) {
        unsafe { lap_upnp_free(self.0) };
    }
}

struct Arguments {
    names: Vec<CString>,
    values: Vec<CString>,
}
impl Arguments {
    fn new(arguments: &[(&str, &str)]) -> Result<Self, String> {
        Ok(Self {
            names: arguments
                .iter()
                .map(|(name, _)| string(name))
                .collect::<Result<_, _>>()?,
            values: arguments
                .iter()
                .map(|(_, value)| string(value))
                .collect::<Result<_, _>>()?,
        })
    }
    fn pointers(&self) -> (Vec<*const c_char>, Vec<*const c_char>) {
        (
            self.names.iter().map(|value| value.as_ptr()).collect(),
            self.values.iter().map(|value| value.as_ptr()).collect(),
        )
    }
}

pub fn document(
    name: &str,
    service: &str,
    arguments: &[(&str, &str)],
    response: bool,
) -> Result<String, String> {
    let name = string(name)?;
    let service = string(service)?;
    let arguments = Arguments::new(arguments)?;
    let (names, values) = arguments.pointers();
    Text(unsafe {
        lap_upnp_action_document(
            name.as_ptr(),
            service.as_ptr(),
            names.as_ptr(),
            values.as_ptr(),
            names.len() as c_int,
            response as c_int,
        )
    })
    .copy()
}

pub async fn action(
    url: String,
    service: String,
    name: String,
    arguments: Vec<(String, String)>,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let handle = client()?;
        let url = string(&url)?;
        let service = string(&service)?;
        let name = string(&name)?;
        let arguments = arguments
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        let arguments = Arguments::new(&arguments)?;
        let (names, values) = arguments.pointers();
        let mut output = std::ptr::null_mut();
        let code = unsafe {
            lap_upnp_action(
                handle,
                url.as_ptr(),
                service.as_ptr(),
                name.as_ptr(),
                names.as_ptr(),
                values.as_ptr(),
                names.len() as c_int,
                &mut output,
            )
        };
        let output = Text(output);
        check(code)?;
        output.copy()
    })
    .await
    .map_err(|e| e.to_string())?
}

pub struct Device {
    raw: NonNull<c_void>,
    _update: Box<Arc<AtomicU32>>,
    udn: CString,
}
// SDK device handles may be moved between threads; Drop unregisters all callbacks
// before releasing the boxed update counter. No caller can access the raw pointer.
unsafe impl Send for Device {}
unsafe extern "C" fn update(cookie: *mut c_void) -> u32 {
    unsafe { &*cookie.cast::<Arc<AtomicU32>>() }.load(Ordering::Acquire)
}
impl Device {
    pub async fn publish(
        description: String,
        id: String,
        protocols: &'static str,
        version: Arc<AtomicU32>,
        document: bool,
    ) -> Result<Self, String> {
        tokio::task::spawn_blocking(move || {
            client()?;
            let description = string(&description)?;
            let protocols = string(protocols)?;
            let mut version = Box::new(version);
            let mut raw = std::ptr::null_mut();
            check(unsafe {
                lap_upnp_publish(
                    description.as_ptr(),
                    protocols.as_ptr(),
                    update,
                    (&mut *version as *mut Arc<AtomicU32>).cast(),
                    &mut raw,
                    i32::from(document),
                )
            })?;
            Ok(Self {
                raw: NonNull::new(raw).ok_or("UPnP 设备注册失败")?,
                _update: version,
                udn: string(&format!("uuid:{id}"))?,
            })
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub fn notify(&mut self, version: u32) -> Result<(), String> {
        check(unsafe { lap_upnp_notify(self.raw.as_ptr(), self.udn.as_ptr(), version) })
    }
    pub async fn close(self) {
        let _ = tokio::task::spawn_blocking(move || drop(self)).await;
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        unsafe { lap_upnp_unpublish(self.raw.as_ptr()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sdk_binds_the_os_default_interface_instead_of_the_first_virtual_adapter() {
        let interface = netdev::get_default_interface().unwrap();
        let bound = tokio::task::spawn_blocking(address).await.unwrap().unwrap();
        assert!(
            interface.ipv4.iter().any(|network| network.addr() == bound),
            "SDK bound {bound}, OS default interface is {} ({:?})",
            interface.name,
            interface.ipv4
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn sdk_discovers_a_standard_renderer_and_unregister_removes_it() {
        let address = tokio::task::spawn_blocking(super::address)
            .await
            .unwrap()
            .unwrap();
        let listener = tokio::net::TcpListener::bind((address, 0)).await.unwrap();
        let location = format!("http://{}/description.xml", listener.local_addr().unwrap());
        let id = "3bdba921-3f9c-4cab-8f7e-9ebf503c2e06";
        let description = Arc::new(format!(
            "<root xmlns=\"urn:schemas-upnp-org:device-1-0\"><specVersion><major>1</major><minor>0</minor></specVersion><device><deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType><friendlyName>Lap standard renderer fixture</friendlyName><manufacturer>Lap tests</manufacturer><modelName>Renderer</modelName><UDN>uuid:{id}</UDN><serviceList><service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType><serviceId>urn:upnp-org:serviceId:AVTransport</serviceId><SCPDURL>transport.xml</SCPDURL><controlURL>control</controlURL><eventSubURL>events</eventSubURL></service></serviceList></device></root>"
        ));
        let (stop, mut stopped) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut stopped => break,
                    accepted = listener.accept() => {
                        let (stream, _) = accepted.unwrap();
                        let description = description.clone();
                        tokio::spawn(super::super::http::serve(stream, move |_| {
                            let description = description.clone();
                            async move { super::super::http::reply("200 OK", "text/xml", description.as_bytes(), &[], false) }
                        }));
                    }
                }
            }
        });
        let device = Device::publish(
            location.clone(),
            id.into(),
            "",
            Arc::new(AtomicU32::new(1)),
            false,
        )
        .await
        .unwrap();
        assert!(
            discover()
                .await
                .unwrap()
                .iter()
                .any(|(url, peer)| url == &location && peer == &address)
        );
        device.close().await;
        assert!(
            !discover()
                .await
                .unwrap()
                .iter()
                .any(|(url, _)| url == &location)
        );
        let _ = stop.send(());
        server.await.unwrap();
    }
}
