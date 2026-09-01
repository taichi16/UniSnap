use std::time::{Duration, Instant};

/// Controls video frame cadence and produces MP4 timestamps in milliseconds.
pub struct RecordingClock {
    started_at: Instant,
    next_frame_at: Instant,
    interval: Duration,
    minimum_sample_duration_ms: u64,
}

impl RecordingClock {
    pub fn new(fps: u32) -> Self {
        let safe_fps = fps.max(1);
        let interval = Duration::from_secs_f64(1.0 / safe_fps as f64);
        Self {
            started_at: Instant::now(),
            next_frame_at: Instant::now(),
            interval,
            minimum_sample_duration_ms: 1_000 / safe_fps as u64,
        }
    }

    pub fn should_capture(&mut self) -> bool {
        let now = Instant::now();
        if now < self.next_frame_at {
            return false;
        }
        // Advance from the planned deadline, not from the time this frame
        // happened to finish. Otherwise capture/crop/encode overhead is added
        // to every 33 ms interval and a requested 30 FPS drifts toward 20 FPS.
        while self.next_frame_at <= now {
            self.next_frame_at += self.interval;
        }
        true
    }

    pub fn elapsed_millis(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }

    pub fn final_sample_duration(&self, timestamp: u64) -> u32 {
        sample_duration(
            timestamp,
            self.elapsed_millis(),
            self.minimum_sample_duration_ms,
        )
    }
}

pub fn sample_duration(start: u64, end: u64, minimum_ms: u64) -> u32 {
    end.saturating_sub(start)
        .max(minimum_ms)
        .clamp(1, u32::MAX as u64) as u32
}

pub fn elapsed_sample_duration(start: u64, end: u64) -> u32 {
    sample_duration(start, end, 1)
}

#[cfg(test)]
mod tests {
    use super::{elapsed_sample_duration, sample_duration};

    #[test]
    fn sample_duration_uses_elapsed_time_when_available() {
        assert_eq!(sample_duration(100, 142, 33), 42);
    }

    #[test]
    fn sample_duration_applies_minimum_for_last_frame() {
        assert_eq!(sample_duration(100, 101, 33), 33);
    }

    #[test]
    fn sample_duration_handles_clock_regression() {
        assert_eq!(elapsed_sample_duration(200, 100), 1);
    }
}
