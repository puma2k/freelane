use freelane::{qemu, Config};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
#[test]
fn serial_console_is_opt_in_private_and_keeps_a_uart_log() {
    let images = Images::new();
    let mut config = images.config();
    let socket = images.0.join("qmp.sock");
    let default = qemu::arguments(&config, &images.0, &socket).unwrap();
    assert!(!default.iter().any(|arg| arg.contains("console.sock")));
    config.runtime.as_mut().unwrap().serial_console = true;
    let args = qemu::arguments(&config, &images.0, &socket).unwrap();
    assert!(args.iter().any(|arg| arg == "chardev:serialconsole"));
    assert!(args
        .iter()
        .any(|arg| arg.starts_with("socket,id=serialconsole,path=")
            && arg.contains("console.sock")
            && arg.contains("logfile=")
            && arg.contains("serial.log")));
    assert!(!args.iter().any(|arg| arg.starts_with("tcp:")));
}
#[test]
fn ubuntu_virtio_storage_is_explicit_and_windows_retains_nvme() {
    let images = Images::new();
    let mut config = images.config();
    let args = qemu::arguments(&config, &images.0, &images.0.join("qmp.sock")).unwrap();
    assert!(args
        .iter()
        .any(|s| s == "nvme,drive=disk,serial=freelane-disk"));
    config.runtime.as_mut().unwrap().disk_device = qemu::DiskDevice::Virtio;
    let parsed = Config::parse(&toml::to_string(&config).unwrap()).unwrap();
    let args = qemu::arguments(&parsed, &images.0, &images.0.join("qmp.sock")).unwrap();
    assert!(args.iter().any(
        |s| s == "virtio-blk-pci,disable-legacy=on,drive=disk,serial=freelane-disk,bootindex=1"
    ));
    assert!(!args.iter().any(|s| s.starts_with("nvme,")));
}
#[test]
fn tcg_threading_is_explicit_and_preserves_guest_resources() {
    let images = Images::new();
    let mut config = images.config();
    for (mode, expected) in [
        (None, "tcg"),
        (Some(qemu::TcgThreading::Single), "tcg,thread=single"),
        (Some(qemu::TcgThreading::Multi), "tcg,thread=multi"),
    ] {
        config.runtime.as_mut().unwrap().tcg_threading = mode;
        let roundtrip = Config::parse(&toml::to_string(&config).unwrap()).unwrap();
        assert_eq!(roundtrip.runtime.as_ref().unwrap().tcg_threading, mode);
        let args = qemu::arguments(&roundtrip, &images.0, &images.0.join("qmp.sock")).unwrap();
        for (flag, value) in [
            ("-accel", expected),
            ("-cpu", "max"),
            ("-smp", "4"),
            ("-m", "4096"),
        ] {
            assert!(args
                .windows(2)
                .any(|pair| pair[0] == flag && pair[1] == value));
        }
    }
    config.runtime.as_mut().unwrap().accelerator = qemu::Accelerator::Hvf;
    assert!(config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("tcg_threading"));
}
struct Images(PathBuf);
impl Images {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "fl-plan-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&base).unwrap();
        for filename in [
            "disk,with spaces.qcow2",
            "firmware.fd",
            "nvram.fd",
            "installer.iso",
        ] {
            fs::write(base.join(filename), b"fixture, not bootable").unwrap();
        }
        Self(fs::canonicalize(base).unwrap())
    }
    fn config(&self) -> Config {
        Config::parse("name='Windows'\ncpus=4\nmemory_mib=4096\ndisk='disk,with spaces.qcow2'\ninstaller='installer.iso'\n[runtime]\nfirmware='firmware.fd'\nnvram='nvram.fd'\naccelerator='tcg'\nheadless=true\ndisk_format='qcow2'").unwrap()
    }
}
impl Drop for Images {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn disk_paths_are_json_values_and_format_is_explicit() {
    let images = Images::new();
    let config = images.config();
    let args = qemu::arguments(&config, &images.0, &images.0.join("qmp.sock")).unwrap();
    let disk: serde_json::Value = args
        .windows(2)
        .filter(|pair| pair[0] == "-blockdev")
        .map(|pair| serde_json::from_str::<serde_json::Value>(&pair[1]).unwrap())
        .find(|v| v["node-name"] == "disk")
        .unwrap();
    assert_eq!(disk["driver"], "qcow2");
    assert_eq!(
        disk["file"]["filename"],
        images.0.join("disk,with spaces.qcow2").to_str().unwrap()
    );
    assert!(args.iter().any(|a| a == "none"));
    assert!(!args.iter().any(|a| a.contains("hostfwd")));
    assert!(
        !args.iter().any(|a| a == "-no-reboot"),
        "Windows installer must be allowed to reboot"
    );
}
#[test]
fn usb_network_is_opt_in_without_host_port_forwarding() {
    let images = Images::new();
    let mut config = images.config();
    let runtime = config.runtime.as_mut().unwrap();
    runtime.network_device = qemu::NetworkDevice::Usb;
    let disabled = qemu::arguments(&config, &images.0, &images.0.join("qmp.sock")).unwrap();
    assert!(!disabled.iter().any(|arg| arg.starts_with("usb-net")));
    config.runtime.as_mut().unwrap().network = true;
    let enabled = qemu::arguments(&config, &images.0, &images.0.join("qmp.sock")).unwrap();
    assert!(enabled.iter().any(|arg| arg == "usb-net,netdev=net0"));
    assert!(!enabled.iter().any(|arg| arg.contains("hostfwd")));
}
#[test]
fn writable_images_cannot_alias_firmware_even_through_hardlinks() {
    let images = Images::new();
    let config = images.config();
    fs::remove_file(images.0.join("nvram.fd")).unwrap();
    fs::hard_link(images.0.join("firmware.fd"), images.0.join("nvram.fd")).unwrap();
    assert!(
        qemu::arguments(&config, &images.0, &images.0.join("qmp.sock"))
            .unwrap_err()
            .to_string()
            .contains("alias")
    );
}
#[test]
fn installer_cannot_alias_guest_disk() {
    let images = Images::new();
    let config = images.config();
    fs::remove_file(images.0.join("installer.iso")).unwrap();
    fs::hard_link(
        images.0.join("disk,with spaces.qcow2"),
        images.0.join("installer.iso"),
    )
    .unwrap();
    assert!(qemu::arguments(&config, &images.0, &images.0.join("qmp.sock")).is_err());
}
#[test]
fn hvf_never_falls_back_silently_to_software_emulation() {
    if !freelane::supported_host() {
        let images = Images::new();
        let mut config = images.config();
        config.runtime.as_mut().unwrap().accelerator = qemu::Accelerator::Hvf;
        assert!(
            qemu::arguments(&config, &images.0, &images.0.join("qmp.sock"))
                .unwrap_err()
                .to_string()
                .contains("Apple Silicon")
        );
    }
}
#[test]
fn initialization_cannot_overwrite_existing_user_directory() {
    let images = Images::new();
    let bytes = fs::read(images.0.join("nvram.fd")).unwrap();
    assert!(qemu::initialize(
        &images.0,
        &images.0.join("firmware.fd"),
        &images.0.join("nvram.fd"),
        std::path::Path::new("missing-qemu-img")
    )
    .is_err());
    assert_eq!(fs::read(images.0.join("nvram.fd")).unwrap(), bytes);
}
#[test]
fn names_cannot_inject_qemu_suboptions() {
    let images = Images::new();
    let mut config = images.config();
    for name in ["Windows,debug-threads=on", "Windows\nVM", "Windows\0VM"] {
        config.name = name.into();
        assert!(config.validate().is_err());
    }
}

#[test]
fn arm_virtio_devices_disable_legacy_port_io() {
    let images = Images::new();
    let mut config = images.config();
    let runtime = config.runtime.as_mut().unwrap();
    runtime.network = true;
    runtime.guest_agent = true;
    let args = qemu::arguments(&config, &images.0, &images.0.join("qmp.sock")).unwrap();
    let devices: Vec<_> = args
        .windows(2)
        .filter(|pair| pair[0] == "-device")
        .map(|pair| pair[1].as_str())
        .filter(|device| device.starts_with("virtio-"))
        .collect();
    assert_eq!(devices.len(), 2);
    assert!(
        devices.iter().all(|device| device
            .split(',')
            .any(|option| option == "disable-legacy=on")),
        "Windows ARM needs memory-mapped modern virtio, not legacy port I/O"
    );
}

#[test]
fn shared_folder_uses_readonly_usb_and_literal_json_path() {
    let images = Images::new();
    let shared = images.0.join("share, spaces & files");
    fs::create_dir(&shared).unwrap();
    fs::write(shared.join("hello.txt"), "shared bytes").unwrap();
    let mut config = images.config();
    config.runtime.as_mut().unwrap().shared_directory = Some(shared.clone());
    let args = qemu::arguments(&config, &images.0, &images.0.join("qmp.sock")).unwrap();
    let block: serde_json::Value = args
        .windows(2)
        .filter(|p| p[0] == "-blockdev")
        .map(|p| serde_json::from_str::<serde_json::Value>(&p[1]).unwrap())
        .find(|b| b["node-name"] == "shared")
        .unwrap();
    assert_eq!(block["dir"], shared.to_str().unwrap());
    assert_eq!(block["driver"], "vvfat");
    assert_eq!(block["rw"], false);
    assert_eq!(block["read-only"], true);
    assert!(args
        .iter()
        .any(|a| a == "usb-storage,drive=shared,removable=on"));
    config.runtime.as_mut().unwrap().shared_directory = Some(images.0.join("installer.iso"));
    assert!(qemu::arguments(&config, &images.0, &images.0.join("qmp.sock")).is_err());
}

#[test]
fn settings_save_preserves_backup_and_rejects_running_or_conflicting_edits() {
    use fs2::FileExt;
    let images = Images::new();
    let path = images.0.join("vm.toml");
    let mut config = images.config();
    let original = toml::to_string_pretty(&config).unwrap();
    fs::write(&path, &original).unwrap();
    config.cpus = 6;
    let backup = qemu::save_config(&path, &original, &config).unwrap();
    assert_eq!(fs::read_to_string(backup).unwrap(), original);
    let saved = fs::read_to_string(&path).unwrap();
    assert_eq!(Config::parse(&saved).unwrap().cpus, 6);
    assert!(qemu::save_config(&path, &original, &config)
        .unwrap_err()
        .to_string()
        .contains("changed on disk"));
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(qemu::runtime_dir(&path).unwrap().join("lock"))
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    assert!(qemu::save_config(&path, &saved, &config)
        .unwrap_err()
        .to_string()
        .contains("Shut down"));
    assert_eq!(fs::read_to_string(&path).unwrap(), saved);
    drop(lock);
    config.memory_mib = 1;
    assert!(qemu::save_config(&path, &saved, &config).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), saved);
    let _ = fs::remove_dir_all(qemu::runtime_dir(&path).unwrap());
}

#[test]
fn readwrite_share_requires_ubuntu_and_escapes_qemu_separators() {
    use freelane::GuestOs;
    let images = Images::new();
    let shared = images.0.join("share,security_model=none");
    fs::create_dir(&shared).unwrap();
    let mut config = images.config();
    config.runtime.as_mut().unwrap().shared_directory = Some(shared.clone());
    config.runtime.as_mut().unwrap().share_mode = qemu::ShareMode::Virtio9p;
    assert!(config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("Ubuntu"));
    config.guest_os = GuestOs::Ubuntu;
    let args = qemu::arguments(&config, &images.0, &images.0.join("qmp.sock")).unwrap();
    assert!(args.iter().any(|arg| arg
        == &format!(
            "local,id=hostshare,path={},security_model=mapped-xattr",
            shared.to_str().unwrap().replace(',', ",,")
        )));
    assert!(args
        .iter()
        .any(|arg| arg == "virtio-9p-pci,disable-legacy=on,fsdev=hostshare,mount_tag=freelane"));
    assert!(!args
        .iter()
        .any(|arg| arg == "usb-storage,drive=shared,removable=on"));
    config.runtime.as_mut().unwrap().shared_directory = Some(PathBuf::new());
    assert!(config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("must not be empty"));
}

#[test]
fn physical_usb_requires_exact_bounded_selectors_without_duplicates() {
    let images = Images::new();
    let mut config = images.config();
    let device = qemu::UsbHostDevice {
        bus: 2,
        address: 3,
        vendor_id: 0x1234,
        product_id: 0xabcd,
    };
    config
        .runtime
        .as_mut()
        .unwrap()
        .usb_devices
        .push(device.clone());
    let args = qemu::arguments(&config, &images.0, &images.0.join("qmp.sock")).unwrap();
    assert!(args
        .iter()
        .any(|arg| arg == "usb-host,hostbus=2,hostaddr=3,vendorid=0x1234,productid=0xabcd"));
    config.runtime.as_mut().unwrap().usb_devices.push(device);
    assert!(config.validate().is_err());
    config.runtime.as_mut().unwrap().usb_devices.pop();
    config.runtime.as_mut().unwrap().usb_devices[0].address = 0;
    assert!(config.validate().is_err());
    config.runtime.as_mut().unwrap().usb_devices[0].address = 3;
    config.runtime.as_mut().unwrap().usb_devices[0].vendor_id = 0;
    assert!(config.validate().is_err());
}

#[test]
fn virtio_gpu_is_explicit_linux_only_and_preserves_basic_firmware_display() {
    let images = Images::new();
    let mut config = images.config();
    let socket = images.0.join("qmp.sock");
    let original = qemu::arguments(&config, &images.0, &socket).unwrap();
    assert!(!original.iter().any(|arg| arg.starts_with("virtio-gpu")));
    config.runtime.as_mut().unwrap().graphics = qemu::Graphics::Virtio2d;
    assert!(config.validate().is_err());
    config.guest_os = freelane::GuestOs::Ubuntu;
    let args = qemu::arguments(&config, &images.0, &socket).unwrap();
    assert!(args
        .iter()
        .any(|arg| arg == "virtio-gpu-pci,id=freelane-gpu,disable-legacy=on"));
    assert!(args.iter().any(|arg| arg == "ramfb"));
    assert!(!args
        .iter()
        .any(|arg| arg.contains("gl=on") || arg.contains("virgl")));
    let roundtrip = Config::parse(&toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(
        roundtrip.runtime.unwrap().graphics,
        qemu::Graphics::Virtio2d
    );
}

#[test]
fn virtio_network_for_disk_boot_does_not_require_an_external_pxe_rom() {
    let images = Images::new();
    let mut config = images.config();
    config.runtime.as_mut().unwrap().network = true;
    let args = qemu::arguments(&config, &images.0, &images.0.join("qmp.sock")).unwrap();
    let device = args
        .iter()
        .find(|arg| arg.starts_with("virtio-net-pci,"))
        .unwrap();
    assert!(
        device.split(',').any(|option| option == "romfile="),
        "Disk/ISO boot must work without optional PXE ROM packages"
    );
}

#[test]
fn live_readonly_share_is_enforced_by_backend_and_remains_ubuntu_only() {
    let images = Images::new();
    let shared = images.0.join("readonly,security_model=none");
    fs::create_dir(&shared).unwrap();
    let mut config = images.config();
    let runtime = config.runtime.as_mut().unwrap();
    runtime.shared_directory = Some(shared.clone());
    runtime.share_mode = qemu::ShareMode::Virtio9pReadonly;
    assert!(config.validate().is_err());
    config.guest_os = freelane::GuestOs::Ubuntu;
    let args = qemu::arguments(&config, &images.0, &images.0.join("qmp.sock")).unwrap();
    assert!(args.iter().any(|arg| arg
        == &format!(
            "local,id=hostshare,path={},security_model=mapped-xattr,readonly=on",
            shared.to_str().unwrap().replace(',', ",,")
        )));
    let saved = toml::to_string(&config).unwrap();
    assert!(saved.contains("virtio9p_readonly"));
    assert_eq!(
        freelane::Config::parse(&saved)
            .unwrap()
            .runtime
            .unwrap()
            .share_mode,
        qemu::ShareMode::Virtio9pReadonly
    );
}

#[test]
fn video_port_is_separate_and_uses_the_existing_virtio_controller() {
    let images = Images::new();
    let mut config = images.config();
    config.runtime.as_mut().unwrap().guest_agent = true;
    let args = qemu::arguments(&config, &images.0, &images.0.join("qmp.sock")).unwrap();
    assert_eq!(
        args.iter()
            .filter(|arg| arg.starts_with("virtio-serial-pci,"))
            .count(),
        1
    );
    for (id, name, socket) in [
        ("guestagent", "org.freelane.agent.0", "agent.sock"),
        ("guestvideo", "org.freelane.video.0", "video.sock"),
    ] {
        assert!(args
            .iter()
            .any(|arg| arg == &format!("virtserialport,chardev={id},name={name}")));
        assert!(args
            .iter()
            .any(|arg| arg.starts_with(&format!("socket,id={id},path="))
                && arg.contains(socket)
                && arg.ends_with("server=on,wait=off")));
    }
    assert!(!args.iter().any(|arg| arg.starts_with("tcp:")));
    config.runtime.as_mut().unwrap().guest_agent = false;
    let args = qemu::arguments(&config, &images.0, &images.0.join("qmp.sock")).unwrap();
    assert!(!args
        .iter()
        .any(|arg| arg.contains("org.freelane.") || arg.contains("video.sock")));
}
