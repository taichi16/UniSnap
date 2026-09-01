use std::sync::{atomic::AtomicBool, mpsc, Arc, Mutex};

pub struct RecordingState<T>(Mutex<Option<RecordingSession<T>>>);

pub struct RecordingSession<T> {
    pub stop: Arc<AtomicBool>,
    pub finished: mpsc::Receiver<Result<T, String>>,
}

pub enum RecordingPoll<T> {
    Inactive,
    Active { stopping: bool },
    Finished(Result<T, String>),
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

    pub fn install(&self, session: RecordingSession<T>) -> Result<(), String>
    where
        T: Send + 'static,
    {
        let mut active = self.0.lock().map_err(|_| "無法鎖定錄影狀態".to_string())?;
        if active.is_some() {
            session
                .stop
                .store(true, std::sync::atomic::Ordering::SeqCst);
            // The rejected worker must still be reaped, but duplicate-start
            // handling must never block the UI or detach an untracked worker.
            std::thread::spawn(move || {
                let _ = session.finished.recv();
            });
            return Err("已有錄影正在進行中".into());
        }
        *active = Some(session);
        Ok(())
    }

    pub fn request_stop(&self) -> Result<(), String> {
        let active = self.0.lock().map_err(|_| "無法鎖定錄影狀態".to_string())?;
        let session = active
            .as_ref()
            .ok_or_else(|| "目前沒有進行中的錄影".to_string())?;
        session
            .stop
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    pub fn poll(&self) -> Result<RecordingPoll<T>, String> {
        let mut active = self.0.lock().map_err(|_| "無法鎖定錄影狀態".to_string())?;
        let Some(session) = active.as_ref() else {
            return Ok(RecordingPoll::Inactive);
        };
        let stopping = session.stop.load(std::sync::atomic::Ordering::SeqCst);
        match session.finished.try_recv() {
            Ok(result) => {
                *active = None;
                Ok(RecordingPoll::Finished(result))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(RecordingPoll::Active { stopping }),
            Err(mpsc::TryRecvError::Disconnected) => {
                *active = None;
                Ok(RecordingPoll::Finished(Err(
                    "錄影背景工作異常結束，未回傳存檔結果".to_string(),
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RecordingPoll, RecordingSession, RecordingState};
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
    fn keeps_session_while_finalizing_and_clears_after_result() {
        let state = RecordingState::new();
        let (sender, receiver) = mpsc::channel();
        state
            .install(RecordingSession {
                stop: Arc::new(AtomicBool::new(false)),
                finished: receiver,
            })
            .unwrap();
        state.request_stop().unwrap();
        assert!(matches!(
            state.poll().unwrap(),
            RecordingPoll::Active { stopping: true }
        ));
        sender.send(Ok(7)).unwrap();
        assert!(matches!(
            state.poll().unwrap(),
            RecordingPoll::Finished(Ok(7))
        ));
        assert!(!state.is_active().unwrap());
    }
}
