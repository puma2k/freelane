//! Portable VM configuration, lifecycle boundaries and an external QEMU backend.
pub mod agent;
pub mod app_launcher;
pub mod benchmark;
pub mod checkpoint;
pub mod diagnostics;
pub mod file_receive;
pub mod file_transfer;
pub mod library;
pub mod mobile;
pub mod provision;
pub mod qemu;
pub mod qmp;
pub mod resource_presets;
mod socket_wait;
pub mod telemetry;
pub mod video;

use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub name: String,
    #[serde(default)]
    pub guest_os: GuestOs,
    pub cpus: u16,
    pub memory_mib: u64,
    pub disk: PathBuf,
    pub installer: Option<PathBuf>,
    pub runtime: Option<qemu::RuntimeConfig>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GuestOs {
    #[default]
    Windows,
    Ubuntu,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

impl Config {
    pub fn parse(text: &str) -> Result<Self> {
        let config: Self = toml::from_str(text).map_err(|e| Error(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<()> {
        if let Some(runtime) = &self.runtime {
            if runtime.graphics == qemu::Graphics::Virtio2d && self.guest_os != GuestOs::Ubuntu {
                return Err(Error("Virtio 2D graphics requires Ubuntu; a supported Windows ARM GPU driver is not available in FreeLane".into()));
            }
            if runtime
                .shared_directory
                .as_ref()
                .is_some_and(|path| path.as_os_str().is_empty())
            {
                return Err(Error(
                    "Shared directory must not be empty; omit it to disable sharing".into(),
                ));
            }
            if runtime.shared_directory.is_some()
                && matches!(
                    runtime.share_mode,
                    qemu::ShareMode::Virtio9p | qemu::ShareMode::Virtio9pReadonly
                )
                && self.guest_os != GuestOs::Ubuntu
            {
                return Err(Error("Live virtio-9p sharing requires an Ubuntu guest; Windows needs a separate compatible sharing backend".into()));
            }
            let mut devices = std::collections::HashSet::new();
            if runtime.usb_devices.len() > 16
                || runtime.usb_devices.iter().any(|device| {
                    device.vendor_id == 0
                        || device.bus == 0
                        || !(1..=127).contains(&device.address)
                        || !devices.insert((device.bus, device.address))
                })
            {
                return Err(Error(
                    "Choose up to 16 distinct USB devices with bus 1–255 and address 1–127 and a nonzero vendor ID".into(),
                ));
            }
        }
        if self.runtime.as_ref().is_some_and(|runtime| {
            runtime.tcg_threading.is_some() && runtime.accelerator != qemu::Accelerator::Tcg
        }) {
            return Err(Error("tcg_threading requires accelerator='tcg'".into()));
        }
        if self.name.trim().is_empty()
            || self.name.len() > 128
            || self.name.contains(',')
            || self.name.chars().any(char::is_control)
        {
            return Err(Error(
                "VM name must be 1–128 bytes without commas or control characters".into(),
            ));
        }
        if !(1..=64).contains(&self.cpus) {
            return Err(Error("cpus must be between 1 and 64".into()));
        }
        if !(2048..=1_048_576).contains(&self.memory_mib) {
            return Err(Error("memory_mib must be between 2048 and 1048576".into()));
        }
        if self.disk.as_os_str().is_empty() {
            return Err(Error("disk path must not be empty".into()));
        }
        if self
            .installer
            .as_ref()
            .is_some_and(|p| p.as_os_str().is_empty())
        {
            return Err(Error("installer path must not be empty".into()));
        }
        Ok(())
    }
    /// Validate existing, readable images without modifying them. Relative paths use the config directory.
    pub fn check_images(&self, base: &std::path::Path) -> Result<()> {
        for image in std::iter::once(&self.disk).chain(self.installer.iter()) {
            let path = base.join(image);
            let initial =
                std::fs::metadata(&path).map_err(|e| Error(format!("{}: {e}", path.display())))?;
            if !initial.is_file() || initial.len() == 0 {
                return Err(Error(format!(
                    "{} must be a nonempty regular image file",
                    path.display()
                )));
            }
            let file = std::fs::File::open(&path)
                .map_err(|e| Error(format!("{}: {e}", path.display())))?;
            let metadata = file.metadata().map_err(|e| Error(e.to_string()))?;
            if !metadata.is_file() || metadata.len() == 0 {
                return Err(Error(format!(
                    "{} must be a nonempty regular image file",
                    path.display()
                )));
            }
        }
        Ok(())
    }
}

pub fn supported_host() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}
pub fn require_supported_host() -> Result<()> {
    if supported_host() {
        Ok(())
    } else {
        Err(Error(
            "FreeLane requires macOS on Apple Silicon; this host can run portable tests only"
                .into(),
        ))
    }
}

/// Backend implementations own all guest resources. A failed operation must leave state unchanged.
pub trait Backend {
    fn prepare(&mut self, config: &Config) -> Result<()>;
    fn start(&mut self) -> Result<()>;
    fn pause(&mut self) -> Result<()>;
    fn resume(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Created,
    Ready,
    Running,
    Paused,
    Stopped,
}
pub struct Vm<B: Backend> {
    backend: B,
    state: State,
}
impl<B: Backend> Vm<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            state: State::Created,
        }
    }
    pub fn state(&self) -> State {
        self.state
    }
    fn expect(&self, expected: State) -> Result<()> {
        if self.state == expected {
            Ok(())
        } else {
            Err(Error(format!(
                "expected {expected:?}, found {:?}",
                self.state
            )))
        }
    }
    pub fn prepare(&mut self, config: &Config) -> Result<()> {
        self.expect(State::Created)?;
        config.validate()?;
        self.backend.prepare(config)?;
        self.state = State::Ready;
        Ok(())
    }
    pub fn start(&mut self) -> Result<()> {
        self.expect(State::Ready)?;
        self.backend.start()?;
        self.state = State::Running;
        Ok(())
    }
    pub fn pause(&mut self) -> Result<()> {
        self.expect(State::Running)?;
        self.backend.pause()?;
        self.state = State::Paused;
        Ok(())
    }
    pub fn resume(&mut self) -> Result<()> {
        self.expect(State::Paused)?;
        self.backend.resume()?;
        self.state = State::Running;
        Ok(())
    }
    pub fn stop(&mut self) -> Result<()> {
        if !matches!(self.state, State::Ready | State::Running | State::Paused) {
            return Err(Error("VM has no active resources to stop".into()));
        }
        self.backend.stop()?;
        self.state = State::Stopped;
        Ok(())
    }
}

/// Probe Apple's hypervisor by creating and destroying an empty VM. Does not execute a guest.
pub fn probe_hypervisor() -> Result<()> {
    require_supported_host()?;
    native_probe()
}
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn native_probe() -> Result<()> {
    unreachable!("host checked before native probe")
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn native_probe() -> Result<()> {
    #[link(name = "Hypervisor", kind = "framework")]
    extern "C" {
        fn hv_vm_create(config: *mut std::ffi::c_void) -> i32;
        fn hv_vm_destroy() -> i32;
    }
    // SAFETY: null requests Apple's default configuration; no guest resources escape this probe.
    let status = unsafe { hv_vm_create(std::ptr::null_mut()) };
    if status != 0 {
        return Err(Error(format!(
            "hv_vm_create failed: {:#x}; verify hypervisor entitlement and host support",
            status as u32
        )));
    }
    // SAFETY: this process successfully created the VM above; no vCPUs or mappings were added.
    let status = unsafe { hv_vm_destroy() };
    if status != 0 {
        return Err(Error(format!("hv_vm_destroy failed: {:#x}", status as u32)));
    }
    Ok(())
}

pub mod storage;

pub mod media;

pub mod readiness;

pub mod settings_bundle;
pub mod sharing;

pub mod cloud_image;

pub mod agent_capabilities;

pub mod host_tools;

/// Fresh clipboard origin identifier; not an authentication credential.
pub fn clipboard_source() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error("Cannot initialize clipboard source".into()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
