//! Private native-helper setup and owned process lifecycle.
use crate::video::Encoder;
use freelane_agent_protocol::video_control::Parameters;
use freelane_agent_protocol::WindowTarget;
use std::io;
use std::{
    io::{Read, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Copy, PartialEq)]
enum Stage {
    AwaitReady,
    Ready,
    AwaitStarted,
    Active,
    Failed,
}
struct Owned {
    child: Child,
    input: Option<ChildStdin>,
    output: ChildStdout,
    stage: Stage,
    key_pending: bool,
}
pub struct ProcessEncoder {
    command: Command,
    authorization: fn(&WindowTarget) -> io::Result<()>,
    owned: Option<Owned>,
}
impl ProcessEncoder {
    pub fn new(command: Command, authorization: fn(&WindowTarget) -> io::Result<()>) -> Self {
        Self {
            command,
            authorization,
            owned: None,
        }
    }
    fn pump(&mut self) -> io::Result<()> {
        let Some(owned) = self.owned.as_mut() else {
            return Ok(());
        };
        for _ in 0..8 {
            if !readable(&owned.output)? {
                return Ok(());
            }
            let mut byte = [0];
            match owned.output.read(&mut byte) {
                Ok(1) => match (owned.stage, byte[0]) {
                    (Stage::AwaitReady, b'R') => owned.stage = Stage::Ready,
                    (Stage::AwaitStarted, b'S') => owned.stage = Stage::Active,
                    (Stage::Active, b'K') if owned.key_pending => owned.key_pending = false,
                    _ => {
                        owned.stage = Stage::Failed;
                        return Err(io::Error::other("Unexpected native worker status"));
                    }
                },
                Ok(_) => {
                    owned.stage = Stage::Failed;
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    return Ok(())
                }
                Err(e) => {
                    owned.stage = Stage::Failed;
                    return Err(e);
                }
            }
        }
        owned.stage = Stage::Failed;
        Err(io::Error::other("Native worker status overflow"))
    }
    fn wait_stage(&mut self, stage: Stage) -> io::Result<()> {
        let end = Instant::now() + Duration::from_secs(2);
        loop {
            self.pump()?;
            let owned = self.owned.as_mut().unwrap();
            if owned.child.try_wait()?.is_some() {
                return Err(io::ErrorKind::NotConnected.into());
            }
            if owned.stage == stage {
                return Ok(());
            }
            if Instant::now() >= end {
                owned.stage = Stage::Failed;
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Native video worker startup timed out",
                ));
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Encoder for ProcessEncoder {
    fn available(&self) -> bool {
        let path = Path::new(self.command.get_program());
        path.is_absolute()
            && path
                .symlink_metadata()
                .is_ok_and(|m| m.file_type().is_file())
    }
    fn authorize(&mut self, target: &WindowTarget) -> io::Result<()> {
        (self.authorization)(target)
    }
    fn start(&mut self, parameters: &Parameters) -> io::Result<()> {
        let bytes = setup_bytes(parameters)?;
        if self.owned.is_some() {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        self.authorize(&parameters.target)?;
        if !self.available() {
            return Err(io::ErrorKind::Unsupported.into());
        }
        self.command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.command.creation_flags(0x08000000); // CREATE_NO_WINDOW: capture target remains separate.
        }
        let mut child = self.command.spawn()?;
        let input = child.stdin.take().expect("piped stdin");
        let output = child.stdout.take().expect("piped stdout");
        self.owned = Some(Owned {
            child,
            input: Some(input),
            output,
            stage: Stage::AwaitReady,
            key_pending: false,
        });
        self.owned
            .as_mut()
            .unwrap()
            .input
            .as_mut()
            .unwrap()
            .write_all(&bytes)?;
        self.wait_stage(Stage::Ready)?;
        self.authorize(&parameters.target)?;
        let owned = self.owned.as_mut().unwrap();
        owned.stage = Stage::AwaitStarted;
        owned.input.as_mut().unwrap().write_all(b"G")?;
        self.wait_stage(Stage::Active)
    }
    fn running(&mut self) -> io::Result<bool> {
        if self
            .owned
            .as_mut()
            .is_some_and(|owned| matches!(owned.child.try_wait(), Ok(Some(_))))
        {
            return Ok(false);
        }
        if let Err(error) = self.pump() {
            if matches!(
                error.kind(),
                io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe
            ) {
                // A closed status pipe is no longer a healthy session, even if
                // the process is still finishing exit. Keep ownership until Stop
                // confirms cleanup; Session can then start a fresh binding.
                if let Some(owned) = &mut self.owned {
                    owned.stage = Stage::Failed;
                }
                return Ok(false);
            }
            return Err(error);
        }
        match self.owned.as_mut() {
            Some(owned) => Ok(owned.stage == Stage::Active && owned.child.try_wait()?.is_none()),
            None => Ok(false),
        }
    }
    fn stop(&mut self) -> io::Result<()> {
        let Some(owned) = self.owned.as_mut() else {
            return Ok(());
        };
        owned.stage = Stage::Failed;
        owned.input.take(); // EOF is a graceful request; killing is bounded fallback.
        let kill_error = if owned.child.try_wait()?.is_none() {
            owned.child.kill().err()
        } else {
            None
        };
        let end = Instant::now() + Duration::from_secs(2);
        while owned.child.try_wait()?.is_none() {
            if Instant::now() >= end {
                if let Some(error) = kill_error {
                    return Err(error);
                }
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Native worker cleanup remains uncertain",
                ));
            }
            thread::sleep(Duration::from_millis(5));
        }
        self.owned = None;
        Ok(())
    }
    fn keyframe(&mut self) -> io::Result<()> {
        if !self.running()? {
            return Err(io::ErrorKind::NotConnected.into());
        }
        let owned = self.owned.as_mut().unwrap();
        if !owned.key_pending {
            owned.key_pending = true;
            owned.input.as_mut().unwrap().write_all(b"K")?;
        }
        Ok(())
    }
}
impl Drop for ProcessEncoder {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(unix)]
fn readable(output: &ChildStdout) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    let mut fd = libc::pollfd {
        fd: output.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let result = unsafe { libc::poll(&mut fd, 1, 0) };
    if result < 0 {
        let e = io::Error::last_os_error();
        if e.kind() == io::ErrorKind::Interrupted {
            return Ok(false);
        }
        return Err(e);
    }
    Ok(result > 0)
}
#[cfg(windows)]
fn readable(output: &ChildStdout) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn PeekNamedPipe(
            pipe: *mut std::ffi::c_void,
            buffer: *mut std::ffi::c_void,
            size: u32,
            read: *mut u32,
            available: *mut u32,
            left: *mut u32,
        ) -> i32;
    }
    let mut available = 0;
    let ok = unsafe {
        PeekNamedPipe(
            output.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(available > 0)
}
pub fn setup_bytes(p: &Parameters) -> io::Result<[u8; 80]> {
    p.validate()?;
    let mut b = [0; 80];
    b[..8].copy_from_slice(b"FLW1\0P\0\0");
    b[8..16].copy_from_slice(&p.target.handle.to_be_bytes());
    b[16..20].copy_from_slice(&p.target.pid.to_be_bytes());
    b[24..32].copy_from_slice(&p.target.created.to_be_bytes());
    b[32..40].copy_from_slice(&p.target.incarnation.to_be_bytes());
    b[40..56].copy_from_slice(&p.binding.nonce);
    b[56..64].copy_from_slice(&p.binding.stream.to_be_bytes());
    b[64..72].copy_from_slice(&p.binding.generation.to_be_bytes());
    b[72..74].copy_from_slice(&p.width.to_be_bytes());
    b[74..76].copy_from_slice(&p.height.to_be_bytes());
    b[76] = p.fps;
    Ok(b)
}
