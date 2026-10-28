//! Each interface owns an SDK process; media stays in the parent HTTP service.
use super::{native, network, upnp};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    net::Ipv4Addr,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_util::sync::CancellationToken;

const WORKER: &str = "--lap-dlna-publish";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Endpoint {
    pub address: Ipv4Addr,
    pub event_base: String,
}

#[derive(Serialize, Deserialize)]
struct Configuration {
    id: String,
    title: String,
    port: u16,
    update: u32,
}

fn reply(result: &Result<Endpoint, String>) -> Result<(), String> {
    let mut output = std::io::stdout().lock();
    serde_json::to_writer(&mut output, result).map_err(|error| error.to_string())?;
    output
        .write_all(b"\n")
        .and_then(|_| output.flush())
        .map_err(|error| error.to_string())
}

pub async fn worker_entry() -> bool {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(WORKER)) {
        return false;
    }
    let result = async {
        let id = args
            .next()
            .and_then(|id| id.into_string().ok())
            .ok_or("缺少发布网卡")?;
        if !network::interfaces()
            .iter()
            .any(|interface| interface.id == id)
        {
            return Err("发布网卡不可用".into());
        }
        native::use_interface(id)?;
        let mut input = BufReader::new(tokio::io::stdin()).lines();
        let config = input
            .next_line()
            .await
            .map_err(|error| error.to_string())?
            .ok_or("发布已取消")?;
        let config: Configuration =
            serde_json::from_str(&config).map_err(|error| error.to_string())?;
        let endpoint = Endpoint {
            address: native::address()?,
            event_base: native::endpoint()?,
        };
        let base = format!("http://{}:{}/{}", endpoint.address, config.port, config.id);
        let description = upnp::description(&config.id, &config.title, &endpoint.event_base, &base);
        let version = Arc::new(AtomicU32::new(config.update));
        let mut device = native::Device::publish(
            description,
            config.id,
            upnp::PROTOCOLS,
            version.clone(),
            true,
        )
        .await?;
        // The SDK serves its own description, so registration needs no parent handshake.
        let result = reply(&Ok(endpoint));
        if result.is_ok() {
            while let Ok(Some(update)) = input.next_line().await {
                let Ok(update) = serde_json::from_str::<u32>(&update) else {
                    break;
                };
                version.store(update, Ordering::Release);
                if device.notify(update).is_err() {
                    break;
                }
            }
        }
        device.close().await;
        result
    }
    .await;
    if let Err(error) = result {
        let _ = reply(&Err(error));
    }
    true
}

pub(super) struct Publication {
    pub interface: network::Interface,
    pub endpoint: Endpoint,
    child: tokio::process::Child,
    input: Option<tokio::process::ChildStdin>,
}

impl Publication {
    pub async fn start(
        interface: network::Interface,
        id: &str,
        title: &str,
        port: u16,
        update: u32,
        cancel: &CancellationToken,
    ) -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        #[cfg(test)]
        let exe = std::env::var_os("LAP_DLNA_TEST_WORKER")
            .map(std::path::PathBuf::from)
            .unwrap_or(exe);
        Self::start_with_exe(exe, interface, id, title, port, update, cancel).await
    }

    pub async fn start_with_exe(
        exe: std::path::PathBuf,
        interface: network::Interface,
        id: &str,
        title: &str,
        port: u16,
        update: u32,
        cancel: &CancellationToken,
    ) -> Result<Self, String> {
        if cancel.is_cancelled() {
            return Err("共享已取消".into());
        }
        let mut command = network::worker_command(exe, WORKER, &interface.id);
        let mut child = command
            .stdin(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("无法启动媒体发布：{error}"))?;
        let mut input = child.stdin.take().ok_or("媒体发布输入不可用")?;
        let output = child.stdout.take().ok_or("媒体发布输出不可用")?;
        let mut config = serde_json::to_vec(&Configuration {
            id: id.into(),
            title: title.into(),
            port,
            update,
        })
        .map_err(|error| error.to_string())?;
        config.push(b'\n');
        let result = tokio::select! {
            _ = cancel.cancelled() => Err("共享已取消".into()),
            result = tokio::time::timeout(Duration::from_secs(10), async {
                input.write_all(&config).await.map_err(|error| error.to_string())?;
                let response = BufReader::new(output).lines().next_line().await.map_err(|error| error.to_string())?.ok_or("媒体发布进程已退出")?;
                serde_json::from_str::<Result<Endpoint, String>>(&response).map_err(|_| "媒体发布响应无效".to_owned())?
            }) => result.map_err(|_| "此网络媒体发布超时".to_owned()).and_then(|result| result),
        };
        let mut publication = Self {
            interface,
            endpoint: Endpoint {
                address: Ipv4Addr::UNSPECIFIED,
                event_base: String::new(),
            },
            child,
            input: Some(input),
        };
        match result {
            Ok(endpoint)
                if publication.interface.addresses.iter().any(|address| {
                    address.split('/').next() == Some(endpoint.address.to_string().as_str())
                }) =>
            {
                publication.endpoint = endpoint;
                Ok(publication)
            }
            result => {
                publication.close().await;
                Err(result
                    .err()
                    .unwrap_or_else(|| "媒体发布网卡地址已变化".into()))
            }
        }
    }

    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub async fn update(&mut self, update: u32) -> Result<(), String> {
        let input = self.input.as_mut().ok_or("媒体发布已关闭")?;
        tokio::time::timeout(
            Duration::from_secs(1),
            input.write_all(format!("{update}\n").as_bytes()),
        )
        .await
        .map_err(|_| "媒体事件通知超时".to_owned())?
        .map_err(|error| error.to_string())
    }

    pub async fn close(&mut self) {
        // EOF lets libupnp send byebye and unregister subscriptions before exit.
        self.input.take();
        if tokio::time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.kill().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancelled_publication_does_not_spawn_a_worker() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let interface = network::Interface {
            id: "missing".into(),
            name: "Test".into(),
            addresses: vec!["192.168.1.2/24".into()],
            is_default: false,
            unavailable_reason: None,
        };
        let result = Publication::start_with_exe(
            "missing-publisher".into(),
            interface,
            "test",
            "Test",
            1234,
            1,
            &cancel,
        )
        .await;
        assert!(result.err().unwrap().contains("取消"));
    }
}
