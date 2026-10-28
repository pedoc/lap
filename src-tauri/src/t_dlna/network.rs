//! SDK discovery runs in a short-lived helper because libupnp has one global
//! interface per process. Searching another NIC must not tear down active shares.
use super::native;
use serde::{Deserialize, Serialize};
use std::{io::Write, net::Ipv4Addr, process::Stdio, time::Duration};
use tokio_util::sync::CancellationToken;

const WORKER: &str = "--lap-dlna-discover";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interface {
    pub id: String,
    pub name: String,
    pub addresses: Vec<String>,
    pub is_default: bool,
}

pub fn interfaces() -> Vec<Interface> {
    let default = netdev::get_default_interface()
        .ok()
        .map(|interface| interface.name);
    let mut interfaces = netdev::get_interfaces()
        .into_iter()
        .filter(|interface| {
            interface.is_up() && !interface.is_loopback() && interface.is_multicast()
        })
        .filter_map(|interface| {
            let addresses = interface
                .ipv4
                .iter()
                .filter(|network| super::protocol::lan_address(network.addr()))
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            (!addresses.is_empty()).then(|| Interface {
                is_default: default.as_ref() == Some(&interface.name),
                name: interface
                    .friendly_name
                    .unwrap_or_else(|| interface.name.clone()),
                id: interface.name,
                addresses,
            })
        })
        .collect::<Vec<_>>();
    interfaces.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then_with(|| a.name.cmp(&b.name))
    });
    interfaces
}

pub async fn worker_entry() -> bool {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(WORKER)) {
        return false;
    }
    let result = async {
        let id = args
            .next()
            .and_then(|value| value.into_string().ok())
            .ok_or("缺少搜索网卡")?;
        if !interfaces().iter().any(|interface| interface.id == id) {
            return Err("搜索网卡不可用".into());
        }
        native::use_interface(id)?;
        if args.next().as_deref() == Some(std::ffi::OsStr::new(super::upnp::MEDIA_SERVER)) {
            native::discover_services(&[super::upnp::MEDIA_SERVER]).await
        } else {
            native::discover().await
        }
    }
    .await;
    // This branch runs before Tauri, database, and library initialization.
    if let Ok(json) = serde_json::to_vec(&result) {
        let _ = std::io::stdout().lock().write_all(&json);
    }
    true
}

pub async fn discover(
    interface: &Interface,
    cancel: &CancellationToken,
) -> Result<Vec<(String, Ipv4Addr)>, String> {
    discover_with_exe(
        std::env::current_exe().map_err(|error| error.to_string())?,
        interface,
        cancel,
    )
    .await
}

pub(super) async fn discover_with_exe(
    exe: std::path::PathBuf,
    interface: &Interface,
    cancel: &CancellationToken,
) -> Result<Vec<(String, Ipv4Addr)>, String> {
    if cancel.is_cancelled() {
        return Err("搜索已取消".into());
    }
    let mut command = worker_command(exe, WORKER, &interface.id);
    command.stdin(Stdio::null()).stderr(Stdio::piped());
    let output = tokio::select! {
        _ = cancel.cancelled() => return Err("搜索已取消".into()),
        result = tokio::time::timeout(Duration::from_secs(10), command.output()) =>
            result.map_err(|_| "此网络搜索超时".to_owned())?.map_err(|error| format!("无法启动设备搜索：{error}"))?,
    };
    if !output.status.success() {
        return Err("此网络的设备搜索进程异常退出".into());
    }
    if output.stdout.len() > 512 * 1024 {
        return Err("设备搜索响应过大".into());
    }
    serde_json::from_slice::<Result<Vec<(String, Ipv4Addr)>, String>>(&output.stdout)
        .map_err(|_| "设备搜索响应无效".to_owned())?
}

pub(super) fn worker_command(
    exe: std::path::PathBuf,
    mode: &str,
    interface: &str,
) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(exe);
    command
        .arg(mode)
        .arg(interface)
        .stdout(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interface_picker_lists_local_networks_and_puts_the_default_first() {
        let interfaces = interfaces();
        assert!(!interfaces.is_empty());
        assert!(
            interfaces
                .iter()
                .all(|interface| !interface.id.is_empty() && !interface.addresses.is_empty())
        );
        if interfaces.iter().any(|interface| interface.is_default) {
            assert!(interfaces[0].is_default);
        }
    }

    #[tokio::test]
    async fn cancelled_search_does_not_start_a_helper() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = discover_with_exe(
            std::path::PathBuf::from("missing-dlna-helper"),
            &interfaces()[0],
            &cancel,
        )
        .await;
        assert!(result.unwrap_err().contains("取消"));
    }
}
