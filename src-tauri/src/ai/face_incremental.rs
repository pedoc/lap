//! Batch grouping without rescanning pixels or moving existing person anchors.
use std::time::Duration;

#[derive(Default)]
pub struct GroupingCadence {
    pending_images: usize,
    published: bool,
}
impl GroupingCadence {
    pub fn committed(&mut self, regions: usize) {
        if regions > 0 {
            self.pending_images += 1;
        }
    }
    pub fn due(&self, elapsed: Duration) -> bool {
        self.pending_images > 0
            && (!self.published || self.pending_images >= 16 || elapsed >= Duration::from_secs(2))
    }
    pub fn published(&mut self) {
        self.pending_images = 0;
        self.published = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_result_is_immediate_then_batches_are_size_or_time_bounded() {
        let mut cadence = GroupingCadence::default();
        cadence.committed(0);
        assert!(!cadence.due(Duration::from_secs(30)));
        cadence.committed(1);
        assert!(cadence.due(Duration::ZERO));
        cadence.published();
        cadence.committed(2);
        assert!(!cadence.due(Duration::from_millis(100)));
        assert!(cadence.due(Duration::from_secs(2)));
        for _ in 1..16 {
            cadence.committed(1);
        }
        assert!(cadence.due(Duration::ZERO));
        cadence.published();
        assert!(!cadence.due(Duration::from_secs(30)));
    }
}
