use freelane::{qemu, video::VideoBroker};
use freelane_agent_protocol::{
    video::{write_record, Binding, Kind, Record},
    WindowTarget,
};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::{
        fs::{symlink, OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};
struct Fixture {
    _temp: tempfile::TempDir,
    config: PathBuf,
    dir: PathBuf,
    lock: Option<File>,
    listener: UnixListener,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("vm.toml");
        fs::write(&config, "").unwrap();
        let dir = qemu::runtime_dir(&config).unwrap();
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.join("lock"))
            .unwrap();
        lock.try_lock_exclusive().unwrap();
        let listener = UnixListener::bind(dir.join("video.sock")).unwrap();
        Self {
            _temp: temp,
            config,
            dir,
            lock: Some(lock),
            listener,
        }
    }
    fn connect(&self) -> (VideoBroker, UnixStream) {
        let broker = VideoBroker::connect(&self.config, target(), binding()).unwrap();
        let peer = self.listener.accept().unwrap().0;
        (broker, peer)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.dir).unwrap();
    }
}
fn target() -> WindowTarget {
    WindowTarget {
        handle: 4,
        pid: 5,
        created: 6,
        incarnation: 7,
    }
}
fn binding() -> Binding {
    Binding {
        nonce: [7; 16],
        stream: 1,
        generation: 1,
    }
}
fn config() -> Record {
    Record {
        binding: binding(),
        kind: Kind::Configuration,
        width: 320,
        height: 180,
        sequence: 0,
        pts_us: 0,
        payload: vec![
            1, 66, 0, 30, 255, 225, 0, 4, 0x67, 66, 0, 30, 1, 0, 2, 0x68, 0,
        ],
    }
}
fn frame(sequence: u64, idr: bool) -> Record {
    Record {
        kind: Kind::AccessUnit { idr },
        sequence,
        pts_us: sequence * 33333,
        payload: vec![0, 0, 0, 2, if idr { 0x65 } else { 0x41 }, 0x80],
        ..config()
    }
}
fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "worker did not complete");
        thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn one_reader_preserves_target_and_delivers_valid_frames() {
    let f = Fixture::new();
    let (b, mut peer) = f.connect();
    assert_eq!(b.target(), &target());
    assert!(VideoBroker::connect(&f.config, target(), binding()).is_err());
    write_record(&mut peer, &config()).unwrap();
    write_record(&mut peer, &frame(1, true)).unwrap();
    let mut got = None;
    wait_until(|| {
        got = b.pop();
        got.is_some()
    });
    assert_eq!(got.unwrap().sequence, 1);
    assert!(b.configuration().is_some());
    drop(b);
    assert!(f.dir.join("video.sock").exists());
    let (b, _peer) = f.connect();
    drop(b);
}
#[test]
fn malformed_and_disconnected_sources_clear_queued_frames() {
    for malformed in [false, true] {
        let f = Fixture::new();
        let (b, mut peer) = f.connect();
        write_record(&mut peer, &config()).unwrap();
        write_record(&mut peer, &frame(1, true)).unwrap();
        wait_until(|| b.configuration().is_some());
        if malformed {
            peer.write_all(&[0; 64]).unwrap();
        } else {
            drop(peer);
        }
        wait_until(|| b.ended());
        assert!(b.pop().is_none());
        assert!(b.configuration().is_none());
    }
}
#[test]
fn idle_and_partial_record_shutdown_are_prompt() {
    for prefix in [vec![], vec![b'F'], {
        let mut bytes = vec![];
        write_record(&mut bytes, &frame(1, true)).unwrap();
        bytes.truncate(66);
        bytes
    }] {
        let f = Fixture::new();
        let (b, mut peer) = f.connect();
        peer.write_all(&prefix).unwrap();
        thread::sleep(Duration::from_millis(15));
        let now = Instant::now();
        drop(b);
        assert!(now.elapsed() < Duration::from_secs(1));
        assert!(f.dir.join("video.sock").exists());
    }
}
#[test]
fn stopped_vm_unsafe_directory_and_symlinked_lease_are_rejected() {
    let mut f = Fixture::new();
    drop(f.lock.take());
    assert!(VideoBroker::connect(&f.config, target(), binding()).is_err());
    f.lock = Some(
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(f.dir.join("lock"))
            .unwrap(),
    );
    f.lock.as_ref().unwrap().try_lock_exclusive().unwrap();
    fs::set_permissions(&f.dir, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(VideoBroker::connect(&f.config, target(), binding()).is_err());
    fs::set_permissions(&f.dir, fs::Permissions::from_mode(0o700)).unwrap();
    let outside = f._temp.path().join("untouched");
    fs::write(&outside, "preserve").unwrap();
    symlink(&outside, f.dir.join("video-client.lock")).unwrap();
    assert!(VideoBroker::connect(&f.config, target(), binding()).is_err());
    assert_eq!(fs::read_to_string(outside).unwrap(), "preserve");
}
#[test]
fn partial_record_has_absolute_deadline_despite_trickle() {
    let f = Fixture::new();
    let (b, mut peer) = f.connect();
    let producer = thread::spawn(move || {
        for _ in 0..30 {
            if peer.write_all(b"F").is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    });
    let start = Instant::now();
    wait_until(|| b.ended());
    assert!(start.elapsed() < Duration::from_millis(2900));
    producer.join().unwrap();
}
#[test]
fn saturated_video_does_not_take_the_control_channel_lease() {
    use freelane_agent_protocol::{Action, Outcome, Response};
    let f = Fixture::new();
    let (b, mut peer) = f.connect();
    let source = thread::spawn(move || {
        if write_record(&mut peer, &config()).is_err() {
            return;
        }
        for seq in 1..u64::MAX {
            if write_record(&mut peer, &frame(seq, seq % 30 == 1)).is_err() {
                break;
            }
        }
    });
    let control = UnixListener::bind(f.dir.join("agent.sock")).unwrap();
    let responder = thread::spawn(move || {
        let (mut socket, _) = control.accept().unwrap();
        let request: freelane_agent_protocol::Request = freelane_agent_protocol::read_frame(
            &mut std::io::BufReader::new(socket.try_clone().unwrap()),
        )
        .unwrap();
        freelane_agent_protocol::write_frame(
            &mut socket,
            &Response {
                version: 1,
                id: request.id,
                outcome: Outcome::Pong {
                    os: "fixture".into(),
                    arch: "arm64".into(),
                },
            },
        )
        .unwrap();
    });
    wait_until(|| b.configuration().is_some());
    assert!(!source.is_finished());
    let start = Instant::now();
    let response = freelane::agent::request(&f.config, Action::Ping).unwrap();
    assert!(matches!(response.outcome, Outcome::Pong { .. }));
    assert!(start.elapsed() < Duration::from_secs(2));
    responder.join().unwrap();
    drop(b);
    source.join().unwrap();
}

#[test]
fn broker_descriptor_is_not_inherited_by_unrelated_child() {
    use std::io::Read;
    use std::os::fd::AsRawFd;
    let f = Fixture::new();
    let (b, mut peer) = f.connect();
    peer.set_nonblocking(true).unwrap();
    let mut child = std::process::Command::new("sleep")
        .arg("3")
        .spawn()
        .unwrap();
    drop(b);
    // Darwin rejects SO_RCVTIMEO in this disconnected-socket lifecycle.
    // Poll for EOF while the unrelated child is still alive; never wait for
    // its normal exit, which could hide an inherited descriptor.
    let mut descriptor = libc::pollfd {
        fd: peer.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let ready = unsafe { libc::poll(&mut descriptor, 1, 300) };
    let result = if ready > 0 {
        peer.read(&mut [0])
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "Video peer did not reach EOF before the child exited",
        ))
    };
    if let Err(error) = &result {
        // Public enum/numeric diagnostics only: no paths, payloads or raw logs.
        eprintln!(
            "::error title=Video descriptor EOF::kind={:?}, code={:?}",
            error.kind(),
            error.raw_os_error()
        );
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(ready > 0, "video EOF was not ready while child was alive");
    assert_eq!(result.unwrap(), 0, "child retained the video descriptor");
}

#[test]
fn existing_readable_supervisor_lock_inside_private_runtime_is_compatible() {
    let f = Fixture::new();
    fs::set_permissions(f.dir.join("lock"), fs::Permissions::from_mode(0o644)).unwrap();
    let (broker, _peer) = f.connect();
    drop(broker);
}

#[test]
fn record_and_end_wake_observers_for_delivery_and_fallback() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let f = Fixture::new();
    let count = Arc::new(AtomicUsize::new(0));
    let wakes = count.clone();
    let b = VideoBroker::connect_with_wakeup(
        &f.config,
        target(),
        binding(),
        Arc::new(move || {
            wakes.fetch_add(1, Ordering::SeqCst);
        }),
    )
    .unwrap();
    let mut peer = f.listener.accept().unwrap().0;
    write_record(&mut peer, &config()).unwrap();
    wait_until(|| count.load(Ordering::SeqCst) >= 1);
    write_record(&mut peer, &frame(1, true)).unwrap();
    wait_until(|| count.load(Ordering::SeqCst) >= 2);
    assert_eq!(b.pop().unwrap().sequence, 1);
    drop(peer);
    wait_until(|| count.load(Ordering::SeqCst) >= 3);
    assert!(b.ended());
}

fn serve_video_control(
    listener: &UnixListener,
    f: impl FnOnce(freelane_agent_protocol::Action) -> freelane_agent_protocol::Outcome,
) {
    use freelane_agent_protocol::{read_frame, write_frame, Request, Response};
    let end = Instant::now() + Duration::from_secs(5);
    listener.set_nonblocking(true).unwrap();
    let (mut peer, _) = loop {
        match listener.accept() {
            Ok(p) => break p,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < end => {
                thread::sleep(Duration::from_millis(2))
            }
            Err(e) => panic!("Owned control accept failed: {e}"),
        }
    };
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let request: Request =
        read_frame(&mut std::io::BufReader::new(peer.try_clone().unwrap())).unwrap();
    let outcome = f(request.action);
    write_frame(
        &mut peer,
        &Response {
            version: request.version,
            id: request.id,
            outcome,
        },
    )
    .unwrap();
}
fn video_info() -> freelane_agent_protocol::Outcome {
    use freelane_agent_protocol::{AgentInfo, Feature, Outcome};
    Outcome::AgentInfo {
        info: AgentInfo {
            agent_version: "owned-session-fixture".into(),
            os: "windows".into(),
            arch: "arm64".into(),
            protocols: vec![1, 2, 3, 9, 14],
            features: vec![Feature {
                name: "window_video".into(),
                protocol: 14,
            }],
            max_frame: 65536,
        },
    }
}
#[test]
fn live_session_drop_is_prompt_and_keeps_lease_until_exact_stop_finishes() {
    use freelane::video::LiveSession;
    use freelane_agent_protocol::{
        video_control::{Operation, Reply},
        Action, Outcome,
    };
    use std::sync::{atomic::Ordering, mpsc, Arc};
    let f = Fixture::new();
    let control = UnixListener::bind(f.dir.join("agent.sock")).unwrap();
    let video = f.listener.try_clone().unwrap();
    let (entered, waiting) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let owner = thread::spawn(move || {
        let mut parameters = None;
        while parameters.is_none() {
            serve_video_control(&control, |action| match action {
                Action::Ping => video_info(),
                Action::Video {
                    operation: Operation::Start { parameters: p },
                } => {
                    assert_eq!(p.target, target());
                    let mut peer = accept_video(&video);
                    let mut record = config();
                    record.binding = p.binding;
                    write_record(&mut peer, &record).unwrap();
                    record = frame(1, true);
                    record.binding = p.binding;
                    write_record(&mut peer, &record).unwrap();
                    parameters = Some((p.clone(), peer));
                    Outcome::Video {
                        report: Reply::Started { parameters: p },
                    }
                }
                _ => panic!("Unexpected startup mutation"),
            });
        }
        let (parameters, _peer) = parameters.unwrap();
        serve_video_control(&control, |action| {
            assert_eq!(
                action,
                Action::Video {
                    operation: Operation::Keyframe {
                        binding: parameters.binding
                    }
                }
            );
            entered.send(()).unwrap();
            blocked.recv_timeout(Duration::from_secs(3)).unwrap();
            Outcome::Video {
                report: Reply::KeyframeRequested {
                    binding: parameters.binding,
                },
            }
        });
        serve_video_control(&control, |action| {
            assert_eq!(
                action,
                Action::Video {
                    operation: Operation::Stop {
                        binding: parameters.binding
                    }
                }
            );
            Outcome::Video {
                report: Reply::Stopped {
                    binding: parameters.binding,
                },
            }
        });
    });
    let live = LiveSession::start(&f.config, target(), Arc::new(|| {})).unwrap();
    wait_until(|| live.configuration().is_some());
    assert_eq!(live.target(), &target());
    for _ in 0..1000 {
        live.request_keyframe();
    }
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    let retired = live.retirement();
    let before = Instant::now();
    drop(live);
    assert!(before.elapsed() < Duration::from_millis(100));
    assert!(!retired.load(Ordering::Acquire));
    assert!(VideoBroker::connect(&f.config, target(), binding()).is_err());
    release.send(()).unwrap();
    wait_until(|| retired.load(Ordering::Acquire));
    owner.join().unwrap();
    let (broker, _peer) = f.connect();
    drop(broker);
}
#[test]
fn uncertain_start_sends_only_its_own_stop_and_releases_broker() {
    use freelane::video::LiveSession;
    use freelane_agent_protocol::{
        video_control::{Operation, Reply},
        Action, Outcome,
    };
    use std::sync::Arc;
    let f = Fixture::new();
    let control = UnixListener::bind(f.dir.join("agent.sock")).unwrap();
    let video = f.listener.try_clone().unwrap();
    let owner = thread::spawn(move || {
        let mut attempted = None;
        while attempted.is_none() {
            serve_video_control(&control, |action| match action {
                Action::Ping => video_info(),
                Action::Video {
                    operation: Operation::Start { parameters },
                } => {
                    let peer = accept_video(&video);
                    attempted = Some((parameters.binding, peer));
                    let mut wrong = parameters;
                    wrong.binding.generation += 1;
                    Outcome::Video {
                        report: Reply::Started { parameters: wrong },
                    }
                }
                _ => panic!("Unexpected startup mutation"),
            });
        }
        let (binding, _peer) = attempted.unwrap();
        serve_video_control(&control, |action| {
            assert_eq!(
                action,
                Action::Video {
                    operation: Operation::Stop { binding }
                }
            );
            Outcome::Video {
                report: Reply::Stopped { binding },
            }
        });
    });
    assert!(LiveSession::start(&f.config, target(), Arc::new(|| {})).is_err());
    owner.join().unwrap();
    let (broker, _peer) = f.connect();
    drop(broker);
}

fn accept_video(listener: &UnixListener) -> UnixStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match listener.accept() {
            Ok((peer, _)) => return peer,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(2))
            }
            Err(e) => panic!("Owned video accept failed: {e}"),
        }
    }
}

#[allow(dead_code)]
#[path = "../crates/guest-agent/src/video.rs"]
mod guest_policy;
struct PolicyEncoder;
impl guest_policy::Encoder for PolicyEncoder {
    fn available(&self) -> bool {
        true
    }
    fn authorize(&mut self, _: &WindowTarget) -> std::io::Result<()> {
        Ok(())
    }
    fn start(
        &mut self,
        _: &freelane_agent_protocol::video_control::Parameters,
    ) -> std::io::Result<()> {
        Ok(())
    }
    fn running(&mut self) -> std::io::Result<bool> {
        Ok(true)
    }
    fn stop(&mut self) -> std::io::Result<()> {
        Ok(())
    }
    fn keyframe(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[test]
fn successive_host_sessions_satisfy_production_guest_retirement_policy() {
    use freelane::video::LiveSession;
    use freelane_agent_protocol::{
        video_control::{Operation, Reply},
        Action, Outcome,
    };
    use std::sync::{atomic::Ordering, Arc};
    let f = Fixture::new();
    let control = UnixListener::bind(f.dir.join("agent.sock")).unwrap();
    let video = f.listener.try_clone().unwrap();
    let owner = thread::spawn(move || {
        let mut attempts = Vec::new();
        for _ in 0..2 {
            let mut parameters = None;
            while parameters.is_none() {
                serve_video_control(&control, |action| match action {
                    Action::Ping => video_info(),
                    Action::Video {
                        operation: Operation::Start { parameters: p },
                    } => {
                        let peer = accept_video(&video);
                        parameters = Some((p.clone(), peer));
                        Outcome::Video {
                            report: Reply::Started { parameters: p },
                        }
                    }
                    _ => panic!("Unexpected startup"),
                });
            }
            let (p, _peer) = parameters.unwrap();
            serve_video_control(&control, |action| {
                assert_eq!(
                    action,
                    Action::Video {
                        operation: Operation::Stop { binding: p.binding }
                    }
                );
                Outcome::Video {
                    report: Reply::Stopped { binding: p.binding },
                }
            });
            attempts.push(p);
        }
        attempts
    });
    for _ in 0..2 {
        let live = LiveSession::start(&f.config, target(), Arc::new(|| {})).unwrap();
        let done = live.retirement();
        drop(live);
        wait_until(|| done.load(Ordering::Acquire));
    }
    let mut guest = guest_policy::Session::new(PolicyEncoder);
    for parameters in owner.join().unwrap() {
        let binding = parameters.binding;
        guest
            .action(&Operation::Start { parameters })
            .expect("Fresh host session must satisfy real guest restart policy");
        guest.action(&Operation::Stop { binding }).unwrap();
    }
}
#[test]
fn silent_open_serial_socket_expires_after_established_stream() {
    use freelane::video::LiveSession;
    use freelane_agent_protocol::{
        video_control::{Operation, Reply},
        Action, Outcome,
    };
    use std::sync::Arc;
    let f = Fixture::new();
    let control = UnixListener::bind(f.dir.join("agent.sock")).unwrap();
    let video = f.listener.try_clone().unwrap();
    let owner = thread::spawn(move || {
        let mut active = None;
        while active.is_none() {
            serve_video_control(&control, |action| match action {
                Action::Ping => video_info(),
                Action::Video {
                    operation: Operation::Start { parameters },
                } => {
                    let mut peer = accept_video(&video);
                    let mut cfg = config();
                    cfg.binding = parameters.binding;
                    write_record(&mut peer, &cfg).unwrap();
                    let mut image = frame(1, true);
                    image.binding = parameters.binding;
                    write_record(&mut peer, &image).unwrap();
                    active = Some((parameters.binding, peer));
                    Outcome::Video {
                        report: Reply::Started { parameters },
                    }
                }
                _ => panic!("Unexpected operation"),
            });
        }
        let (binding, _peer) = active.unwrap();
        serve_video_control(&control, |action| {
            assert_eq!(
                action,
                Action::Video {
                    operation: Operation::Stop { binding }
                }
            );
            Outcome::Video {
                report: Reply::Stopped { binding },
            }
        });
    });
    let live = LiveSession::start(&f.config, target(), Arc::new(|| {})).unwrap();
    wait_until(|| live.configuration().is_some());
    assert!(!live.ended());
    thread::sleep(Duration::from_millis(4200));
    let expired = live.ended();
    drop(live);
    owner.join().unwrap();
    assert!(
        expired,
        "A silent but open QEMU transport must not retain frozen video forever"
    );
}
#[test]
fn unchanged_content_heartbeats_keep_an_open_stream_healthy() {
    let f = Fixture::new();
    let (broker, mut peer) = f.connect();
    write_record(&mut peer, &config()).unwrap();
    for sequence in 1..=6 {
        // Same encoded pixels; only wire sequence/time advance, like idle heartbeat.
        write_record(&mut peer, &frame(sequence, true)).unwrap();
        wait_until(|| broker.pop().is_some());
        thread::sleep(Duration::from_millis(750));
        assert!(!broker.silent_for(Duration::from_secs(4)));
    }
}
