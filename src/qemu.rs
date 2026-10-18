//! External QEMU backend. No firmware, OS images or drivers are bundled.
use crate::{qmp::Qmp, Backend, Config, Error, GuestOs, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Accelerator {
    #[default]
    Hvf,
    Tcg,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TcgThreading {
    Single,
    Multi,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum DiskFormat {
    Raw,
    #[default]
    Qcow2,
    Vhdx,
}
impl DiskFormat {
    fn name(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Qcow2 => "qcow2",
            Self::Vhdx => "vhdx",
        }
    }
}
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Graphics {
    #[default]
    Ramfb,
    /// Virtio GPU scanout/2D for Linux guests; no GL/Metal/3D claim.
    Virtio2d,
}
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskDevice {
    #[default]
    Nvme,
    Virtio,
}
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkDevice {
    #[default]
    Virtio,
    Usb,
}
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShareMode {
    #[default]
    ReadonlyUsb,
    /// Live read/write virtio-9p mount; requires a Linux guest driver.
    Virtio9p,
    /// Live read-only mount, enforced by the QEMU backend.
    Virtio9pReadonly,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UsbHostDevice {
    pub bus: u8,
    pub address: u8,
    pub vendor_id: u16,
    pub product_id: u16,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    pub firmware: PathBuf,
    pub nvram: PathBuf,
    #[serde(default)]
    pub disk_format: DiskFormat,
    #[serde(default)]
    pub disk_device: DiskDevice,
    #[serde(default)]
    pub accelerator: Accelerator,
    /// Optional software-emulation diagnostic override; omitted uses QEMU's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tcg_threading: Option<TcgThreading>,
    /// Binary path or executable name; no shell parsing occurs.
    #[serde(default = "default_binary")]
    pub binary: PathBuf,
    /// Use a local Cocoa/GTK display, or disable display for automated tests.
    #[serde(default)]
    pub headless: bool,
    #[serde(default)]
    pub graphics: Graphics,
    /// Opt-in private local UART socket for interactive headless guest diagnostics.
    #[serde(default)]
    pub serial_console: bool,
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub network_device: NetworkDevice,
    #[serde(default)]
    pub guest_agent: bool,
    /// Optional read-only host folder exposed as a virtual FAT USB disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_directory: Option<PathBuf>,
    #[serde(default)]
    pub share_mode: ShareMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub usb_devices: Vec<UsbHostDevice>,
    pub tpm: Option<TpmConfig>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TpmConfig {
    pub state: PathBuf,
    #[serde(default = "default_swtpm")]
    pub binary: PathBuf,
}
fn default_swtpm() -> PathBuf {
    crate::host_tools::binary("swtpm")
}
fn default_binary() -> PathBuf {
    crate::host_tools::binary("qemu-system-aarch64")
}
fn canonical(base: &Path, path: &Path) -> Result<PathBuf> {
    fs::canonicalize(base.join(path))
        .map_err(|e| Error(format!("{}: {e}", base.join(path).display())))
}
fn text_path(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| Error("QEMU paths must be valid UTF-8".into()))
}
fn file_block(node: &str, format: &str, path: &Path, readonly: bool) -> Result<String> {
    Ok(json!({"driver":format,"node-name":node,"read-only":readonly,"file":{"driver":"file","filename":text_path(path)?}}).to_string())
}

/// Runtime files are keyed by canonical configuration path, rather than user-controlled VM name.
pub fn runtime_dir(config: &Path) -> Result<PathBuf> {
    let path = fs::canonicalize(config).map_err(|e| Error(e.to_string()))?;
    let mut hash = 0xcbf29ce484222325u64;
    use std::os::unix::ffi::OsStrExt;
    for byte in path.as_os_str().as_bytes() {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
    // Short socket path avoids Unix sockaddr limits. Runtime state is ephemeral.
    Ok(std::env::temp_dir().join(format!("freelane-{:016x}", hash)))
}

fn resolve_shared_directory(
    base: &Path,
    selected: &Path,
    validate: impl Fn(&Path) -> Result<()>,
) -> Result<PathBuf> {
    let absolute = if selected.is_absolute() {
        selected.to_path_buf()
    } else if base.is_absolute() {
        base.join(selected)
    } else {
        std::env::current_dir()
            .map_err(|e| Error(e.to_string()))?
            .join(base)
            .join(selected)
    };
    validate(&absolute)?;
    let directory = canonical(base, selected)?;
    validate(&directory)?;
    Ok(directory)
}
pub fn arguments(config: &Config, base: &Path, socket: &Path) -> Result<Vec<String>> {
    arguments_inner(config, base, socket, false)
}
pub(crate) fn creation_arguments(
    config: &Config,
    base: &Path,
    socket: &Path,
) -> Result<Vec<String>> {
    arguments_inner(config, base, socket, true)
}
fn arguments_inner(
    config: &Config,
    base: &Path,
    socket: &Path,
    preparing: bool,
) -> Result<Vec<String>> {
    if !preparing
        && base
            .join(crate::provision::MARKER)
            .symlink_metadata()
            .is_ok()
    {
        return Err(Error(
            "VM creation is incomplete; use creation-recover for a prepared workspace, or retry in a new directory"
                .into(),
        ));
    }
    config.validate()?;
    if base.join(crate::checkpoint::MARKER).exists() {
        return Err(Error(
            "Checkpoint workspace is immutable; restore/import it before starting a guest".into(),
        ));
    }
    config.check_images(base)?;
    let runtime = config.runtime.as_ref().ok_or_else(|| {
        Error("Missing [runtime] settings (firmware, nvram and disk format)".into())
    })?;
    if runtime.accelerator == Accelerator::Hvf {
        crate::require_supported_host()?;
    }
    let firmware = canonical(base, &runtime.firmware)?;
    let nvram = canonical(base, &runtime.nvram)?;
    let disk = canonical(base, &config.disk)?;
    if firmware == nvram || firmware == disk || nvram == disk {
        return Err(Error(
            "Firmware, NVRAM and disk must be distinct files".into(),
        ));
    }
    for image in [&firmware, &nvram] {
        let metadata = fs::metadata(image).map_err(|e| Error(e.to_string()))?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(Error(format!(
                "{} must be a nonempty firmware file",
                image.display()
            )));
        }
    }
    // Canonical paths alone do not detect hard links to shared writable images.
    use std::os::unix::fs::MetadataExt;
    let mut identities = std::collections::HashSet::new();
    for image in [&firmware, &nvram, &disk]
        .into_iter()
        .chain(config.installer.iter())
    {
        let image = canonical(base, image)?;
        let metadata = fs::metadata(&image).map_err(|e| Error(e.to_string()))?;
        if !identities.insert((metadata.dev(), metadata.ino())) {
            return Err(Error(
                "Images must not alias the same underlying file".into(),
            ));
        }
    }
    // Prove writable files can be opened without truncating them; QEMU takes image locks.
    for image in [&nvram, &disk] {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(image)
            .map_err(|e| Error(format!("{} is not writable: {e}", image.display())))?;
    }
    let socket = text_path(socket)?;
    if socket.contains(',') || socket.len() > 100 {
        return Err(Error("Invalid QMP socket path".into()));
    }
    let (accel, cpu) = match runtime.accelerator {
        Accelerator::Hvf => ("hvf", "host"),
        Accelerator::Tcg => (
            match runtime.tcg_threading {
                None => "tcg",
                Some(TcgThreading::Single) => "tcg,thread=single",
                Some(TcgThreading::Multi) => "tcg,thread=multi",
            },
            "max",
        ),
    };
    let mut args: Vec<String> = [
        "-name",
        &config.name,
        "-machine",
        "virt-10.0,gic-version=3,pflash0=firmware,pflash1=nvram",
        "-accel",
        accel,
        "-cpu",
        cpu,
        "-smp",
        &config.cpus.to_string(),
        "-m",
        &config.memory_mib.to_string(),
        "-S",
        "-qmp",
        &format!("unix:{socket},server=on,wait=off"),
        "-monitor",
        "none",
        "-display",
        if runtime.headless {
            "none"
        } else if cfg!(target_os = "macos") {
            "cocoa"
        } else {
            "gtk"
        },
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    let directory = Path::new(socket).parent().unwrap();
    if runtime.serial_console {
        args.extend([
            "-chardev".into(),
            format!(
                "socket,id=serialconsole,path={},server=on,wait=off,logfile={}",
                directory.join("console.sock").display(),
                directory.join("serial.log").display()
            ),
            "-serial".into(),
            "chardev:serialconsole".into(),
        ]);
    } else {
        args.extend([
            "-serial".into(),
            format!("file:{}", directory.join("serial.log").display()),
        ]);
    }
    for (node, format, path, readonly) in [
        ("firmware", "raw", &firmware, true),
        ("nvram", "raw", &nvram, false),
        ("disk", runtime.disk_format.name(), &disk, false),
    ] {
        args.extend([
            "-blockdev".into(),
            file_block(node, format, path, readonly)?,
        ]);
    }
    if runtime.graphics == Graphics::Virtio2d {
        args.extend([
            "-device".into(),
            "virtio-gpu-pci,id=freelane-gpu,disable-legacy=on".into(),
        ]);
    }
    args.extend(
        [
            "-device",
            match runtime.disk_device {
                DiskDevice::Nvme => "nvme,drive=disk,serial=freelane-disk",
                DiskDevice::Virtio => {
                    "virtio-blk-pci,disable-legacy=on,drive=disk,serial=freelane-disk,bootindex=1"
                }
            },
            "-device",
            "ramfb",
            "-device",
            "qemu-xhci",
            "-device",
            "usb-kbd",
            "-device",
            "usb-tablet",
        ]
        .into_iter()
        .map(str::to_string),
    );
    if let Some(installer) = &config.installer {
        let installer = canonical(base, installer)?;
        if installer == disk || installer == nvram || installer == firmware {
            return Err(Error(
                "Installer must be distinct from writable images and firmware".into(),
            ));
        }
        args.extend([
            "-blockdev".into(),
            file_block("installer", "raw", &installer, true)?,
            "-device".into(),
            "usb-bot,id=installusb".into(),
            "-device".into(),
            "scsi-cd,bus=installusb.0,drive=installer,bootindex=0".into(),
        ]);
    }
    if let Some(directory) = &runtime.shared_directory {
        let directory =
            resolve_shared_directory(base, directory, crate::storage::check_share_volume)?;
        if !directory.is_dir() {
            return Err(Error("Shared directory must be an existing folder".into()));
        }
        match runtime.share_mode {
            ShareMode::ReadonlyUsb => args.extend([
                "-blockdev".into(),
                json!({"driver":"vvfat", "node-name":"shared", "dir":text_path(&directory)?,
                    "fat-type":16, "rw":false, "read-only":true})
                .to_string(),
                "-device".into(),
                "usb-storage,drive=shared,removable=on".into(),
            ]),
            ShareMode::Virtio9p | ShareMode::Virtio9pReadonly => {
                // Escape QEMU key/value separators, rather than treating a path as options.
                let directory = text_path(&directory)?.replace(',', ",,");
                if directory.chars().any(char::is_control) {
                    return Err(Error(
                        "Shared path must not contain control characters".into(),
                    ));
                }
                args.extend([
                    "-fsdev".into(),
                    format!(
                        "local,id=hostshare,path={directory},security_model=mapped-xattr{}",
                        if runtime.share_mode == ShareMode::Virtio9pReadonly {
                            ",readonly=on"
                        } else {
                            ""
                        }
                    ),
                    "-device".into(),
                    "virtio-9p-pci,disable-legacy=on,fsdev=hostshare,mount_tag=freelane".into(),
                ]);
            }
        }
    }
    for device in &runtime.usb_devices {
        args.extend([
            "-device".into(),
            format!(
                "usb-host,hostbus={},hostaddr={},vendorid=0x{:04x},productid=0x{:04x}",
                device.bus, device.address, device.vendor_id, device.product_id
            ),
        ]);
    }

    if runtime.guest_agent {
        let agent_socket = Path::new(socket).parent().unwrap().join("agent.sock");
        args.extend([
            "-chardev".into(),
            format!(
                "socket,id=guestagent,path={},server=on,wait=off",
                agent_socket.display()
            ),
            "-device".into(),
            "virtio-serial-pci,disable-legacy=on".into(),
            "-device".into(),
            "virtserialport,chardev=guestagent,name=org.freelane.agent.0".into(),
            "-chardev".into(),
            format!(
                "socket,id=guestvideo,path={},server=on,wait=off",
                text_path(&Path::new(socket).parent().unwrap().join("video.sock"))?
                    .replace(',', ",,")
            ),
            "-device".into(),
            "virtserialport,chardev=guestvideo,name=org.freelane.video.0".into(),
        ]);
    }
    if runtime.network {
        args.extend(
            [
                "-netdev",
                "user,id=net0",
                "-device",
                match runtime.network_device {
                    NetworkDevice::Virtio => {
                        "virtio-net-pci,disable-legacy=on,netdev=net0,romfile="
                    }
                    NetworkDevice::Usb => "usb-net,netdev=net0",
                },
            ]
            .into_iter()
            .map(str::to_string),
        );
    } else {
        args.extend(["-nic".into(), "none".into()]);
    }
    if runtime.tpm.is_some() {
        let tpm_socket = socket
            .strip_suffix("qmp.sock")
            .ok_or_else(|| Error("Unexpected QMP socket filename".into()))?
            .to_owned()
            + "tpm.sock";
        args.extend([
            "-chardev".into(),
            format!("socket,id=tpmchar,path={tpm_socket}"),
            "-tpmdev".into(),
            "emulator,id=tpm0,chardev=tpmchar".into(),
            "-device".into(),
            "tpm-tis-device,tpmdev=tpm0".into(),
        ]);
    }
    Ok(args)
}

/// Save settings only while the VM is stopped, preserving a private backup.
/// The original text prevents overwriting edits made after the GUI loaded it.
pub fn save_config(path: &Path, original: &str, config: &Config) -> Result<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    let path = fs::canonicalize(path).map_err(|e| Error(e.to_string()))?;
    let base = path
        .parent()
        .ok_or_else(|| Error("Missing config directory".into()))?;
    config.validate()?;
    config.check_images(base)?;
    // Validate the full device plan without requiring this editor's host to support HVF.
    let mut checked = config.clone();
    if let Some(runtime) = checked.runtime.as_mut() {
        runtime.accelerator = Accelerator::Tcg;
    }
    let directory = runtime_dir(&path)?;
    match fs::create_dir(&directory) {
        Ok(()) => fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .map_err(|e| Error(e.to_string()))?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(Error(e.to_string())),
    }
    let metadata = fs::symlink_metadata(&directory).map_err(|e| Error(e.to_string()))?;
    if !metadata.is_dir()
        || metadata.mode() & 0o077 != 0
        || metadata.uid() != fs::metadata(base).map_err(|e| Error(e.to_string()))?.uid()
    {
        return Err(Error(
            "VM runtime directory must be private and owned by VM owner".into(),
        ));
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(directory.join("lock"))
        .map_err(|e| Error(e.to_string()))?;
    lock.try_lock_exclusive()
        .map_err(|_| Error("Shut down the VM before saving settings".into()))?;
    arguments(&checked, base, &directory.join("qmp.sock"))?;
    let current = fs::read_to_string(&path).map_err(|e| Error(e.to_string()))?;
    if current != original {
        return Err(Error(
            "Configuration changed on disk. Reopen it before saving".into(),
        ));
    }
    let serialized = toml::to_string_pretty(config).map_err(|e| Error(e.to_string()))?;
    let mut nonce = [0u8; 8];
    getrandom::fill(&mut nonce).map_err(|e| Error(e.to_string()))?;
    let suffix = u64::from_ne_bytes(nonce);
    let backup = path.with_extension(format!("toml.backup-{suffix:016x}"));
    let temporary = path.with_extension(format!("toml.pending-{suffix:016x}"));
    let write_private = |destination: &Path, contents: &str| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(destination)
            .map_err(|e| Error(e.to_string()))?;
        file.write_all(contents.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|e| Error(e.to_string()))
    };
    write_private(&backup, &current)?;
    let result = write_private(&temporary, &serialized)
        .and_then(|()| fs::rename(&temporary, &path).map_err(|e| Error(e.to_string())));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    File::open(base)
        .and_then(|file| file.sync_all())
        .map_err(|e| Error(e.to_string()))?;
    Ok(backup)
}

pub struct QemuBackend {
    base: PathBuf,
    directory: PathBuf,
    child: Option<Child>,
    lock: Option<File>,
    tpm_child: Option<Child>,
    tpm_lock: Option<File>,
}
impl QemuBackend {
    pub fn new(config_path: &Path) -> Result<Self> {
        let config_path = fs::canonicalize(config_path).map_err(|e| Error(e.to_string()))?;
        if config_path
            .parent()
            .unwrap()
            .join(crate::provision::MARKER)
            .symlink_metadata()
            .is_ok()
        {
            return Err(Error(
                "VM creation is incomplete; use creation-recover for a prepared workspace, or retry in a new directory"
                    .into(),
            ));
        }
        Ok(Self {
            base: config_path.parent().unwrap().into(),
            directory: runtime_dir(&config_path)?,
            child: None,
            lock: None,
            tpm_child: None,
            tpm_lock: None,
        })
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn poll(&mut self) -> Result<Option<ExitStatus>> {
        self.child
            .as_mut()
            .ok_or_else(|| Error("QEMU has not started".into()))?
            .try_wait()
            .map_err(|e| Error(e.to_string()))
    }
    fn command(&mut self, command: &str) -> Result<()> {
        Qmp::connect(&self.directory.join("qmp.sock"))?.execute(command, None)?;
        Ok(())
    }
    fn cleanup(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(mut child) = self.tpm_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.tpm_lock.take();
        // Hold the lock through socket cleanup, including failure paths.
        if self.lock.is_some() {
            let _ = fs::remove_file(self.directory.join("qmp.sock"));
            let _ = fs::remove_file(self.directory.join("agent.sock"));
            let _ = fs::remove_file(self.directory.join("video.sock"));
            let _ = fs::remove_file(self.directory.join("console.sock"));
            let _ = fs::remove_file(self.directory.join("tpm.sock"));
        }
        self.lock.take();
    }
    fn start_tpm(&mut self, config: &TpmConfig) -> Result<()> {
        let state = canonical(&self.base, &config.state)?;
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::metadata(&state).map_err(|e| Error(e.to_string()))?;
        if !metadata.is_dir()
            || metadata.mode() & 0o077 != 0
            || metadata.uid()
                != fs::metadata(&self.base)
                    .map_err(|e| Error(e.to_string()))?
                    .uid()
        {
            return Err(Error(
                "TPM state must be a private directory owned by VM directory owner".into(),
            ));
        }
        let state_text = text_path(&state)?;
        if state_text.contains(',') {
            return Err(Error("TPM state directory cannot contain commas".into()));
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(state.join(".freelane.lock"))
            .map_err(|e| Error(e.to_string()))?;
        lock.try_lock_exclusive()
            .map_err(|_| Error("TPM state is in use by another VM".into()))?;
        self.tpm_lock = Some(lock);
        let socket = self.directory.join("tpm.sock");
        let _ = fs::remove_file(&socket);
        let log =
            File::create(self.directory.join("swtpm.log")).map_err(|e| Error(e.to_string()))?;
        let binary = if config.binary.components().count() > 1 {
            self.base.join(&config.binary)
        } else {
            config.binary.clone()
        };
        self.tpm_child = Some(
            Command::new(binary)
                .args([
                    "socket",
                    "--tpm2",
                    "--tpmstate",
                    &format!("dir={state_text},mode=0600"),
                    "--ctrl",
                    &format!("type=unixio,path={}", socket.display()),
                    "--flags",
                    "not-need-init",
                    "--terminate",
                ])
                .stdin(Stdio::null())
                .stdout(log.try_clone().map_err(|e| Error(e.to_string()))?)
                .stderr(log)
                .spawn()
                .map_err(|e| Error(format!("swtpm startup: {e}")))?,
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while !socket.exists() {
            if let Some(status) = self
                .tpm_child
                .as_mut()
                .unwrap()
                .try_wait()
                .map_err(|e| Error(e.to_string()))?
            {
                return Err(Error(format!(
                    "swtpm exited with {status}; see {}/swtpm.log",
                    self.directory.display()
                )));
            }
            if Instant::now() >= deadline {
                return Err(Error("Timed out starting software TPM".into()));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }
    fn prepare_inner(&mut self, config: &Config) -> Result<()> {
        use std::os::unix::fs::OpenOptionsExt;
        if self.child.is_some() {
            return Err(Error("QEMU already prepared".into()));
        }
        let args = arguments(config, &self.base, &self.directory.join("qmp.sock"))?;
        match fs::create_dir(&self.directory) {
            Ok(()) => {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&self.directory, fs::Permissions::from_mode(0o700))
                    .map_err(|e| Error(e.to_string()))?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(Error(e.to_string())),
        }
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::symlink_metadata(&self.directory).map_err(|e| Error(e.to_string()))?;
        let own_uid = fs::metadata(&self.base)
            .map_err(|e| Error(e.to_string()))?
            .uid();
        if !metadata.is_dir() || metadata.uid() != own_uid || metadata.mode() & 0o077 != 0 {
            return Err(Error(
                "Runtime directory must be a private directory owned by checkout owner".into(),
            ));
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(self.directory.join("lock"))
            .map_err(|e| Error(e.to_string()))?;
        lock.try_lock_exclusive()
            .map_err(|_| Error("This VM is already running".into()))?;
        self.lock = Some(lock);
        let _ = fs::remove_file(self.directory.join("qmp.sock"));
        let _ = fs::remove_file(self.directory.join("console.sock"));
        let log =
            File::create(self.directory.join("qemu.log")).map_err(|e| Error(e.to_string()))?;
        if let Some(tpm) = &config.runtime.as_ref().unwrap().tpm {
            self.start_tpm(tpm)?;
        }
        let binary = &config.runtime.as_ref().unwrap().binary;
        let binary = if binary.components().count() > 1 {
            self.base.join(binary)
        } else {
            binary.clone()
        };
        self.child = Some(
            Command::new(binary)
                .args(args)
                .stdin(Stdio::null())
                .stdout(log.try_clone().map_err(|e| Error(e.to_string()))?)
                .stderr(log)
                .spawn()
                .map_err(|e| Error(format!("Could not start QEMU: {e}")))?,
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(status) = self.poll()? {
                return Err(Error(format!(
                    "QEMU exited during startup ({status}); see {}",
                    self.directory.join("qemu.log").display()
                )));
            }
            if self.directory.join("qmp.sock").exists() {
                let mut qmp = Qmp::connect(&self.directory.join("qmp.sock"))?;
                let status = qmp.execute("query-status", None)?;
                if status["running"] != false {
                    return Err(Error("QEMU was not started paused".into()));
                }
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Error("Timed out waiting for QEMU monitor".into()));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
impl Backend for QemuBackend {
    fn prepare(&mut self, config: &Config) -> Result<()> {
        if self.child.is_some() {
            return Err(Error("QEMU already prepared".into()));
        }
        let result = self.prepare_inner(config);
        if result.is_err() {
            self.cleanup();
        }
        result
    }
    fn start(&mut self) -> Result<()> {
        self.command("cont")
    }
    fn pause(&mut self) -> Result<()> {
        self.command("stop")
    }
    fn resume(&mut self) -> Result<()> {
        self.command("cont")
    }
    fn stop(&mut self) -> Result<()> {
        self.command("quit")?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.poll()?.is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        self.cleanup();
        Ok(())
    }
}
impl Drop for QemuBackend {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Send a chord using QEMU qcodes, without interpreting a monitor command string.
pub fn send_keys(config: &Path, keys: &[String]) -> Result<()> {
    if keys.is_empty()
        || keys.len() > 8
        || keys.iter().any(|key| {
            key.is_empty()
                || key.len() > 32
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
    {
        return Err(Error(
            "Provide 1–8 QEMU qcodes (lowercase letters, digits, underscores)".into(),
        ));
    }
    let mut qmp = Qmp::connect(&runtime_dir(config)?.join("qmp.sock"))?;
    if qmp.execute("query-status", None)?["running"] != true {
        return Err(Error("Guest must be running to send keys".into()));
    }
    let keys: Vec<_> = keys
        .iter()
        .map(|key| json!({"type":"qcode", "data":key}))
        .collect();
    qmp.execute("send-key", Some(json!({"keys":keys,"hold-time":30})))?;
    // Allow QEMU's release timer to fire before a caller sends a repeated key.
    // This is not an acknowledgment of the guest's UI action.
    std::thread::sleep(Duration::from_millis(50));
    Ok(())
}

/// Click the USB tablet using QEMU's normalized absolute coordinate range.
pub fn click(config: &Path, x: u16, y: u16) -> Result<()> {
    if x > 32767 || y > 32767 {
        return Err(Error(
            "Mouse coordinates must be between 0 and 32767".into(),
        ));
    }
    let socket = runtime_dir(config)?.join("qmp.sock");
    let mut qmp = Qmp::connect(&socket)?;
    if qmp.execute("query-status", None)?["running"] != true {
        return Err(Error("Guest must be running to send mouse input".into()));
    }
    let release = json!({"events": [
        {"type":"btn", "data":{"down":false, "button":"left"}}
    ]});
    let result: Result<()> = (|| {
        qmp.execute(
            "input-send-event",
            Some(json!({"events": [
                {"type":"abs", "data":{"axis":"x", "value":x}},
                {"type":"abs", "data":{"axis":"y", "value":y}},
                {"type":"btn", "data":{"down":true, "button":"left"}}
            ]})),
        )?;
        // Keep press/release in separate USB reports so the guest observes the click.
        std::thread::sleep(Duration::from_millis(100));
        qmp.execute("input-send-event", Some(release.clone()))?;
        Ok(())
    })();
    if result.is_err() {
        // A lost acknowledgment leaves delivery uncertain. Release only:
        // never replay the press or claim that the click succeeded.
        if let Ok(mut recovery) = Qmp::connect(&socket) {
            let _ = recovery.execute("input-send-event", Some(release));
        }
    }
    result.map_err(|error| Error(format!("Mouse input failed: {error}. Delivery may be unknown; a button release was attempted. Check the guest before repeating the click.")))
}

/// Capture a PPM display frame. Never overwrite a user's existing output file.
pub fn screenshot(config: &Path, output: &Path) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    if output.extension().and_then(|s| s.to_str()) != Some("ppm") {
        return Err(Error("Screenshot output must have a .ppm extension".into()));
    }
    let output = std::env::current_dir()
        .map_err(|e| Error(e.to_string()))?
        .join(output);
    let filename = text_path(&output)?;
    let mut qmp = Qmp::connect(&runtime_dir(config)?.join("qmp.sock"))?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&output)
        .map_err(|e| Error(format!("Screenshot {}: {e}", output.display())))?;
    drop(file);
    let result = qmp.execute(
        "screendump",
        Some(json!({"filename":filename,"format":"ppm"})),
    );
    if let Err(error) = result {
        let _ = fs::remove_file(&output);
        return Err(error);
    }
    Ok(())
}

pub fn control(config: &Path, action: &str) -> Result<serde_json::Value> {
    let mut qmp = Qmp::connect(&runtime_dir(config)?.join("qmp.sock"))?;
    let command = match action {
        "status" => "query-status",
        "pause" => "stop",
        "resume" => "cont",
        "shutdown" => "system_powerdown",
        "stop" => "quit",
        _ => {
            return Err(Error(
                "Control action: status | pause | resume | shutdown | stop".into(),
            ))
        }
    };
    qmp.execute(command, None)
}

/// Create a fresh sparse disk and private firmware variable store. Never reuse or overwrite a directory.
pub fn initialize(
    directory: &Path,
    firmware: &Path,
    template: &Path,
    qemu_img: &Path,
) -> Result<PathBuf> {
    initialize_guest(directory, firmware, template, qemu_img, GuestOs::Windows)
}

pub fn initialize_guest(
    directory: &Path,
    firmware: &Path,
    template: &Path,
    qemu_img: &Path,
    guest_os: GuestOs,
) -> Result<PathBuf> {
    initialize_guest_sized(directory, firmware, template, qemu_img, guest_os, 64)
}

pub fn initialize_guest_sized(
    directory: &Path,
    firmware: &Path,
    template: &Path,
    qemu_img: &Path,
    guest_os: GuestOs,
    disk_gib: u64,
) -> Result<PathBuf> {
    initialize_guest_transaction(
        directory,
        firmware,
        template,
        qemu_img,
        guest_os,
        disk_gib,
        Creation {
            retain_marker: false,
            anchor: None,
        },
    )
}
pub(crate) struct Creation<'a> {
    pub retain_marker: bool,
    pub anchor: Option<&'a crate::storage::ParentAnchor>,
}
pub(crate) fn initialize_guest_transaction(
    directory: &Path,
    firmware: &Path,
    template: &Path,
    qemu_img: &Path,
    guest_os: GuestOs,
    disk_gib: u64,
    creation: Creation<'_>,
) -> Result<PathBuf> {
    let Creation {
        retain_marker,
        anchor,
    } = creation;
    let minimum = if guest_os == GuestOs::Windows { 64 } else { 8 };
    if disk_gib < minimum || disk_gib.checked_mul(1024 * 1024 * 1024).is_none() {
        return Err(Error(format!(
            "Virtual disk must be at least {minimum} GiB and fit the backend size representation"
        )));
    }
    let firmware = fs::canonicalize(firmware).map_err(|e| Error(format!("Firmware: {e}")))?;
    let template = fs::canonicalize(template).map_err(|e| Error(format!("NVRAM template: {e}")))?;
    for path in [&firmware, &template] {
        let metadata = fs::metadata(path).map_err(|e| Error(e.to_string()))?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(Error(
                "Firmware and NVRAM template must be nonempty regular files".into(),
            ));
        }
    }
    if firmware == template {
        return Err(Error(
            "Firmware and variable template must be distinct".into(),
        ));
    }
    // create_dir is the no-overwrite boundary. Failed setup leaves a clearly incomplete directory.
    // Keep the private stage directory descriptor alive through component writes.
    let _stage = match anchor {
        Some(anchor) => Some(anchor.mkdir(directory)?),
        None => {
            fs::create_dir(directory).map_err(|e| {
                Error(format!(
                    "Create new VM directory {}: {e}",
                    directory.display()
                ))
            })?;
            None
        }
    };
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .map_err(|e| Error(e.to_string()))?;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut marker = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directory.join(crate::provision::MARKER))
        .map_err(|e| Error(e.to_string()))?;
    marker
        .write_all(br#"{"version":1,"status":"creating"}"#)
        .and_then(|_| marker.sync_all())
        .map_err(|e| Error(e.to_string()))?;
    fs::copy(firmware, directory.join("firmware.fd")).map_err(|e| Error(e.to_string()))?;
    fs::copy(template, directory.join("nvram.fd")).map_err(|e| Error(e.to_string()))?;
    fs::set_permissions(
        directory.join("nvram.fd"),
        fs::Permissions::from_mode(0o600),
    )
    .map_err(|e| Error(e.to_string()))?;
    let (name, disk) = match guest_os {
        GuestOs::Windows => ("Windows 11 ARM", "windows.qcow2"),
        GuestOs::Ubuntu => ("Ubuntu ARM64", "ubuntu.qcow2"),
    };
    let status = Command::new(qemu_img)
        .args(["create", "-f", "qcow2"])
        .arg(directory.join(disk))
        .arg(format!("{disk_gib}G"))
        .stdin(Stdio::null())
        .status()
        .map_err(|e| Error(format!("qemu-img: {e}; incomplete directory retained")))?;
    if !status.success() {
        return Err(Error(format!(
            "qemu-img failed ({status}); incomplete directory retained"
        )));
    }
    fs::create_dir(directory.join("tpm")).map_err(|e| Error(e.to_string()))?;
    fs::set_permissions(directory.join("tpm"), fs::Permissions::from_mode(0o700))
        .map_err(|e| Error(e.to_string()))?;
    let config = Config {
        name: name.into(),
        guest_os,
        cpus: 4,
        memory_mib: 4096,
        disk: disk.into(),
        installer: None,
        runtime: Some(RuntimeConfig {
            firmware: "firmware.fd".into(),
            nvram: "nvram.fd".into(),
            disk_format: DiskFormat::Qcow2,
            disk_device: if guest_os == GuestOs::Ubuntu {
                DiskDevice::Virtio
            } else {
                DiskDevice::Nvme
            },
            accelerator: Accelerator::Hvf,
            tcg_threading: None,
            binary: default_binary(),
            headless: false,
            graphics: Graphics::Ramfb,
            serial_console: false,
            network: false,
            network_device: NetworkDevice::Virtio,
            guest_agent: false,
            shared_directory: None,
            share_mode: ShareMode::ReadonlyUsb,
            usb_devices: Vec::new(),
            tpm: (guest_os == GuestOs::Windows).then(|| TpmConfig {
                state: "tpm".into(),
                binary: default_swtpm(),
            }),
        }),
    };
    let path = directory.join("vm.toml");
    let serialized = toml::to_string_pretty(&config).map_err(|e| Error(e.to_string()))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| Error(e.to_string()))?;
    file.write_all(serialized.as_bytes())
        .map_err(|e| Error(e.to_string()))?;
    file.sync_all().map_err(|e| Error(e.to_string()))?;
    if !retain_marker {
        fs::remove_file(directory.join(crate::provision::MARKER))
            .map_err(|e| Error(e.to_string()))?;
    }
    Ok(path)
}

#[cfg(test)]
mod shared_source_tests {
    #[test]
    fn external_selection_symlink_cannot_bypass_volume_validation() {
        use std::{fs, os::unix::fs::symlink};
        let root =
            std::env::temp_dir().join(format!("fl-share-volume-source-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let volumes = root.join("Volumes");
        fs::create_dir(&volumes).unwrap();
        let internal = root.join("internal");
        fs::create_dir(&internal).unwrap();
        symlink(&internal, volumes.join("Disconnected")).unwrap();
        let result =
            super::resolve_shared_directory(&root, &volumes.join("Disconnected"), |path| {
                crate::storage::check_mounted_volume(path, &volumes)
            });
        fs::remove_dir_all(&root).unwrap();
        assert!(
            result.is_err(),
            "selected volume must be validated before canonicalization"
        );
    }
}
