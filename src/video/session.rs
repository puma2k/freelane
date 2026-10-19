//! One viewer-owned session. Only its control worker can stop its exact binding.
use super::{fresh_binding, require_capability, VideoBroker};
use crate::{agent, Error, Result};
use freelane_agent_protocol::{
    video::{Binding, Record},
    video_control::{Operation, Parameters},
    WindowTarget,
};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
struct Control {
    stop: AtomicBool,
    key: AtomicBool,
    failed: AtomicBool,
    finished: Arc<AtomicBool>,
    wake: SyncSender<()>,
}
pub struct LiveSession {
    broker: Option<Arc<VideoBroker>>,
    binding: Binding,
    control: Arc<Control>,
}
impl LiveSession {
    /// Blocking negotiation; call from the viewer's existing background task.
    pub fn start(
        config: &Path,
        target: WindowTarget,
        wakeup: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self> {
        require_capability(config)?;
        let binding = fresh_binding(1, 1)?;
        let broker = Arc::new(VideoBroker::connect_with_wakeup(
            config,
            target.clone(),
            binding,
            wakeup.clone(),
        )?);
        let parameters = Parameters {
            target,
            binding,
            width: 1280,
            height: 720,
            fps: 30,
        };
        if let Err(error) = agent::video_request(config, &Operation::Start { parameters }) {
            // An uncertain Start is never repeated. Stop only this fresh nonce.
            let _ = agent::video_request(config, &Operation::Stop { binding });
            return Err(error);
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let control = Arc::new(Control {
            stop: AtomicBool::new(false),
            key: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            finished: Arc::new(AtomicBool::new(false)),
            wake: tx,
        });
        let owned = control.clone();
        let lease = broker.clone();
        let path = config.to_owned();
        let worker = thread::Builder::new()
            .name("freelane-video-control".into())
            .spawn(move || {
                let mut next_key = Instant::now();
                loop {
                    if owned.stop.load(Ordering::Acquire) {
                        break;
                    }
                    if owned.failed.load(Ordering::Acquire) {
                        if rx.recv().is_err() {
                            break;
                        }
                        continue;
                    }
                    let pending = owned.key.load(Ordering::Acquire);
                    if pending && Instant::now() >= next_key {
                        owned.key.store(false, Ordering::Release);
                        if agent::video_request(&path, &Operation::Keyframe { binding }).is_err() {
                            owned.failed.store(true, Ordering::Release);
                            wakeup();
                            continue;
                        }
                        next_key = Instant::now() + Duration::from_millis(500);
                        continue;
                    }
                    if pending {
                        match rx.recv_timeout(next_key.saturating_duration_since(Instant::now())) {
                            Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    } else if rx.recv().is_err() {
                        break;
                    }
                }
                let _ = agent::video_request(&path, &Operation::Stop { binding });
                drop(lease);
                owned.finished.store(true, Ordering::Release);
                wakeup();
            });
        if let Err(error) = worker {
            let _ = agent::video_request(config, &Operation::Stop { binding });
            return Err(Error(format!("Video control unavailable: {error}")));
        }
        Ok(Self {
            broker: Some(broker),
            binding,
            control,
        })
    }
    pub fn target(&self) -> &WindowTarget {
        self.broker.as_ref().unwrap().target()
    }
    pub fn binding(&self) -> Binding {
        self.binding
    }
    pub fn configuration(&self) -> Option<Record> {
        self.broker.as_ref().unwrap().configuration()
    }
    pub fn pop(&self) -> Option<Record> {
        self.broker.as_ref().unwrap().pop()
    }
    pub fn ended(&self) -> bool {
        self.broker.as_ref().unwrap().ended() || self.control.failed.load(Ordering::Acquire)
    }
    pub fn request_keyframe(&self) {
        self.control.key.store(true, Ordering::Release);
        let _ = self.control.wake.try_send(());
    }
    pub fn recover_if_needed(&self) {
        if self.broker.as_ref().unwrap().take_keyframe_request() {
            self.request_keyframe();
        }
    }
    pub fn retirement(&self) -> Arc<AtomicBool> {
        self.control.finished.clone()
    }
}
impl Drop for LiveSession {
    fn drop(&mut self) {
        // Release the UI reference before waking retirement: the owner worker
        // remains the final broker owner and performs its join off the UI thread.
        self.broker.take();
        self.control.stop.store(true, Ordering::Release);
        let _ = self.control.wake.try_send(());
    }
}
