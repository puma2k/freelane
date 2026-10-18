//! Own one encoder, never the application being captured. No backend opts in by default.
use freelane_agent_protocol::{
    video::Binding,
    video_control::{Operation, Parameters, Reply},
    WindowTarget,
};
use std::io;
pub trait Encoder {
    fn available(&self) -> bool;
    fn authorize(&mut self, target: &WindowTarget) -> io::Result<()>;
    /// May fail after partial setup: Session always attempts owned cleanup on failure.
    fn start(&mut self, parameters: &Parameters) -> io::Result<()>;
    fn running(&mut self) -> io::Result<bool>;
    fn stop(&mut self) -> io::Result<()>;
    fn keyframe(&mut self) -> io::Result<()>;
}
struct Active {
    parameters: Parameters,
    confirmed: bool,
}
pub struct Session<E: Encoder> {
    backend: E,
    active: Option<Active>,
    retired: Option<Binding>,
}
impl<E: Encoder> Session<E> {
    pub fn new(backend: E) -> Self {
        Self {
            backend,
            active: None,
            retired: None,
        }
    }
    pub fn available(&self) -> bool {
        self.backend.available()
    }
    fn stop_owned(&mut self) -> io::Result<()> {
        if let Some(active) = self.active.as_mut() {
            active.confirmed = false; // A failed stop must never become an idempotent Started reply.
            self.backend.stop()?;
            self.retired = Some(active.parameters.binding);
            self.active = None;
        }
        Ok(())
    }
    fn check_owned(&mut self) -> io::Result<()> {
        let active = self.active.as_ref().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotConnected, "Video session is not active")
        })?;
        if !active.confirmed {
            return Err(io::Error::other(
                "Video setup/stop is uncertain; stop this session before retrying",
            ));
        }
        let target = active.parameters.target.clone();
        let check = self.backend.running().and_then(|running| {
            if running {
                self.backend.authorize(&target)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::NotConnected,
                    "Video encoder ended; start a fresh session",
                ))
            }
        });
        if let Err(error) = check {
            let _ = self.stop_owned();
            return Err(error);
        }
        Ok(())
    }
    pub fn action(&mut self, operation: &Operation) -> io::Result<Reply> {
        operation.validate()?;
        match operation {
            Operation::Start { parameters } => {
                // A terminated worker is no longer a competing live owner. Never
                // replay its old Start: stop_owned retires that nonce/generation.
                if self.active.as_ref().is_some_and(|active| active.confirmed) {
                    match self.backend.running() {
                        Ok(false) => self.stop_owned()?,
                        Err(error) => {
                            let _ = self.stop_owned();
                            return Err(error);
                        }
                        Ok(true) => {}
                    }
                }
                if let Some(active) = &self.active {
                    if &active.parameters != parameters {
                        return Err(io::Error::new(
                            io::ErrorKind::WouldBlock,
                            "Another video session owns the encoder",
                        ));
                    }
                    self.check_owned()?;
                    return Ok(Reply::Started {
                        parameters: parameters.clone(),
                    });
                }
                if self.retired.is_some_and(|old| {
                    old.nonce == parameters.binding.nonce
                        || (old.stream == parameters.binding.stream
                            && parameters.binding.generation <= old.generation)
                }) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Video restart requires a fresh nonce and generation",
                    ));
                }
                if !self.backend.available() {
                    return Err(io::ErrorKind::Unsupported.into());
                }
                self.backend.authorize(&parameters.target)?;
                self.active = Some(Active {
                    parameters: parameters.clone(),
                    confirmed: false,
                });
                if let Err(error) = self.backend.start(parameters) {
                    let _ = self.stop_owned();
                    return Err(error);
                }
                self.active.as_mut().unwrap().confirmed = true;
                Ok(Reply::Started {
                    parameters: parameters.clone(),
                })
            }
            Operation::Stop { binding } => {
                if let Some(active) = &self.active {
                    if active.parameters.binding != *binding {
                        return Err(io::ErrorKind::PermissionDenied.into());
                    }
                    self.stop_owned()?;
                } else if self.retired != Some(*binding) {
                    return Err(io::ErrorKind::NotFound.into());
                }
                Ok(Reply::Stopped { binding: *binding })
            }
            Operation::Keyframe { binding } => {
                if self
                    .active
                    .as_ref()
                    .is_none_or(|active| active.parameters.binding != *binding)
                {
                    return Err(io::ErrorKind::PermissionDenied.into());
                }
                self.check_owned()?;
                self.backend.keyframe()?;
                Ok(Reply::KeyframeRequested { binding: *binding })
            }
        }
    }
}
impl<E: Encoder> Drop for Session<E> {
    fn drop(&mut self) {
        let _ = self.stop_owned();
    }
}
