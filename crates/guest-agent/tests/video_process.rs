use freelane_agent_protocol::{video::Binding, video_control::Parameters, WindowTarget};
use freelane_guest_agent::video_process::setup_bytes;
use freelane_guest_agent::{video::Encoder, video_process::ProcessEncoder};
use std::{
    io::{self, Read},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
fn parameters() -> Parameters {
    Parameters {
        target: WindowTarget {
            handle: 1,
            pid: 2,
            created: 3,
            incarnation: 4,
        },
        binding: Binding {
            nonce: [5; 16],
            stream: 6,
            generation: 7,
        },
        width: 320,
        height: 180,
        fps: 30,
    }
}

fn authorize(_: &WindowTarget) -> io::Result<()> {
    Ok(())
}
static CHECKS: AtomicUsize = AtomicUsize::new(0);
fn becomes_stale(_: &WindowTarget) -> io::Result<()> {
    if CHECKS.fetch_add(1, Ordering::SeqCst) > 0 {
        Err(io::ErrorKind::NotFound.into())
    } else {
        Ok(())
    }
}
fn worker_command(mode: &str) -> Command {
    static FIXTURE: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let directory = FIXTURE.get_or_init(|| {
        let directory = tempfile::tempdir().unwrap();
        let source =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video_worker.rs");
        let output = directory
            .path()
            .join(format!("worker{}", std::env::consts::EXE_SUFFIX));
        let result = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .arg("--edition=2021")
            .arg(source)
            .arg("-o")
            .arg(output)
            .status()
            .unwrap();
        assert!(result.success());
        directory
    });
    let mut command = Command::new(
        directory
            .path()
            .join(format!("worker{}", std::env::consts::EXE_SUFFIX)),
    );
    command.env("FREELANE_WORKER_FIXTURE", mode);
    command
}

#[test]
fn helper_handshake_stop_and_keyframe_coalescing_preserve_other_process() {
    let mut other = worker_command("unrelated")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut backend = ProcessEncoder::new(worker_command("normal"), authorize);
    assert!(backend.available());
    backend.start(&parameters()).unwrap();
    for _ in 0..1000 {
        backend.keyframe().unwrap();
    }
    assert!(backend.running().unwrap());
    backend.stop().unwrap();
    backend.stop().unwrap();
    assert!(other.try_wait().unwrap().is_none());
    other.kill().unwrap();
    other.wait().unwrap();
}
#[test]
fn malformed_early_and_stalled_startups_fail_with_owned_cleanup() {
    for mode in ["wrong", "early", "stall"] {
        let mut backend = ProcessEncoder::new(worker_command(mode), authorize);
        let start = Instant::now();
        assert!(backend.start(&parameters()).is_err(), "{mode}");
        backend.stop().unwrap();
        assert!(start.elapsed() < Duration::from_secs(4));
        assert!(!backend.running().unwrap());
    }
}
#[test]
fn authorization_is_rechecked_between_ready_and_go() {
    CHECKS.store(0, Ordering::SeqCst);
    let mut backend = ProcessEncoder::new(worker_command("normal"), becomes_stale);
    assert!(backend.start(&parameters()).is_err());
    backend.stop().unwrap();
    assert_eq!(CHECKS.load(Ordering::SeqCst), 2);
}
#[test]
fn drop_closes_owned_resources_without_terminating_another_process() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let mut command = worker_command("normal");
    command.env(
        "FREELANE_FIXTURE_SOCKET",
        listener.local_addr().unwrap().to_string(),
    );
    let mut other = worker_command("unrelated")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut backend = ProcessEncoder::new(command, authorize);
    backend.start(&parameters()).unwrap();
    let (mut peer, _) = listener.accept().unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    drop(backend);
    assert_eq!(peer.read(&mut [0]).unwrap(), 0);
    assert!(other.try_wait().unwrap().is_none());
    other.kill().unwrap();
    other.wait().unwrap();
}

#[test]
fn setup_has_fixed_big_endian_layout_and_validates_before_encoding() {
    let p = parameters();
    let bytes = setup_bytes(&p).unwrap();
    assert_eq!(bytes.len(), 80);
    assert_eq!(&bytes[..8], b"FLW1\0P\0\0");
    assert_eq!(&bytes[8..16], &1u64.to_be_bytes());
    assert_eq!(&bytes[16..20], &2u32.to_be_bytes());
    assert_eq!(&bytes[24..32], &3u64.to_be_bytes());
    assert_eq!(&bytes[32..40], &4u64.to_be_bytes());
    assert_eq!(&bytes[40..56], &[5; 16]);
    assert_eq!(&bytes[56..64], &6u64.to_be_bytes());
    assert_eq!(&bytes[64..72], &7u64.to_be_bytes());
    assert_eq!(&bytes[72..], &[1, 64, 0, 180, 30, 0, 0, 0]);
    let mut bad = p;
    bad.target.incarnation = 0;
    assert!(setup_bytes(&bad).is_err());
}
