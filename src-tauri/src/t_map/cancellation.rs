use once_cell::sync::Lazy;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Notify;
struct Signal {
    cancelled: AtomicBool,
    notify: Notify,
}
static REQUESTS: Lazy<Mutex<HashMap<String, (Instant, Arc<Signal>)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
pub struct Ticket {
    id: Option<String>,
    signal: Arc<Signal>,
}
impl Ticket {
    pub async fn cancelled(&self) {
        if self.signal.cancelled.load(Ordering::Acquire) {
            return;
        }
        let wait = self.signal.notify.notified();
        if self.signal.cancelled.load(Ordering::Acquire) {
            return;
        }
        wait.await;
    }
}
impl Drop for Ticket {
    fn drop(&mut self) {
        if let Some(id) = &self.id {
            if let Ok(mut requests) = REQUESTS.lock() {
                requests.remove(id);
            }
        }
    }
}
fn valid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
fn signal() -> Arc<Signal> {
    Arc::new(Signal {
        cancelled: AtomicBool::new(false),
        notify: Notify::new(),
    })
}
pub fn start(id: Option<&str>) -> Result<Ticket, String> {
    let Some(id) = id else {
        return Ok(Ticket {
            id: None,
            signal: signal(),
        });
    };
    if !valid(id) {
        return Err("Invalid map tile request ID".into());
    }
    let mut requests = REQUESTS
        .lock()
        .map_err(|_| "Map request state unavailable")?;
    requests.retain(|_, (created, signal)| {
        created.elapsed() < Duration::from_secs(180) || Arc::strong_count(signal) > 1
    });
    if requests.len() >= 8192 {
        return Err("Too many pending map tile requests".into());
    }
    let item = requests
        .entry(id.into())
        .or_insert_with(|| (Instant::now(), signal()))
        .1
        .clone();
    Ok(Ticket {
        id: Some(id.into()),
        signal: item,
    })
}
pub fn cancel(id: &str) -> Result<(), String> {
    if !valid(id) {
        return Err("Invalid map tile request ID".into());
    }
    let mut requests = REQUESTS
        .lock()
        .map_err(|_| "Map request state unavailable")?;
    if requests.len() >= 8192 && !requests.contains_key(id) {
        return Ok(());
    }
    let item = &requests
        .entry(id.into())
        .or_insert_with(|| (Instant::now(), signal()))
        .1;
    item.cancelled.store(true, Ordering::Release);
    item.notify.notify_one();
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancelled_tiles_release_waiters_and_can_be_cancelled_before_start() {
        let a = start(Some("live-tile-test")).unwrap();
        cancel("live-tile-test").unwrap();
        tokio::time::timeout(Duration::from_millis(100), a.cancelled())
            .await
            .unwrap();
        cancel("queued-tile-test").unwrap();
        let b = start(Some("queued-tile-test")).unwrap();
        tokio::time::timeout(Duration::from_millis(100), b.cancelled())
            .await
            .unwrap();
        assert!(start(Some("../unsafe")).is_err());
    }
}
