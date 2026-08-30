use std::time::Instant;

/// Per-session counters for diagnosing long-capture quality.
#[derive(Debug)]
pub struct ScrollMetrics {
    started_at: Instant,
    pub capture_steps: usize,
    pub settle_attempts: usize,
    pub accepted_frames: usize,
    pub rejected_frames: usize,
    pub alignment_failures: usize,
    pub final_height: u32,
}

impl ScrollMetrics {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
            capture_steps: 0,
            settle_attempts: 0,
            accepted_frames: 1,
            rejected_frames: 0,
            alignment_failures: 0,
            final_height: 0,
        }
    }

    pub fn summary(&self, outcome: &str) -> String {
        format!(
            "[scroll] session-summary outcome={} steps={} accepted={} rejected={} alignment_failures={} settle_attempts={} height={} elapsed_ms={}",
            outcome, self.capture_steps, self.accepted_frames, self.rejected_frames,
            self.alignment_failures, self.settle_attempts, self.final_height,
            self.started_at.elapsed().as_millis()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::ScrollMetrics;

    #[test]
    fn summary_contains_session_counters() {
        let mut metrics = ScrollMetrics::new();
        metrics.capture_steps = 3;
        metrics.accepted_frames = 4;
        metrics.final_height = 1200;
        let summary = metrics.summary("saved");
        assert!(summary.contains("outcome=saved"));
        assert!(summary.contains("steps=3"));
        assert!(summary.contains("accepted=4"));
        assert!(summary.contains("height=1200"));
    }
}
