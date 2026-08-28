use std::sync::{atomic::AtomicBool, mpsc, Arc, Mutex};
use std::time::Duration;

pub struct RecordingState<T>(Mutex<Option<RecordingSession<T>>>);

pub struct RecordingSession<T> {
    pub stop: Arc<AtomicBool>,
    pub finished: mpsc::Receiver<Result<T, String>>,
}

impl<T> RecordingState<T> {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }

    pub fn is_active(&self) -> Result<bool, String> {
        self.0
            .lock()
            .map(|active| active.is_some())
            .map_err(|_| "無法鎖定錄影狀態".to_string())
    }

    pub fn install(&self, session: RecordingSession<T>) -> Result<(), String> {
        let mut active = self.0.lock().map_err(|_| "無法鎖定錄影狀態".to_string())?;
        if active.is_some() {
            session
                .stop
                .store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = session.finished.recv_timeout(Duration::from_secs(30));
            return Err("已有錄影正在進行中".into());
        }
        *active = Some(session);
        Ok(())
    }

    pub fn take(&self) -> Result<RecordingSession<T>, String> {
        self.0
            .lock()
            .map_err(|_| "無法鎖定錄影狀態".to_string())?
            .take()
            .ok_or_else(|| "目前沒有進行中的錄影".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{RecordingSession, RecordingState};
    use std::sync::{atomic::AtomicBool, mpsc, Arc};

    fn session() -> RecordingSession<u8> {
        let (_sender, receiver) = mpsc::channel();
        RecordingSession {
            stop: Arc::new(AtomicBool::new(false)),
            finished: receiver,
        }
    }

    #[test]
    fn prevents_duplicate_active_sessions() {
        let state = RecordingState::new();
        state.install(session()).unwrap();
        assert!(state.is_active().unwrap());
        assert!(state.install(session()).is_err());
    }

    #[test]
    fn takes_session_and_reports_empty_state() {
        let state = RecordingState::new();
        let taken = state.install(session()).and_then(|_| state.take());
        assert!(taken.is_ok());
        assert!(!state.is_active().unwrap());
        assert!(state.take().is_err());
    }
}
