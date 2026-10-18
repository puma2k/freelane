use freelane_agent_protocol::{
    video::Binding,
    video_control::{Operation, Parameters, Reply},
    WindowTarget,
};
use freelane_guest_agent::video::{Encoder, Session};
use std::{
    io,
    process::{Child, Command},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};
#[derive(Default)]
struct Signals {
    starts: AtomicUsize,
    stops: AtomicUsize,
    keys: AtomicUsize,
    stale: AtomicBool,
    fail_start: AtomicBool,
    fail_stop: AtomicBool,
    die: AtomicBool,
    stale_on_start: AtomicBool,
}
struct Worker {
    child: Option<Child>,
    signals: Arc<Signals>,
}
impl Encoder for Worker {
    fn available(&self) -> bool {
        true
    }
    fn authorize(&mut self, _: &WindowTarget) -> io::Result<()> {
        if self.signals.stale.load(Ordering::SeqCst) {
            Err(io::ErrorKind::NotFound.into())
        } else {
            Ok(())
        }
    }
    fn start(&mut self, _: &Parameters) -> io::Result<()> {
        self.signals.starts.fetch_add(1, Ordering::SeqCst);
        self.child = Some(worker_child()?);
        if self.signals.stale_on_start.swap(false, Ordering::SeqCst) {
            self.signals.stale.store(true, Ordering::SeqCst);
        }
        if self.signals.fail_start.swap(false, Ordering::SeqCst) {
            Err(io::Error::other("startup failed after spawn"))
        } else {
            Ok(())
        }
    }
    fn running(&mut self) -> io::Result<bool> {
        if self.signals.die.swap(false, Ordering::SeqCst) {
            if let Some(child) = self.child.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        self.child
            .as_mut()
            .map(|c| c.try_wait().map(|s| s.is_none()))
            .unwrap_or(Ok(false))
    }
    fn stop(&mut self) -> io::Result<()> {
        if self.signals.fail_stop.load(Ordering::SeqCst) {
            return Err(io::Error::other("stop uncertain"));
        }
        self.signals.stops.fetch_add(1, Ordering::SeqCst);
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            child.wait()?;
        }
        Ok(())
    }
    fn keyframe(&mut self) -> io::Result<()> {
        self.signals.keys.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
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
        width: 1280,
        height: 720,
        fps: 30,
    }
}
fn session() -> (Session<Worker>, Arc<Signals>) {
    let signals = Arc::new(Signals::default());
    (
        Session::new(Worker {
            child: None,
            signals: signals.clone(),
        }),
        signals,
    )
}
#[test]
fn idempotent_start_busy_target_and_exact_owner_commands() {
    let (mut s, signals) = session();
    let p = parameters();
    let start = Operation::Start {
        parameters: p.clone(),
    };
    assert!(matches!(s.action(&start).unwrap(), Reply::Started { .. }));
    s.action(&start).unwrap();
    assert_eq!(signals.starts.load(Ordering::SeqCst), 1);
    let mut other = p.clone();
    other.target.incarnation += 1;
    assert!(s.action(&Operation::Start { parameters: other }).is_err());
    let mut wrong = p.binding;
    wrong.generation += 1;
    assert!(s.action(&Operation::Stop { binding: wrong }).is_err());
    assert!(s.action(&Operation::Keyframe { binding: wrong }).is_err());
    assert_eq!(signals.stops.load(Ordering::SeqCst), 0);
    s.action(&Operation::Keyframe { binding: p.binding })
        .unwrap();
    assert_eq!(signals.keys.load(Ordering::SeqCst), 1);
    s.action(&Operation::Stop { binding: p.binding }).unwrap();
    s.action(&Operation::Stop { binding: p.binding }).unwrap();
    assert_eq!(signals.stops.load(Ordering::SeqCst), 1);
    assert!(s.action(&start).is_err());
}
#[test]
fn startup_failure_cleans_owned_child_and_uncertain_cleanup_stays_quarantined() {
    let (mut s, signals) = session();
    let p = parameters();
    signals.fail_start.store(true, Ordering::SeqCst);
    signals.fail_stop.store(true, Ordering::SeqCst);
    let start = Operation::Start {
        parameters: p.clone(),
    };
    assert!(s.action(&start).is_err());
    assert!(s.action(&start).is_err());
    assert_eq!(signals.starts.load(Ordering::SeqCst), 1);
    signals.fail_stop.store(false, Ordering::SeqCst);
    s.action(&Operation::Stop { binding: p.binding }).unwrap();
    let mut next = p;
    next.binding.nonce = [8; 16];
    next.binding.generation += 1;
    s.action(&Operation::Start { parameters: next }).unwrap();
    assert_eq!(signals.starts.load(Ordering::SeqCst), 2);
    drop(s);
    assert_eq!(signals.stops.load(Ordering::SeqCst), 2);
}
#[test]
fn stale_target_revalidation_stops_encoder_without_starting_another() {
    let (mut s, signals) = session();
    let start = Operation::Start {
        parameters: parameters(),
    };
    s.action(&start).unwrap();
    signals.stale.store(true, Ordering::SeqCst);
    assert!(s.action(&start).is_err());
    assert_eq!(signals.starts.load(Ordering::SeqCst), 1);
    assert_eq!(signals.stops.load(Ordering::SeqCst), 1);
}
#[test]
fn stop_and_drop_preserve_unrelated_process() {
    let (mut s, signals) = session();
    let mut unrelated = Worker {
        child: Some(worker_child().unwrap()),
        signals: Arc::new(Signals::default()),
    };
    let p = parameters();
    s.action(&Operation::Start {
        parameters: p.clone(),
    })
    .unwrap();
    s.action(&Operation::Stop { binding: p.binding }).unwrap();
    drop(s);
    assert!(unrelated.running().unwrap());
    assert_eq!(signals.stops.load(Ordering::SeqCst), 1);
}

fn worker_child() -> io::Result<Child> {
    Command::new(std::env::current_exe()?)
        .args(["--exact", "owned_worker_process", "--nocapture"])
        .env("FREELANE_TEST_ENCODER_WORKER", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
}
#[test]
fn owned_worker_process() {
    if std::env::var_os("FREELANE_TEST_ENCODER_WORKER").is_some() {
        loop {
            std::thread::park_timeout(std::time::Duration::from_secs(30));
        }
    }
}

struct Adapter {
    session: Session<Worker>,
}
impl freelane_guest_agent::Launcher for Adapter {
    fn launch(&mut self, _: &str, _: &[String]) -> io::Result<u32> {
        panic!("video control must not launch applications")
    }
    fn window_identity_support(&self) -> bool {
        true
    }
    fn video_support(&self) -> bool {
        self.session.available()
    }
    fn video_action(&mut self, op: &Operation) -> io::Result<Reply> {
        self.session.action(op)
    }
}
#[test]
fn serialized_control_reconnect_preserves_encoder_through_transfer_wrapper() {
    use freelane_agent_protocol::{Action, Outcome, Request, Response};
    let (session, signals) = session();
    let mut launcher =
        freelane_guest_agent::TransferLauncher::new(Adapter { session }, None).unwrap();
    let info = freelane_guest_agent::handle(
        Request {
            version: 3,
            id: 1,
            action: Action::Ping,
        },
        &mut launcher,
    );
    let Outcome::AgentInfo { info } = info.outcome else {
        panic!("no capabilities")
    };
    assert!(info
        .features
        .iter()
        .any(|f| f.name == "window_video" && f.protocol == 14));
    info.validate().unwrap();
    for _ in 0..2 {
        let request = Request {
            version: 14,
            id: 2,
            action: Action::Video {
                operation: Operation::Start {
                    parameters: parameters(),
                },
            },
        };
        let mut bytes = vec![];
        freelane_agent_protocol::write_frame(&mut bytes, &request).unwrap();
        let mut reply = vec![];
        freelane_guest_agent::serve(&mut std::io::Cursor::new(bytes), &mut reply, &mut launcher)
            .unwrap();
        let response: Response =
            freelane_agent_protocol::read_frame(&mut std::io::Cursor::new(reply)).unwrap();
        assert!(matches!(
            response.outcome,
            Outcome::Video {
                report: Reply::Started { .. }
            }
        ));
        launcher.disconnect();
    }
    assert_eq!(signals.starts.load(Ordering::SeqCst), 1);
    assert_eq!(signals.stops.load(Ordering::SeqCst), 0);
    drop(launcher);
    assert_eq!(signals.stops.load(Ordering::SeqCst), 1);
}
#[test]
fn unregistered_backend_does_not_advertise_video_or_dispatch_to_launch() {
    use freelane_agent_protocol::{Action, Outcome, Request};
    struct Old;
    impl freelane_guest_agent::Launcher for Old {
        fn launch(&mut self, _: &str, _: &[String]) -> io::Result<u32> {
            panic!("unrelated launch")
        }
    }
    let mut old = Old;
    let info = freelane_guest_agent::handle(
        Request {
            version: 3,
            id: 1,
            action: Action::Ping,
        },
        &mut old,
    );
    let Outcome::AgentInfo { info } = info.outcome else {
        panic!("no capabilities")
    };
    assert!(!info.protocols.contains(&14));
    assert!(!info.features.iter().any(|f| f.name == "window_video"));
    let reply = freelane_guest_agent::handle(
        Request {
            version: 14,
            id: 2,
            action: Action::Video {
                operation: Operation::Start {
                    parameters: parameters(),
                },
            },
        },
        &mut old,
    );
    assert_eq!(reply.version, 14);
    assert!(matches!(reply.outcome, Outcome::Failed { .. }));
}
#[test]
fn backend_cannot_emit_a_reply_for_another_binding() {
    use freelane_agent_protocol::{Action, Outcome, Request};
    struct Wrong;
    impl freelane_guest_agent::Launcher for Wrong {
        fn launch(&mut self, _: &str, _: &[String]) -> io::Result<u32> {
            unreachable!()
        }
        fn video_action(&mut self, _: &Operation) -> io::Result<Reply> {
            let mut binding = parameters().binding;
            binding.generation += 1;
            Ok(Reply::Stopped { binding })
        }
    }
    let reply = freelane_guest_agent::handle(
        Request {
            version: 14,
            id: 1,
            action: Action::Video {
                operation: Operation::Stop {
                    binding: parameters().binding,
                },
            },
        },
        &mut Wrong,
    );
    assert!(matches!(reply.outcome, Outcome::Failed { .. }));
}

#[test]
fn ended_worker_does_not_block_a_fresh_owner_or_replay_old_start() {
    let (mut s, signals) = session();
    let p = parameters();
    s.action(&Operation::Start {
        parameters: p.clone(),
    })
    .unwrap();
    signals.die.store(true, Ordering::SeqCst);
    let mut next = p.clone();
    next.binding.nonce = [9; 16];
    next.binding.generation += 1;
    s.action(&Operation::Start { parameters: next }).unwrap();
    assert_eq!(signals.starts.load(Ordering::SeqCst), 2);
    assert_eq!(signals.stops.load(Ordering::SeqCst), 1);
}

#[test]
fn startup_must_not_acknowledge_a_target_replaced_during_setup() {
    let (mut s, signals) = session();
    signals.stale_on_start.store(true, Ordering::SeqCst);
    assert!(s
        .action(&Operation::Start {
            parameters: parameters()
        })
        .is_err());
    assert_eq!(signals.stops.load(Ordering::SeqCst), 1);
}
#[test]
fn startup_must_not_acknowledge_an_already_ended_worker() {
    let (mut s, signals) = session();
    signals.die.store(true, Ordering::SeqCst);
    assert!(s
        .action(&Operation::Start {
            parameters: parameters()
        })
        .is_err());
    assert_eq!(signals.stops.load(Ordering::SeqCst), 1);
}
