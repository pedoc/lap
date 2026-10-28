//! SDK discovery runs in a short-lived helper because libupnp has one global
//! interface per process. Searching another NIC must not tear down active shares.
use super::native;
use serde::{Deserialize, Serialize};
use std::{io::Write, net::Ipv4Addr, process::Stdio, time::Duration};
use tokio_util::sync::CancellationToken;

const WORKER: &str = "--lap-dlna-discover";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableReason {
    Disconnected,
    NoIpv4,
    NoMulticast,
    NoLanIpv4,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interface {
    pub id: String,
    pub name: String,
    pub addresses: Vec<String>,
    pub is_default: bool,
    pub unavailable_reason: Option<UnavailableReason>,
}

pub fn interfaces() -> Vec<Interface> {
    inventory()
        .into_iter()
        .filter(|interface| interface.unavailable_reason.is_none())
        .collect()
}

fn describe(interface: netdev::Interface, default: Option<&str>) -> Interface {
    let unavailable_reason = if !interface.is_up() {
        Some(UnavailableReason::Disconnected)
    } else if interface.ipv4.is_empty() {
        Some(UnavailableReason::NoIpv4)
    } else if !interface.is_multicast() {
        Some(UnavailableReason::NoMulticast)
    } else if !interface
        .ipv4
        .iter()
        .any(|network| super::protocol::lan_address(network.addr()))
    {
        Some(UnavailableReason::NoLanIpv4)
    } else {
        None
    };
    // Unavailable adapters retain diagnostic addresses without becoming usable.
    let addresses = interface
        .ipv4
        .iter()
        .filter(|network| {
            unavailable_reason.is_some() || super::protocol::lan_address(network.addr())
        })
        .map(ToString::to_string)
        .collect();
    Interface {
        is_default: default == Some(interface.name.as_str()),
        name: interface
            .friendly_name
            .unwrap_or_else(|| interface.name.clone()),
        id: interface.name,
        addresses,
        unavailable_reason,
    }
}

pub fn inventory() -> Vec<Interface> {
    let default = netdev::get_default_interface()
        .ok()
        .map(|interface| interface.name);
    let mut interfaces = netdev::get_interfaces()
        .into_iter()
        .filter(|interface| !interface.is_loopback())
        .map(|interface| describe(interface, default.as_deref()))
        .collect::<Vec<_>>();
    interfaces.sort_by(|a, b| {
        a.unavailable_reason
            .is_some()
            .cmp(&b.unavailable_reason.is_some())
            .then_with(|| {
                b.is_default
                    .cmp(&a.is_default)
                    .then_with(|| a.name.cmp(&b.name))
            })
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
    fn inventory_keeps_unavailable_adapters_and_explains_each_reason() {
        use netdev::interface::flags::{IFF_MULTICAST, IFF_UP};
        let mut wired = netdev::Interface::dummy();
        wired.name = "wired".into();
        wired.friendly_name = Some("以太网".into());
        wired.flags = IFF_MULTICAST as u32;
        wired.ipv4 = vec!["169.254.76.215/16".parse().unwrap()];
        let disconnected = describe(wired.clone(), None);
        assert_eq!(disconnected.name, "以太网");
        assert_eq!(disconnected.addresses, ["169.254.76.215/16"]);
        assert_eq!(
            disconnected.unavailable_reason,
            Some(UnavailableReason::Disconnected)
        );
        wired.flags |= IFF_UP as u32;
        wired.ipv4.clear();
        assert_eq!(
            describe(wired.clone(), None).unavailable_reason,
            Some(UnavailableReason::NoIpv4)
        );
        wired.ipv4 = vec!["8.8.8.8/24".parse().unwrap()];
        assert_eq!(
            describe(wired.clone(), None).unavailable_reason,
            Some(UnavailableReason::NoLanIpv4)
        );
        wired.ipv4 = vec!["192.168.1.2/24".parse().unwrap()];
        wired.flags = IFF_UP as u32;
        assert_eq!(
            describe(wired.clone(), None).unavailable_reason,
            Some(UnavailableReason::NoMulticast)
        );
        wired.flags |= IFF_MULTICAST as u32;
        let connected = describe(wired, Some("wired"));
        assert!(connected.unavailable_reason.is_none());
        assert!(connected.is_default);
    }

    #[test]
    fn inventory_lists_every_os_adapter_and_usable_interfaces_still_exclude_disconnected_ones() {
        let listed = inventory();
        for adapter in netdev::get_interfaces()
            .into_iter()
            .filter(|adapter| !adapter.is_loopback())
        {
            let item = listed
                .iter()
                .find(|item| item.id == adapter.name)
                .expect("OS adapter must remain visible");
            if !adapter.is_up() {
                assert_eq!(
                    item.unavailable_reason,
                    Some(UnavailableReason::Disconnected)
                );
            }
            println!(
                "DLNA_INTERFACE {} {:?} {:?}",
                item.name, item.addresses, item.unavailable_reason
            );
        }
        assert!(
            interfaces()
                .iter()
                .all(|interface| interface.unavailable_reason.is_none())
        );
    }

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
