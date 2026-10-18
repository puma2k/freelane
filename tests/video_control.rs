use freelane::{agent, video};
use freelane_agent_protocol::{
    video::Binding,
    video_control::{Operation, Parameters, Reply},
    Action, AgentInfo, Feature, Outcome, Request, Response, WindowTarget,
};
use std::{
    fs,
    io::BufReader,
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::PathBuf,
    thread,
};
struct Fixture {
    _temp: tempfile::TempDir,
    config: PathBuf,
    dir: PathBuf,
    control: UnixListener,
    _port: Option<UnixListener>,
}
impl Fixture {
    fn new(port: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("vm.toml");
        fs::write(&config, "").unwrap();
        let dir = freelane::qemu::runtime_dir(&config).unwrap();
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let control = UnixListener::bind(dir.join("agent.sock")).unwrap();
        let video = if port {
            Some(UnixListener::bind(dir.join("video.sock")).unwrap())
        } else {
            None
        };
        Self {
            _temp: temp,
            config,
            dir,
            control,
            _port: video,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.dir).unwrap();
    }
}
fn params() -> Parameters {
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
        width: 1280,
        height: 720,
        fps: 30,
    }
}
fn reply(listener: &UnixListener, outcome: impl FnOnce(&Request) -> Outcome) {
    let (mut peer, _) = listener.accept().unwrap();
    let request: Request =
        freelane_agent_protocol::read_frame(&mut BufReader::new(peer.try_clone().unwrap()))
            .unwrap();
    let result = outcome(&request);
    freelane_agent_protocol::write_frame(
        &mut peer,
        &Response {
            version: request.version,
            id: request.id,
            outcome: result,
        },
    )
    .unwrap();
}
fn info(support: bool) -> Outcome {
    Outcome::AgentInfo {
        info: AgentInfo {
            agent_version: "fixture".into(),
            os: "windows".into(),
            arch: "arm64".into(),
            protocols: if support {
                vec![1, 2, 3, 9, 14]
            } else {
                vec![1, 2, 3, 9]
            },
            features: if support {
                vec![Feature {
                    name: "window_video".into(),
                    protocol: 14,
                }]
            } else {
                vec![]
            },
            max_frame: 65536,
        },
    }
}
#[test]
fn start_negotiates_before_one_exact_mutation() {
    let f = Fixture::new(true);
    let listener = f.control.try_clone().unwrap();
    let worker = thread::spawn(move || {
        reply(&listener, |r| {
            assert_eq!(r.version, 3);
            assert_eq!(r.action, Action::Ping);
            info(true)
        });
        reply(&listener, |r| {
            assert_eq!(r.version, 14);
            let Action::Video {
                operation: Operation::Start { parameters },
            } = &r.action
            else {
                panic!("wrong operation")
            };
            Outcome::Video {
                report: Reply::Started {
                    parameters: parameters.clone(),
                },
            }
        });
    });
    let p = params();
    let got = agent::video_request(
        &f.config,
        &Operation::Start {
            parameters: p.clone(),
        },
    )
    .unwrap();
    assert_eq!(got, Reply::Started { parameters: p });
    worker.join().unwrap();
}
#[test]
fn unsupported_or_missing_port_sends_no_start_and_keeps_vm_running() {
    for (supported, port) in [(false, true), (true, false)] {
        let f = Fixture::new(port);
        let listener = f.control.try_clone().unwrap();
        let worker = thread::spawn(move || reply(&listener, |_| info(supported)));
        assert!(agent::video_request(
            &f.config,
            &Operation::Start {
                parameters: params()
            }
        )
        .is_err());
        worker.join().unwrap();
        f.control.set_nonblocking(true).unwrap();
        assert!(matches!(f.control.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
        assert!(f.dir.join("agent.sock").exists());
    }
}
#[test]
fn stop_does_not_require_rediscovering_capability_or_a_video_port() {
    let f = Fixture::new(false);
    let listener = f.control.try_clone().unwrap();
    let worker = thread::spawn(move || {
        reply(&listener, |r| {
            let Action::Video {
                operation: Operation::Stop { binding },
            } = &r.action
            else {
                panic!("stop must not probe/restart")
            };
            Outcome::Video {
                report: Reply::Stopped { binding: *binding },
            }
        })
    });
    agent::video_request(
        &f.config,
        &Operation::Stop {
            binding: params().binding,
        },
    )
    .unwrap();
    worker.join().unwrap();
}
#[test]
fn uncorrelated_or_wrong_kind_video_success_is_rejected() {
    for bad in 0..4 {
        let f = Fixture::new(true);
        let listener = f.control.try_clone().unwrap();
        let worker = thread::spawn(move || {
            reply(&listener, |_| {
                let mut p = params();
                match bad {
                    0 => p.binding.generation += 1,
                    1 => p.target.incarnation += 1,
                    2 => p.width = 640,
                    _ => {
                        return Outcome::Video {
                            report: Reply::Stopped { binding: p.binding },
                        }
                    }
                }
                Outcome::Video {
                    report: Reply::Started { parameters: p },
                }
            })
        });
        let result = agent::request_socket_version(
            &f.dir.join("agent.sock"),
            Action::Video {
                operation: Operation::Start {
                    parameters: params(),
                },
            },
            14,
        );
        assert!(result.is_err());
        worker.join().unwrap();
    }
}
#[test]
fn invalid_start_is_rejected_before_even_a_probe() {
    let f = Fixture::new(true);
    let mut p = params();
    p.target.incarnation = 0;
    assert!(agent::video_request(&f.config, &Operation::Start { parameters: p }).is_err());
    f.control.set_nonblocking(true).unwrap();
    assert!(matches!(f.control.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}
#[test]
fn fresh_nonce_is_unpredictable_nonzero_and_preserves_stream_generation() {
    let a = video::fresh_binding(1, 2).unwrap();
    let b = video::fresh_binding(1, 2).unwrap();
    assert_ne!(a.nonce, b.nonce);
    assert_ne!(a.nonce, [0; 16]);
    assert_eq!((a.stream, a.generation), (1, 2));
    assert!(video::fresh_binding(0, 2).is_err());
    assert!(video::fresh_binding(1, 0).is_err());
}
