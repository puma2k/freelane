//! Owned binary video reader and negotiated control; native backend/frontend integration is separate.
mod control;
mod reader;
mod session;
use crate::{qemu, Error, Result};
pub use control::{fresh_binding, require_capability};
use freelane_agent_protocol::{
    video::{read_record, Binding, Receiver, Record},
    WindowTarget,
};
use fs2::FileExt;
pub use session::LiveSession;
use std::{
    fs::{self, File, OpenOptions},
    net::Shutdown,
    os::unix::{
        fs::{FileTypeExt, MetadataExt, OpenOptionsExt},
        net::UnixStream,
    },
    path::Path,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};
struct State {
    receiver: Receiver,
    ended: bool,
    last_record: std::time::Instant,
}
pub struct VideoBroker {
    target: WindowTarget,
    state: Arc<Mutex<State>>,
    cancel: UnixStream,
    worker: Option<JoinHandle<()>>,
    _lease: File,
}
fn private_file(path: &Path, owner: u32, create: bool) -> std::io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(create)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    let metadata = file.metadata()?;
    // Older supervisors created 0644 locks under umask0022. Their parent is
    // verified0700; allow read bits, but never another user's write access.
    let forbidden = if create { 0o077 } else { 0o022 };
    if !metadata.is_file()
        || metadata.mode() & forbidden != 0
        || metadata.uid() != owner
        || metadata.nlink() != 1
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Video lease must be a private owned regular file",
        ));
    }
    Ok(file)
}
impl VideoBroker {
    /// Caller must first negotiate/authorize this exact target and binding on control.
    /// Video bytes cannot start capture, switch target, or relaunch an application.
    pub fn connect(config: &Path, target: WindowTarget, binding: Binding) -> Result<Self> {
        Self::connect_with_wakeup(config, target, binding, Arc::new(|| {}))
    }
    /// Wake observers after records and termination, outside the receiver lock.
    /// The callback must return promptly; it executes on the owned reader thread.
    pub fn connect_with_wakeup(
        config: &Path,
        target: WindowTarget,
        binding: Binding,
        wakeup: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self> {
        if target.handle == 0 || target.pid == 0 || target.created == 0 || target.incarnation == 0 {
            return Err(Error("Video requires a complete window identity".into()));
        }
        let receiver = Receiver::new(binding).map_err(|e| Error(e.to_string()))?;
        let setup = || -> std::io::Result<_> {
            let directory =
                qemu::runtime_dir(config).map_err(|e| std::io::Error::other(e.to_string()))?;
            let metadata = fs::symlink_metadata(&directory)?;
            let owner = fs::metadata(config)?.uid();
            if !metadata.is_dir() || metadata.mode() & 0o077 != 0 || metadata.uid() != owner {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Video runtime must be private and owned by VM owner",
                ));
            }
            let supervisor = private_file(&directory.join("lock"), owner, false)?;
            match supervisor.try_lock_exclusive() {
                Ok(()) => {
                    FileExt::unlock(&supervisor)?;
                    return Err(std::io::Error::other("Video requires a running VM"));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e),
            }
            let lease = private_file(&directory.join("video-client.lock"), owner, true)?;
            lease.try_lock_exclusive().map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    "Video stream is busy; use snapshots or close its current viewer",
                )
            })?;
            let socket = directory.join("video.sock");
            let metadata = fs::symlink_metadata(&socket)?;
            if !metadata.file_type().is_socket() || metadata.uid() != owner {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Invalid owned video socket",
                ));
            }
            let source = reader::connect(&socket)?;
            let (cancel, wake) = UnixStream::pair()?;
            Ok((lease, source, cancel, wake))
        };
        let (lease, source, cancel, wake) =
            setup().map_err(|e| Error(format!("Video unavailable: {e}")))?;
        let state = Arc::new(Mutex::new(State {
            receiver,
            ended: false,
            last_record: std::time::Instant::now(),
        }));
        let shared = state.clone();
        let worker = thread::Builder::new()
            .name("freelane-video".into())
            .spawn(move || {
                let mut source = reader::Reader::new(source, wake);
                while let Ok(Some(record)) = read_record(&mut source) {
                    source.finish_record();
                    let accepted = {
                        let mut state = shared.lock().unwrap();
                        let accepted = state.receiver.accept(record).is_ok();
                        if accepted {
                            state.last_record = std::time::Instant::now();
                        }
                        accepted
                    };
                    if !accepted {
                        break;
                    }
                    wakeup();
                }
                let mut state = shared.lock().unwrap();
                state.receiver.clear();
                state.ended = true;
                drop(state);
                wakeup();
            })
            .map_err(|e| Error(e.to_string()))?;
        Ok(Self {
            target,
            state,
            cancel,
            worker: Some(worker),
            _lease: lease,
        })
    }
    pub fn target(&self) -> &WindowTarget {
        &self.target
    }
    pub fn pop(&self) -> Option<Record> {
        self.state.lock().unwrap().receiver.pop()
    }
    pub fn configuration(&self) -> Option<Record> {
        self.state.lock().unwrap().receiver.configuration().cloned()
    }
    pub fn take_keyframe_request(&self) -> bool {
        self.state.lock().unwrap().receiver.take_keyframe_request()
    }
    pub fn silent_for(&self, duration: std::time::Duration) -> bool {
        self.state.lock().unwrap().last_record.elapsed() >= duration
    }
    pub fn ended(&self) -> bool {
        self.state.lock().unwrap().ended
    }
}
impl Drop for VideoBroker {
    fn drop(&mut self) {
        let _ = self.cancel.shutdown(Shutdown::Both);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
