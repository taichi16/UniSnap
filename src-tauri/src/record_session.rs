//! 可觀察的錄影生命週期，不負責影格編碼或平台擷取。

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingPhase {
    Idle,
    Preparing,
    Recording,
    Paused,
    Stopping,
    Finished,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordingLifecycle {
    phase: RecordingPhase,
}

impl RecordingLifecycle {
    pub fn new() -> Self {
        Self {
            phase: RecordingPhase::Idle,
        }
    }

    #[allow(dead_code)]
    pub fn phase(&self) -> RecordingPhase {
        self.phase
    }

    pub fn transition(&mut self, next: RecordingPhase) -> Result<(), String> {
        let allowed = matches!(
            (self.phase, next),
            (RecordingPhase::Idle, RecordingPhase::Preparing)
                | (RecordingPhase::Preparing, RecordingPhase::Recording)
                | (RecordingPhase::Recording, RecordingPhase::Paused)
                | (RecordingPhase::Paused, RecordingPhase::Recording)
                | (RecordingPhase::Recording, RecordingPhase::Stopping)
                | (RecordingPhase::Paused, RecordingPhase::Stopping)
                | (RecordingPhase::Stopping, RecordingPhase::Finished)
                | (RecordingPhase::Stopping, RecordingPhase::Failed)
                | (RecordingPhase::Preparing, RecordingPhase::Failed)
                | (RecordingPhase::Recording, RecordingPhase::Failed)
                | (RecordingPhase::Paused, RecordingPhase::Failed)
        );
        if !allowed {
            return Err(format!(
                "不允許的錄影狀態轉移：{:?} -> {:?}",
                self.phase, next
            ));
        }
        self.phase = next;
        Ok(())
    }
}

impl Default for RecordingLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_recording_lifecycle() {
        let mut lifecycle = RecordingLifecycle::new();
        for phase in [
            RecordingPhase::Preparing,
            RecordingPhase::Recording,
            RecordingPhase::Paused,
            RecordingPhase::Recording,
            RecordingPhase::Stopping,
            RecordingPhase::Finished,
        ] {
            lifecycle.transition(phase).unwrap();
        }
        assert_eq!(lifecycle.phase(), RecordingPhase::Finished);
    }

    #[test]
    fn rejects_skipping_preparation() {
        let mut lifecycle = RecordingLifecycle::new();
        let error = lifecycle.transition(RecordingPhase::Recording).unwrap_err();
        assert!(error.contains("不允許的錄影狀態轉移"));
    }
}
