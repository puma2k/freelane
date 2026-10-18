//! Real QEMU tests; run explicitly after installing QEMU and AArch64 EDK2 firmware.
use freelane::{
    qemu::{self, QemuBackend},
    qmp::Qmp,
    Backend, Config,
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct Workspace(PathBuf);
impl Workspace {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "fl-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn await_socket(child: &mut Child, path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(child.try_wait().unwrap().is_none(), "QEMU exited early");
        assert!(Instant::now() < deadline, "QMP socket never appeared");
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn installer_iso(base: &Path) -> PathBuf {
    let tree = base.join("iso-tree");
    fs::create_dir(&tree).unwrap();
    let executable = base.join("BOOTAA64.EFI");
    assert!(Command::new("rustc")
        .args([
            "--edition=2021",
            "--target",
            "aarch64-unknown-uefi",
            "-C",
            "opt-level=s",
            "-C",
            "panic=abort",
            "-o"
        ])
        .arg(&executable)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/uefi_guest.rs"))
        .status()
        .unwrap()
        .success());
    let fat = tree.join("efiboot.img");
    let file = fs::File::create(&fat).unwrap();
    file.set_len(2880 * 1024).unwrap();
    drop(file);
    assert!(Command::new("mformat")
        .arg("-i")
        .arg(&fat)
        .args(["-f", "2880", "::"])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("mmd")
        .arg("-i")
        .arg(&fat)
        .args(["::/EFI", "::/EFI/BOOT"])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("mcopy")
        .arg("-i")
        .arg(&fat)
        .arg(&executable)
        .arg("::/EFI/BOOT/BOOTAA64.EFI")
        .status()
        .unwrap()
        .success());
    let iso = base.join("installer.iso");
    assert!(Command::new("xorriso")
        .args([
            "-as",
            "mkisofs",
            "-quiet",
            "-R",
            "-J",
            "-V",
            "FREELANE_TEST",
            "-e",
            "efiboot.img",
            "-no-emul-boot",
            "-o"
        ])
        .arg(&iso)
        .arg(&tree)
        .status()
        .unwrap()
        .success());
    iso
}
#[test]
#[ignore = "requires qemu-system-aarch64 in PATH"]
fn arm_guest_executes_and_qmp_controls_real_cpu() {
    let temp = Workspace::new("cpu");
    // AArch64 guest: write a fixed marker to PL011 UART, then WFI. virt-10.0 fixes the device map.
    let mut instructions = vec![0xd2a12000u32]; // MOVZ X0, #0x900, LSL #16 => 0x09000000
    for byte in b"FREELANE_ARM_OK\n" {
        instructions.push(0x52800001 | (u32::from(*byte) << 5)); // MOVZ W1, byte
        instructions.push(0xb9000001); // STR W1, [X0]
    }
    instructions.extend([0xd503207f, 0x17ffffff]); // WFI; B back to WFI
    let binary: Vec<u8> = instructions
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    let kernel = temp.0.join("guest.bin");
    fs::write(&kernel, binary).unwrap();
    let serial = temp.0.join("serial.txt");
    let socket = temp.0.join("qmp.sock");
    let log = fs::File::create(temp.0.join("qemu.log")).unwrap();
    let mut process = Process(
        Command::new("qemu-system-aarch64")
            .args([
                "-machine",
                "virt-10.0",
                "-cpu",
                "cortex-a57",
                "-accel",
                "tcg",
                "-m",
                "128",
                "-display",
                "none",
                "-nic",
                "none",
                "-monitor",
                "none",
                "-S",
                "-kernel",
            ])
            .arg(&kernel)
            .arg("-serial")
            .arg(format!("file:{}", serial.display()))
            .arg("-qmp")
            .arg(format!("unix:{},server=on,wait=off", socket.display()))
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    await_socket(&mut process.0, &socket);
    let mut qmp = Qmp::connect(&socket).unwrap();
    assert_eq!(qmp.peer_pid().unwrap(), process.0.id());
    assert!(
        freelane::telemetry::process(qmp.peer_pid().unwrap())
            .unwrap()
            .resident_bytes
            > 0
    );
    assert_eq!(qmp.execute("query-status", None).unwrap()["running"], false);
    qmp.execute("cont", None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if fs::read_to_string(&serial)
            .unwrap_or_default()
            .contains("FREELANE_ARM_OK")
        {
            break;
        }
        assert!(Instant::now() < deadline, "guest never printed marker");
        std::thread::sleep(Duration::from_millis(20));
    }
    qmp.execute("stop", None).unwrap();
    assert_eq!(qmp.execute("query-status", None).unwrap()["running"], false);
    qmp.execute("cont", None).unwrap();
    assert_eq!(qmp.execute("query-status", None).unwrap()["running"], true);
    assert!(qmp.execute("freelane-invalid-command", None).is_err());
    // A command error must not desynchronize subsequent replies/events.
    assert_eq!(qmp.execute("query-status", None).unwrap()["running"], true);
    qmp.execute("quit", None).unwrap();
    assert!(process.0.wait().unwrap().success());
}

#[test]
#[ignore = "requires QEMU, qemu-img, swtpm and firmware paths"]
fn uefi_boots_with_backend_and_private_nvram() {
    uefi_boot_with_threading(None);
}

#[test]
#[ignore = "requires QEMU, qemu-img, swtpm and firmware paths"]
fn uefi_boots_with_single_threaded_tcg() {
    uefi_boot_with_threading(Some(qemu::TcgThreading::Single));
}

fn uefi_boot_with_threading(threading: Option<qemu::TcgThreading>) {
    let temp = Workspace::new("uefi");
    let firmware =
        PathBuf::from(std::env::var_os("FREELANE_TEST_FIRMWARE").expect("firmware path required"));
    let template =
        PathBuf::from(std::env::var_os("FREELANE_TEST_NVRAM").expect("NVRAM template required"));
    let template_before = fs::read(&template).unwrap();
    let config_path = qemu::initialize(
        &temp.0.join("vm"),
        &firmware,
        &template,
        Path::new("qemu-img"),
    )
    .unwrap();
    // Must reject reinitialization without touching existing images.
    assert!(qemu::initialize(
        &temp.0.join("vm"),
        &firmware,
        &template,
        Path::new("qemu-img")
    )
    .is_err());
    let mut config = Config::parse(&fs::read_to_string(&config_path).unwrap()).unwrap();
    let runtime = config.runtime.as_mut().unwrap();
    runtime.accelerator = qemu::Accelerator::Tcg;
    runtime.tcg_threading = threading;
    runtime.headless = true;
    runtime.network = true;
    runtime.network_device = qemu::NetworkDevice::Usb;
    config.cpus = if threading == Some(qemu::TcgThreading::Single) {
        4
    } else {
        1
    };
    config.memory_mib = 2048;
    config.runtime.as_mut().unwrap().guest_agent = true;
    fs::write(&config_path, toml::to_string_pretty(&config).unwrap()).unwrap();
    let mut backend = QemuBackend::new(&config_path).unwrap();
    backend.prepare(&config).unwrap();
    let mut second = QemuBackend::new(&config_path).unwrap();
    assert!(
        second.prepare(&config).is_err(),
        "concurrent VM launch accepted"
    );
    let baseline_path = temp.0.join("before-boot.ppm");
    qemu::screenshot(&config_path, &baseline_path).unwrap();
    let baseline = fs::read(&baseline_path).unwrap();
    assert!(baseline.starts_with(b"P6\n"));
    assert!(qemu::screenshot(&config_path, &baseline_path).is_err());
    assert_eq!(fs::read(&baseline_path).unwrap(), baseline);
    assert!(qemu::send_keys(&config_path, &["ret;quit".into()]).is_err());
    backend.start().unwrap();
    let screenshot = temp.0.join("uefi.ppm");
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut qmp = Qmp::connect(&backend.directory().join("qmp.sock")).unwrap();
    let cpus = qmp.execute("query-cpus-fast", None).unwrap();
    let cpus = cpus.as_array().unwrap();
    assert_eq!(cpus.len(), usize::from(config.cpus));
    if threading == Some(qemu::TcgThreading::Single) {
        let threads: std::collections::HashSet<u64> = cpus
            .iter()
            .map(|cpu| cpu["thread-id"].as_u64().unwrap())
            .collect();
        assert_eq!(
            threads.len(),
            1,
            "TCG override did not serialize guest CPUs"
        );
    }
    loop {
        assert!(backend.poll().unwrap().is_none(), "UEFI process exited");
        if qmp
            .execute(
                "screendump",
                Some(json!({"filename": screenshot.to_str().unwrap()})),
            )
            .is_ok()
        {
            let bytes = fs::read(&screenshot).unwrap();
            // A real framebuffer must exist and contain a drawn, nonuniform image.
            if bytes != baseline
                && bytes.starts_with(b"P6\n")
                && bytes.len() > 100_000
                && bytes[100..]
                    .windows(3)
                    .any(|pixel| pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0)
            {
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "UEFI produced no framebuffer; log: {}",
            fs::read_to_string(backend.directory().join("qemu.log")).unwrap_or_default()
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    let pci = qmp.execute("query-pci", None).unwrap();
    let serial = pci
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|bus| bus["devices"].as_array().unwrap())
        .find(|device| device["id"]["vendor"] == 0x1af4 && device["id"]["device"] == 0x1043)
        .expect("modern virtio serial PCI device missing");
    assert!(
        serial["regions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|region| region["type"] != "io"),
        "ARM virtio serial exposed legacy port I/O"
    );
    // Do not stop at a firmware logo: reach the shell and execute a guest command through USB input.
    let shell_deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let text = fs::read_to_string(backend.directory().join("serial.log")).unwrap_or_default();
        if text.contains("Shell>") {
            break;
        }
        assert!(
            Instant::now() < shell_deadline,
            "UEFI shell never appeared: {text}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    drop(qmp);
    for key in [
        "e",
        "c",
        "h",
        "o",
        "spc",
        "f",
        "r",
        "e",
        "e",
        "l",
        "a",
        "n",
        "e",
        "shift-minus",
        "u",
        "e",
        "f",
        "i",
        "shift-minus",
        "o",
        "k",
        "ret",
    ] {
        let chord: Vec<String> = key.split('-').map(String::from).collect();
        qemu::send_keys(&config_path, &chord).unwrap();
    }
    let mut qmp = Qmp::connect(&backend.directory().join("qmp.sock")).unwrap();
    let command_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let text = fs::read_to_string(backend.directory().join("serial.log")).unwrap_or_default();
        // Firmware inserts ANSI cursor moves between echoed keypresses; require the separate command output line.
        if text.contains("\r\nfreelane_uefi_ok\r\n") {
            break;
        }
        assert!(
            Instant::now() < command_deadline,
            "USB keyboard/UEFI echo failed: {text}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    qmp.execute(
        "screendump",
        Some(json!({"filename": screenshot.to_str().unwrap()})),
    )
    .unwrap();
    let serial = fs::read_to_string(backend.directory().join("serial.log")).unwrap_or_default();
    assert!(
        serial.contains("UEFI") || serial.contains("BdsDxe") || serial.contains("EDK"),
        "No guest firmware witness: {serial}"
    );
    let channels = qmp.execute("query-chardev", None).unwrap();
    assert!(channels
        .as_array()
        .unwrap()
        .iter()
        .any(|channel| channel["label"] == "guestagent"));
    assert!(channels
        .as_array()
        .unwrap()
        .iter()
        .any(|channel| channel["label"] == "guestvideo"));
    let video = freelane::video::VideoBroker::connect(
        &config_path,
        freelane_agent_protocol::WindowTarget {
            handle: 1,
            pid: 1,
            created: 1,
            incarnation: 1,
        },
        freelane_agent_protocol::video::Binding {
            nonce: [1; 16],
            stream: 1,
            generation: 1,
        },
    )
    .unwrap(); // Owned transport fixture, not a claim of Windows capture authorization.
    drop(video);
    assert!(qmp.execute("query-status", None).unwrap()["running"]
        .as_bool()
        .unwrap());
    let tpms = qmp.execute("query-tpm", None).unwrap();
    assert_eq!(tpms[0]["options"]["type"], "emulator");
    if let Some(artifact) = std::env::var_os("FREELANE_TEST_SCREENSHOT") {
        fs::copy(&screenshot, artifact).unwrap();
    }
    drop(qmp);
    backend.pause().unwrap();
    assert_eq!(
        qemu::control(&config_path, "status").unwrap()["running"],
        false
    );
    backend.resume().unwrap();
    assert_eq!(
        qemu::control(&config_path, "status").unwrap()["running"],
        true
    );
    backend.stop().unwrap();
    assert!(!backend.directory().join("qmp.sock").exists());
    assert!(!backend.directory().join("agent.sock").exists());
    assert!(!backend.directory().join("video.sock").exists());
    assert_eq!(
        fs::read(&template).unwrap(),
        template_before,
        "shared NVRAM template modified"
    );
    // Restart same private state to exercise cleanup and repeatability.
    backend.prepare(&config).unwrap();
    backend.start().unwrap();
    backend.stop().unwrap();
}

#[test]
#[ignore = "requires QEMU, qemu-img, swtpm, FREELANE_TEST_FIRMWARE and FREELANE_TEST_NVRAM"]
fn foreground_cli_can_be_controlled_and_stops_on_signal() {
    let temp = Workspace::new("cli");
    let config_path = qemu::initialize(
        &temp.0.join("vm"),
        &PathBuf::from(std::env::var_os("FREELANE_TEST_FIRMWARE").unwrap()),
        &PathBuf::from(std::env::var_os("FREELANE_TEST_NVRAM").unwrap()),
        Path::new("qemu-img"),
    )
    .unwrap();
    let mut config = Config::parse(&fs::read_to_string(&config_path).unwrap()).unwrap();
    let runtime = config.runtime.as_mut().unwrap();
    runtime.accelerator = qemu::Accelerator::Tcg;
    runtime.headless = true;
    config.cpus = 1;
    fs::write(&config_path, toml::to_string_pretty(&config).unwrap()).unwrap();
    let invoke = |action: &str| {
        Command::new(env!("CARGO_BIN_EXE_freelane"))
            .arg(action)
            .arg(&config_path)
            .output()
            .unwrap()
    };
    assert!(invoke("plan").status.success());
    struct Cli {
        child: Child,
        config: PathBuf,
    }
    impl Drop for Cli {
        fn drop(&mut self) {
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = qemu::control(&self.config, "stop");
                let _ = self.child.kill();
            }
            let _ = self.child.wait();
        }
    }
    for signal_shutdown in [false, true] {
        let mut process = Cli {
            child: Command::new(env!("CARGO_BIN_EXE_freelane"))
                .arg("start")
                .arg(&config_path)
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
            config: config_path.clone(),
        };
        let socket = qemu::runtime_dir(&config_path).unwrap().join("qmp.sock");
        await_socket(&mut process.child, &socket);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(value) = qemu::control(&config_path, "status") {
                if value["running"] == true {
                    break;
                }
            }
            assert!(Instant::now() < deadline, "CLI never started guest");
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(invoke("pause").status.success());
        let output = invoke("status");
        assert!(output.status.success());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["running"],
            false
        );
        assert!(invoke("resume").status.success());
        if signal_shutdown {
            assert!(Command::new("kill")
                .args(["-TERM", &process.child.id().to_string()])
                .status()
                .unwrap()
                .success());
        } else {
            assert!(invoke("stop").status.success());
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = process.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline, "CLI leaked running processes");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!socket.exists());
    }
}

#[test]
#[ignore = "requires QEMU, qemu-img, swtpm, mtools, xorriso, aarch64-unknown-uefi and firmware paths"]
fn installer_cdrom_executes_arm64_efi_application() {
    installer_profile_boot(freelane::GuestOs::Windows);
}

#[test]
#[ignore = "requires QEMU, qemu-img, mtools, xorriso, aarch64-unknown-uefi and firmware paths"]
fn ubuntu_profile_boots_efi_installer_without_tpm() {
    installer_profile_boot(freelane::GuestOs::Ubuntu);
}

fn installer_profile_boot(guest_os: freelane::GuestOs) {
    let temp = Workspace::new("media");
    let firmware = PathBuf::from(std::env::var_os("FREELANE_TEST_FIRMWARE").unwrap());
    let nvram = PathBuf::from(std::env::var_os("FREELANE_TEST_NVRAM").unwrap());
    let config_path = qemu::initialize_guest(
        &temp.0.join("vm"),
        &firmware,
        &nvram,
        Path::new("qemu-img"),
        guest_os,
    )
    .unwrap();
    let iso = installer_iso(&temp.0);
    let iso_before = fs::read(&iso).unwrap();
    let mut config = Config::parse(&fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(config.guest_os, guest_os);
    if guest_os == freelane::GuestOs::Ubuntu {
        assert_eq!(config.disk.to_str().unwrap(), "ubuntu.qcow2");
        assert!(config.runtime.as_ref().unwrap().tpm.is_none());
        config.runtime.as_mut().unwrap().serial_console = true;
        config.runtime.as_mut().unwrap().graphics = qemu::Graphics::Virtio2d;
    }
    config.cpus = 1;
    config.runtime.as_mut().unwrap().accelerator = qemu::Accelerator::Tcg;
    config.runtime.as_mut().unwrap().headless = true;
    config.installer = Some(iso.clone());
    let shared = temp.0.join("shared, files");
    fs::create_dir(&shared).unwrap();
    fs::write(shared.join("hello.txt"), b"shared host file bytes").unwrap();
    config.runtime.as_mut().unwrap().shared_directory = Some(shared.clone());
    fs::write(&config_path, toml::to_string_pretty(&config).unwrap()).unwrap();
    let mut backend = QemuBackend::new(&config_path).unwrap();
    backend.prepare(&config).unwrap();
    backend.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let serial = fs::read_to_string(backend.directory().join("serial.log")).unwrap_or_default();
        if serial.contains("FREELANE_INSTALL_MEDIA_OK") {
            break;
        }
        assert!(backend.poll().unwrap().is_none());
        assert!(
            Instant::now() < deadline,
            "EFI app never ran from CD-ROM: {serial}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut qmp = Qmp::connect(&backend.directory().join("qmp.sock")).unwrap();
    if guest_os == freelane::GuestOs::Ubuntu {
        assert!(qmp
            .execute("query-tpm", None)
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
    }
    let blocks = qmp.execute("query-block", None).unwrap();
    let cd = blocks
        .as_array()
        .unwrap()
        .iter()
        .find(|block| block["inserted"]["node-name"] == "installer")
        .expect("installer CD-ROM not attached");
    assert_eq!(cd["removable"], true);
    assert_eq!(cd["inserted"]["ro"], true);
    let share = blocks
        .as_array()
        .unwrap()
        .iter()
        .find(|block| block["inserted"]["node-name"] == "shared")
        .expect("shared USB volume not attached");
    assert_eq!(share["removable"], true);
    assert_eq!(share["inserted"]["ro"], true);
    drop(qmp);
    backend.stop().unwrap();
    assert_eq!(
        fs::read(shared.join("hello.txt")).unwrap(),
        b"shared host file bytes"
    );
    assert!(!backend.directory().join("console.sock").exists());
    assert_eq!(
        fs::read(&iso).unwrap(),
        iso_before,
        "installer ISO was modified"
    );
}
