//! Shared inference lane; manual object work can wait for an automatic OCR yield.
use super::types::Task;
use std::sync::atomic::{AtomicU8, Ordering};
static OWNER: AtomicU8 = AtomicU8::new(0);
fn code(task: Task) -> u8 {
    match task {
        Task::Ocr => 1,
        Task::Detection => 2,
        Task::Face => 3,
        Task::Semantic => 4,
    }
}
pub struct Lease(u8);
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = OWNER.compare_exchange(self.0, 0, Ordering::SeqCst, Ordering::SeqCst);
    }
}
pub fn try_claim(task: Task) -> Result<Lease, String> {
    let id = code(task);
    OWNER
        .compare_exchange(0, id, Ordering::SeqCst, Ordering::SeqCst)
        .map(|_| Lease(id))
        .map_err(|_| {
            "Another image-analysis inference is running; stop it or wait for it to finish".into()
        })
}
pub fn busy() -> bool {
    OWNER.load(Ordering::SeqCst) != 0
}
pub fn running(task: Task) -> bool {
    OWNER.load(Ordering::SeqCst) == code(task)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_lane_serializes_capabilities_and_is_released_by_drop() {
        let ocr = try_claim(Task::Ocr).unwrap();
        assert!(busy());
        assert!(running(Task::Ocr));
        assert!(try_claim(Task::Detection).is_err());
        drop(ocr);
        let objects = try_claim(Task::Detection).unwrap();
        assert!(running(Task::Detection));
        assert!(try_claim(Task::Face).is_err());
        drop(objects);
        assert!(!busy());
    }
}
