//! Real Windows captured packets through the production session/broker/viewer.
//! The owned Unix peer emulates serial/control transport, not a real guest VM.
#[cfg(not(all(target_os = "macos", feature = "native-video")))]
fn main() {
    eprintln!("Run on macOS with --features native-video");
    std::process::exit(1);
}
#[cfg(all(target_os = "macos", feature = "native-video"))]
#[path = "../src/native_video.rs"]
mod native_video;
#[cfg(all(target_os = "macos", feature = "native-video"))]
#[allow(dead_code)]
#[path = "../src/viewer_video.rs"]
mod viewer_video;
#[cfg(all(target_os = "macos", feature = "native-video"))]
mod owned {
    use super::viewer_video::ViewerVideo;
    use eframe::egui;
    use freelane::{qemu, video::LiveSession};
    use freelane_agent_protocol::{
        video::{read_record, write_record, Binding, Record},
        video_control::{Operation, Reply},
        Action, AgentInfo, Feature, Outcome, Request, Response, WindowTarget,
    };
    use std::{
        fs,
        io::{self, BufReader, Read},
        os::unix::{
            fs::PermissionsExt,
            net::{UnixListener, UnixStream},
        },
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            mpsc, Arc, Mutex,
        },
        thread,
        time::{Duration, Instant},
    };
    fn target() -> WindowTarget {
        WindowTarget {
            handle: 1,
            pid: 2,
            created: 3,
            incarnation: 4,
        }
    }
    struct Stream {
        peer: UnixStream,
        binding: Binding,
        index: usize,
        sequence: u64,
        next: Instant,
    }
    impl Stream {
        fn picture(&mut self, pictures: &[Record]) -> io::Result<()> {
            let mut record = pictures[self.index].clone();
            self.sequence += 1;
            record.sequence = self.sequence;
            record.pts_us = self.sequence * 33333;
            record.binding = self.binding;
            write_record(&mut self.peer, &record)?;
            self.index += 1;
            self.next = Instant::now() + Duration::from_millis(33);
            Ok(())
        }
    }
    fn peer(
        control: UnixListener,
        video: UnixListener,
        records: Vec<Record>,
        quit: Arc<AtomicBool>,
        stopped: Arc<AtomicBool>,
        sent: Arc<AtomicU64>,
    ) -> io::Result<()> {
        control.set_nonblocking(true)?;
        video.set_nonblocking(true)?;
        let mut active: Option<Stream> = None;
        let origin = Instant::now();
        let mut recoveries = 0;
        while !quit.load(Ordering::Acquire) && origin.elapsed() < Duration::from_secs(30) {
            match control.accept() {
                Ok((mut connection, _)) => {
                    connection.set_read_timeout(Some(Duration::from_secs(2)))?;
                    connection.set_write_timeout(Some(Duration::from_secs(2)))?;
                    let request: Request = freelane_agent_protocol::read_frame(
                        &mut BufReader::new(connection.try_clone()?),
                    )?;
                    let outcome = match request.action {
                        Action::Ping => Outcome::AgentInfo {
                            info: AgentInfo {
                                agent_version: "owned-native-transport-fixture".into(),
                                os: "windows".into(),
                                arch: "arm64".into(),
                                protocols: vec![1, 2, 3, 9, 14],
                                features: vec![Feature {
                                    name: "window_video".into(),
                                    protocol: 14,
                                }],
                                max_frame: 65536,
                            },
                        },
                        Action::Video {
                            operation: Operation::Start { parameters },
                        } => {
                            if active.is_some() || parameters.target != target() {
                                return Err(io::Error::other("Unexpected fixture Start"));
                            }
                            let peer = video.accept()?.0;
                            peer.set_write_timeout(Some(Duration::from_millis(500)))?;
                            let mut stream = Stream {
                                peer,
                                binding: parameters.binding,
                                index: 0,
                                sequence: 0,
                                next: Instant::now(),
                            };
                            let mut config = records[0].clone();
                            config.binding = parameters.binding;
                            write_record(&mut stream.peer, &config)?;
                            stream.picture(&records[1..])?;
                            sent.fetch_add(1, Ordering::Relaxed);
                            active = Some(stream);
                            Outcome::Video {
                                report: Reply::Started { parameters },
                            }
                        }
                        Action::Video {
                            operation: Operation::Keyframe { binding },
                        } => {
                            let stream = active
                                .as_mut()
                                .ok_or_else(|| io::Error::other("No owned stream"))?;
                            if stream.binding != binding || recoveries >= 4 {
                                return Err(io::Error::other("Invalid/excess fixture recovery"));
                            }
                            recoveries += 1;
                            stream.index = 0;
                            Outcome::Video {
                                report: Reply::KeyframeRequested { binding },
                            }
                        }
                        Action::Video {
                            operation: Operation::Stop { binding },
                        } => {
                            if active
                                .as_ref()
                                .is_none_or(|stream| stream.binding != binding)
                            {
                                return Err(io::Error::other("Wrong fixture Stop"));
                            }
                            active = None;
                            stopped.store(true, Ordering::Release);
                            Outcome::Video {
                                report: Reply::Stopped { binding },
                            }
                        }
                        _ => return Err(io::Error::other("Unexpected owned fixture operation")),
                    };
                    freelane_agent_protocol::write_frame(
                        &mut connection,
                        &Response {
                            version: request.version,
                            id: request.id,
                            outcome,
                        },
                    )?;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
            if let Some(stream) = active.as_mut() {
                if stream.index < records.len() - 1 && Instant::now() >= stream.next {
                    stream.picture(&records[1..])?;
                    sent.fetch_add(1, Ordering::Relaxed);
                }
            }
            thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    }
    type Report = Arc<Mutex<Option<Result<serde_json::Value, String>>>>;
    struct App {
        video: ViewerVideo,
        config: PathBuf,
        start: Option<mpsc::Receiver<Result<LiveSession, String>>>,
        started: bool,
        origin: Instant,
        result: Report,
    }
    impl eframe::App for App {
        fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
            if self.result.lock().unwrap().is_some() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            self.video.maintain(Some(target()), false);
            if !self.started {
                self.started = true;
                let chosen = self.video.begin().unwrap();
                let config = self.config.clone();
                let wake = ui.ctx().clone();
                let (tx, rx) = mpsc::channel();
                self.start = Some(rx);
                thread::spawn(move || {
                    let notify = wake.clone();
                    let result = LiveSession::start(
                        &config,
                        chosen,
                        Arc::new(move || notify.request_repaint()),
                    )
                    .map_err(|e| e.to_string());
                    let _ = tx.send(result);
                    wake.request_repaint();
                });
            }
            if let Some(rx) = self.start.as_ref() {
                if let Ok(result) = rx.try_recv() {
                    self.video.accept(&target(), result);
                    self.start = None;
                }
            }
            self.video.prepare(frame);
            let available = ui.available_size();
            let (width, height) = self.video.dimensions().unwrap_or((320, 180));
            let scale = (available.x / width as f32).min(available.y / height as f32);
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(width as f32 * scale, height as f32 * scale),
                egui::Sense::click_and_drag(),
            );
            self.video.render(ui.ctx(), rect);
            let stats = self.video.diagnostics();
            let outcome = if let Some(stats) = stats.filter(|s| {
                s.decoded >= 30 && s.presented > 0 && s.hardware && s.error == 0 && s.rendering
            }) {
                Some(Ok(
                    serde_json::json!({"decoded":stats.decoded,"presented":stats.presented,"hardware":stats.hardware,"serial_socket_transport":true,"normal_viewer_video_state":true,"real_guest_vm":false,"onscreen_pixels_verified":false}),
                ))
            } else if self.origin.elapsed() > Duration::from_secs(20)
                || (!self.video.active() && self.start.is_none())
            {
                Some(Err(format!(
                    "Live viewer fixture failed: {}",
                    self.video.status()
                )))
            } else {
                None
            };
            if let Some(outcome) = outcome {
                *self.result.lock().unwrap() = Some(outcome);
                self.video.invalidate();
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ui.ctx().request_repaint_after(Duration::from_millis(10));
        }
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        use std::os::fd::AsRawFd;
        let path = std::env::args()
            .nth(1)
            .ok_or("Expected owned captured-packets.flv")?;
        let mut data = Vec::new();
        fs::File::open(path)?
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut data)?;
        if data.len() > 4 * 1024 * 1024 {
            return Err("Oversized fixture".into());
        }
        let mut input = io::Cursor::new(data);
        let mut records = Vec::new();
        while let Some(record) = read_record(&mut input)? {
            records.push(record);
        }
        if records.len() != 31 {
            return Err("Expected configuration plus thirty Windows pictures".into());
        }
        let temp = tempfile::tempdir()?;
        let config = temp.path().join("vm.toml");
        fs::write(&config, "")?;
        let runtime = qemu::runtime_dir(&config)?;
        fs::create_dir(&runtime)?;
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(runtime.join("lock"))?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(io::Error::last_os_error().into());
        }
        let control = UnixListener::bind(runtime.join("agent.sock"))?;
        let video = UnixListener::bind(runtime.join("video.sock"))?;
        let quit = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let sent = Arc::new(AtomicU64::new(0));
        let (done, finished, count) = (quit.clone(), stopped.clone(), sent.clone());
        let owner = thread::spawn(move || peer(control, video, records, done, finished, count));
        let result = Arc::new(Mutex::new(None));
        let output = result.clone();
        let ran = eframe::run_native(
            "FreeLane owned live transport fixture",
            eframe::NativeOptions::default(),
            Box::new(move |_| {
                Ok(Box::new(App {
                    video: ViewerVideo::default(),
                    config,
                    start: None,
                    started: false,
                    origin: Instant::now(),
                    result,
                }))
            }),
        );
        let deadline = Instant::now() + Duration::from_secs(7);
        while !stopped.load(Ordering::Acquire) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        quit.store(true, Ordering::Release);
        let server = owner.join().map_err(|_| "Owned peer panicked")?;
        fs::remove_dir_all(runtime)?;
        ran?;
        server?;
        if !stopped.load(Ordering::Acquire) {
            return Err("Closing viewer did not stop its own stream".into());
        }
        let mut report = output
            .lock()
            .unwrap()
            .take()
            .ok_or("Closed before verification")?
            .map_err(io::Error::other)?;
        report["encoded_fixture_frames_sent"] = sent.load(Ordering::Relaxed).into();
        report["stop_acknowledged"] = true.into();
        println!("{report}");
        Ok(())
    }
}
#[cfg(all(target_os = "macos", feature = "native-video"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    owned::run()
}
