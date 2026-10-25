use std::{
    collections::HashMap,
    future::Future,
    sync::{LazyLock, Mutex},
};
use tokio_util::sync::CancellationToken;

static REQUESTS: LazyLock<Mutex<HashMap<String, CancellationToken>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
pub struct Request {
    id: String,
    cancel: CancellationToken,
}
impl Request {
    pub fn cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
    pub fn new(id: Option<String>) -> Result<Self, String> {
        let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        if id.is_empty()
            || id.len() > 128
            || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("请求 ID 无效".into());
        }
        let cancel = CancellationToken::new();
        let mut requests = REQUESTS.lock().map_err(|_| "请求锁不可用")?;
        if requests.len() >= 32 || requests.contains_key(&id) {
            return Err("请求过多或 ID 重复".into());
        }
        requests.insert(id.clone(), cancel.clone());
        Ok(Self { id, cancel })
    }
    pub async fn run<T>(
        &self,
        future: impl Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        tokio::select! {biased;_=self.cancel.cancelled()=>Err("请求已取消".into()),result=future=>result}
    }
}
impl Drop for Request {
    fn drop(&mut self) {
        if let Ok(mut requests) = REQUESTS.lock() {
            requests.remove(&self.id);
        }
    }
}
#[tauri::command]
pub fn media_ai_cancel_request(request_id: String) -> Result<(), String> {
    if let Some(cancel) = REQUESTS
        .lock()
        .map_err(|_| "请求锁不可用")?
        .get(&request_id)
    {
        cancel.cancel();
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_drops_inflight_work_and_releases_id() {
        let id = uuid::Uuid::new_v4().to_string();
        let request = Request::new(Some(id.clone())).unwrap();
        assert!(Request::new(Some(id.clone())).is_err());
        media_ai_cancel_request(id.clone()).unwrap();
        assert_eq!(
            request
                .run(std::future::pending::<Result<(), String>>())
                .await
                .unwrap_err(),
            "请求已取消"
        );
        drop(request);
        assert!(Request::new(Some(id)).is_ok());
    }
}
