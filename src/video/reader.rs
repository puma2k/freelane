//! Cancellable descriptor waits; only an in-progress record has a deadline.
use std::{
    io::{self, Read},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{ffi::OsStrExt, net::UnixStream},
    },
    path::Path,
    time::{Duration, Instant},
};
fn timeout(deadline: Option<Instant>) -> io::Result<i32> {
    match deadline {
        None => Ok(-1),
        Some(end) => {
            let remaining = end.checked_duration_since(Instant::now()).ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "Video record deadline exceeded")
            })?;
            Ok(remaining
                .as_millis()
                .saturating_add(1)
                .min(i32::MAX as u128) as i32)
        }
    }
}
pub(super) fn connect(path: &Path) -> io::Result<UnixStream> {
    // A local listener with a full backlog must not hang the calling UI/supervisor.
    let bytes = path.as_os_str().as_bytes();
    // Zeroed sockaddr has no invalid bit patterns; pathname is copied with a trailing NUL.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.contains(&0) || bytes.len() >= address.sun_path.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid video socket path",
        ));
    }
    address.sun_family = libc::AF_UNIX as _;
    #[cfg(target_os = "macos")]
    {
        address.sun_len = std::mem::size_of_val(&address) as u8;
    }
    for (out, byte) in address.sun_path.iter_mut().zip(bytes) {
        *out = *byte as _;
    }
    // The returned descriptor is newly owned and immediately wrapped, including error paths.
    let raw = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let stream = UnixStream::from(fd);
    stream.set_nonblocking(true)?;
    // Borrowed descriptor/address; kernel does not retain the pointer.
    let result = unsafe {
        libc::connect(
            stream.as_raw_fd(),
            (&address as *const libc::sockaddr_un).cast(),
            std::mem::size_of_val(&address) as _,
        )
    };
    if result < 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EINPROGRESS) {
            return Err(error);
        }
        let end = Instant::now() + Duration::from_secs(2);
        loop {
            let mut descriptor = libc::pollfd {
                fd: stream.as_raw_fd(),
                events: libc::POLLOUT,
                revents: 0,
            };
            let result = unsafe { libc::poll(&mut descriptor, 1, timeout(Some(end))?) };
            if result > 0 {
                if let Some(error) = stream.take_error()? {
                    return Err(error);
                }
                break;
            }
            if result < 0 {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    return Err(error);
                }
            }
        }
    }
    Ok(stream)
}
pub(super) struct Reader {
    source: UnixStream,
    cancel: UnixStream,
    deadline: Option<Instant>,
}
impl Reader {
    pub fn new(source: UnixStream, cancel: UnixStream) -> Self {
        Self {
            source,
            cancel,
            deadline: None,
        }
    }
    pub fn finish_record(&mut self) {
        self.deadline = None;
    }
}
impl Read for Reader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        loop {
            let mut descriptors = [
                libc::pollfd {
                    fd: self.source.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: self.cancel.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            // Both descriptors are borrowed from live owned streams; cancellation wins a race.
            let result =
                unsafe { libc::poll(descriptors.as_mut_ptr(), 2, timeout(self.deadline)?) };
            if result < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if result == 0 {
                continue;
            }
            if descriptors[1].revents != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Video reader cancelled",
                ));
            }
            if descriptors[0].revents & libc::POLLNVAL != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Invalid video descriptor",
                ));
            }
            match self.source.read(bytes) {
                Ok(count) => {
                    if count > 0 && self.deadline.is_none() {
                        self.deadline = Some(Instant::now() + Duration::from_secs(2));
                    }
                    return Ok(count);
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) =>
                {
                    continue
                }
                Err(e) => return Err(e),
            }
        }
    }
}
