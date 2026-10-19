//! Execute only in the guest's interactive user session. Never install as a privileged host service.
pub mod catalog;
mod documents;
pub mod export;
pub mod mounts;
pub mod settings;
pub mod transfer;
pub mod video;
pub mod video_process;
use freelane_agent_protocol::{Action, Outcome, Request, Response, VERSION};
use std::io::{self, BufRead, Write};
#[cfg(any(windows, target_os = "linux"))]
mod clipboard;
#[cfg(windows)]
mod windows;
pub trait Launcher {
    fn video_support(&self) -> bool {
        false
    }
    fn video_action(
        &mut self,
        _: &freelane_agent_protocol::video_control::Operation,
    ) -> io::Result<freelane_agent_protocol::video_control::Reply> {
        Err(io::ErrorKind::Unsupported.into())
    }

    fn transfer_folders_support(&self) -> bool {
        false
    }
    fn transfer_folders(
        &mut self,
        _: bool,
    ) -> io::Result<freelane_agent_protocol::transfer_folders::Report> {
        Err(io::ErrorKind::Unsupported.into())
    }

    fn export_catalog_support(&self) -> bool {
        false
    }
    fn export_catalog(&mut self) -> io::Result<freelane_agent_protocol::export_catalog::Page> {
        Err(io::ErrorKind::Unsupported.into())
    }

    fn export_support(&self) -> bool {
        false
    }
    fn export_action(
        &mut self,
        _: &freelane_agent_protocol::export::Operation,
    ) -> io::Result<Outcome> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn window_input_support(&self) -> bool {
        false
    }
    fn window_identity_support(&self) -> bool {
        false
    }
    fn catalog_support(&self) -> bool {
        false
    }
    fn catalog(&mut self, _: u16) -> io::Result<freelane_agent_protocol::AppPage> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn share_status_support(&self) -> bool {
        false
    }
    fn share_status(&mut self) -> io::Result<Vec<freelane_agent_protocol::SharedMount>> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn open_document(&mut self, _: &std::path::Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn transfer_support(&self) -> bool {
        false
    }
    fn transfer_action(
        &mut self,
        _: &freelane_agent_protocol::TransferAction,
    ) -> io::Result<Outcome> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn clipboard_support(&self) -> bool {
        false
    }
    fn clipboard_action(&mut self, _action: &Action) -> io::Result<Outcome> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Clipboard unavailable",
        ))
    }
    fn window_action(&mut self, _action: &Action) -> io::Result<Outcome> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows interactive window extension unavailable",
        ))
    }
    fn window_support(&self) -> bool {
        false
    }
    fn launch(&mut self, executable: &str, arguments: &[String]) -> io::Result<u32>;
}
pub fn handle(request: Request, launcher: &mut impl Launcher) -> Response {
    let outcome =
        match request.validate() {
            Err(error) => Outcome::Failed {
                message: error.to_string(),
            },
            Ok(()) => match request.action {
                Action::Ping if request.version == freelane_agent_protocol::CAPABILITY_VERSION => {
                    let mut features = vec![freelane_agent_protocol::Feature {
                        name: "launch".into(),
                        protocol: VERSION,
                    }];
                    if launcher.video_support() && launcher.window_identity_support() {
                        features.push(freelane_agent_protocol::Feature {
                            name: "window_video".into(),
                            protocol: freelane_agent_protocol::video_control::VERSION,
                        });
                    }
                    if launcher.window_input_support() {
                        features.push(freelane_agent_protocol::Feature {
                            name: "window_input".into(),
                            protocol: freelane_agent_protocol::window_input::VERSION,
                        });
                    }
                    if launcher.window_identity_support() {
                        features.push(freelane_agent_protocol::Feature {
                            name: "window_identity".into(),
                            protocol: freelane_agent_protocol::WINDOW_IDENTITY_VERSION,
                        });
                    }
                    if launcher.catalog_support() {
                        features.push(freelane_agent_protocol::Feature {
                            name: "app_catalog".into(),
                            protocol: freelane_agent_protocol::CATALOG_VERSION,
                        });
                    }
                    if launcher.share_status_support() {
                        features.push(freelane_agent_protocol::Feature {
                            name: "share_status".into(),
                            protocol: freelane_agent_protocol::SHARE_STATUS_VERSION,
                        });
                    }
                    if launcher.transfer_folders_support() {
                        features.push(freelane_agent_protocol::Feature {
                            name: "file_folders".into(),
                            protocol: freelane_agent_protocol::transfer_folders::VERSION,
                        });
                    }
                    if launcher.export_catalog_support() {
                        features.push(freelane_agent_protocol::Feature {
                            name: "file_export_catalog".into(),
                            protocol: freelane_agent_protocol::export_catalog::VERSION,
                        });
                    }
                    if launcher.export_support() {
                        features.push(freelane_agent_protocol::Feature {
                            name: "file_export".into(),
                            protocol: freelane_agent_protocol::export::VERSION,
                        });
                    }
                    if launcher.transfer_support() {
                        features.push(freelane_agent_protocol::Feature {
                            name: "file_transfer".into(),
                            protocol: freelane_agent_protocol::TRANSFER_VERSION,
                        });
                    }
                    if launcher.window_support() {
                        features.push(freelane_agent_protocol::Feature {
                            name: "window_snapshots".into(),
                            protocol: freelane_agent_protocol::WINDOW_VERSION,
                        });
                    }
                    if launcher.clipboard_support() {
                        features.push(freelane_agent_protocol::Feature {
                            name: "clipboard_sync".into(),
                            protocol: freelane_agent_protocol::CLIPBOARD_SYNC_VERSION,
                        });
                        features.push(freelane_agent_protocol::Feature {
                            name: "text_clipboard".into(),
                            protocol: freelane_agent_protocol::CLIPBOARD_VERSION,
                        });
                    }
                    Outcome::AgentInfo {
                        info: freelane_agent_protocol::AgentInfo {
                            agent_version: env!("CARGO_PKG_VERSION").into(),
                            os: std::env::consts::OS.into(),
                            arch: std::env::consts::ARCH.into(),
                            protocols: vec![
                                VERSION,
                                freelane_agent_protocol::WINDOW_VERSION,
                                freelane_agent_protocol::CAPABILITY_VERSION,
                            ]
                            .into_iter()
                            .chain(
                                (launcher.video_support() && launcher.window_identity_support())
                                    .then_some(freelane_agent_protocol::video_control::VERSION),
                            )
                            .chain(if launcher.clipboard_support() {
                                Some(freelane_agent_protocol::CLIPBOARD_VERSION)
                            } else {
                                None
                            })
                            .chain(
                                launcher
                                    .clipboard_support()
                                    .then_some(freelane_agent_protocol::CLIPBOARD_SYNC_VERSION),
                            )
                            .chain(
                                launcher
                                    .transfer_support()
                                    .then_some(freelane_agent_protocol::TRANSFER_VERSION),
                            )
                            .chain(
                                launcher
                                    .share_status_support()
                                    .then_some(freelane_agent_protocol::SHARE_STATUS_VERSION),
                            )
                            .chain(
                                launcher
                                    .catalog_support()
                                    .then_some(freelane_agent_protocol::CATALOG_VERSION),
                            )
                            .chain(
                                launcher
                                    .window_identity_support()
                                    .then_some(freelane_agent_protocol::WINDOW_IDENTITY_VERSION),
                            )
                            .chain(
                                launcher
                                    .window_input_support()
                                    .then_some(freelane_agent_protocol::window_input::VERSION),
                            )
                            .chain(
                                launcher
                                    .export_support()
                                    .then_some(freelane_agent_protocol::export::VERSION),
                            )
                            .chain(
                                launcher
                                    .export_catalog_support()
                                    .then_some(freelane_agent_protocol::export_catalog::VERSION),
                            )
                            .chain(
                                launcher
                                    .transfer_folders_support()
                                    .then_some(freelane_agent_protocol::transfer_folders::VERSION),
                            )
                            .collect(),
                            features,
                            max_frame: freelane_agent_protocol::MAX_FRAME as u32,
                        },
                    }
                }
                Action::Ping if request.version == freelane_agent_protocol::WINDOW_VERSION => {
                    Outcome::Capabilities {
                        window_snapshots: launcher.window_support(),
                    }
                }
                Action::Ping => Outcome::Pong {
                    os: std::env::consts::OS.into(),
                    arch: std::env::consts::ARCH.into(),
                },
                Action::Video { ref operation } => launcher
                    .video_action(operation)
                    .and_then(|report| {
                        report.validate_for(operation)?;
                        Ok(Outcome::Video { report })
                    })
                    .unwrap_or_else(|error| Outcome::Failed {
                        message: error.to_string(),
                    }),
                Action::TransferFolders { reconnect } => launcher
                    .transfer_folders(reconnect)
                    .map(|report| Outcome::TransferFolders { report })
                    .unwrap_or_else(|e| Outcome::Failed {
                        message: if e.kind() == io::ErrorKind::WouldBlock {
                            "Finish or cancel the file transfer before reconnecting guest folders"
                                .into()
                        } else {
                            "Guest file folders unavailable or refused".into()
                        },
                    }),
                Action::ExportCatalog => launcher
                    .export_catalog()
                    .map(|page| Outcome::ExportCatalog { page })
                    .unwrap_or_else(|_| Outcome::Failed {
                        message: "Guest outbox listing unavailable or refused".into(),
                    }),
                Action::Export { ref operation } => launcher
                    .export_action(operation)
                    .unwrap_or_else(|_| Outcome::Failed {
                        message: "File export unavailable or refused".into(),
                    }),
                Action::Transfer { ref operation } => launcher
                    .transfer_action(operation)
                    .unwrap_or_else(|_| Outcome::Failed {
                        message: "File transfer unavailable or refused".into(),
                    }),
                Action::AppCatalog { cursor } => launcher
                    .catalog(cursor)
                    .map(|page| Outcome::AppCatalog { page })
                    .unwrap_or_else(|_| Outcome::Failed {
                        message: "Application catalog unavailable".into(),
                    }),
                Action::ShareStatus => launcher
                    .share_status()
                    .map(|mounts| Outcome::ShareMounts { mounts })
                    .unwrap_or_else(|_| Outcome::Failed {
                        message: "Guest mount status unavailable".into(),
                    }),
                Action::Launch {
                    executable,
                    arguments,
                } => match launcher.launch(&executable, &arguments) {
                    Ok(pid) => Outcome::Launched { pid },
                    Err(error) => Outcome::Failed {
                        message: format!("Launch failed ({:?})", error.kind()),
                    },
                },
                ref action @ (Action::ClipboardGet
                | Action::ClipboardSet { .. }
                | Action::ClipboardSyncGet
                | Action::ClipboardSyncSet { .. }) => launcher
                    .clipboard_action(action)
                    .unwrap_or_else(|_| Outcome::Failed {
                        message: "Text clipboard transfer unavailable or refused".into(),
                    }),
                ref action => launcher
                    .window_action(action)
                    .unwrap_or_else(|e| Outcome::Failed {
                        message: e.to_string(),
                    }),
            },
        };
    Response {
        version: if [
            freelane_agent_protocol::WINDOW_VERSION,
            freelane_agent_protocol::CAPABILITY_VERSION,
            freelane_agent_protocol::CLIPBOARD_VERSION,
            freelane_agent_protocol::TRANSFER_VERSION,
            freelane_agent_protocol::CLIPBOARD_SYNC_VERSION,
            freelane_agent_protocol::SHARE_STATUS_VERSION,
            freelane_agent_protocol::CATALOG_VERSION,
            freelane_agent_protocol::WINDOW_IDENTITY_VERSION,
            freelane_agent_protocol::window_input::VERSION,
            freelane_agent_protocol::export::VERSION,
            freelane_agent_protocol::export_catalog::VERSION,
            freelane_agent_protocol::transfer_folders::VERSION,
            freelane_agent_protocol::video_control::VERSION,
        ]
        .contains(&request.version)
        {
            request.version
        } else {
            VERSION
        },
        id: request.id,
        outcome,
    }
}
pub fn serve(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    launcher: &mut impl Launcher,
) -> io::Result<()> {
    loop {
        let request = match freelane_agent_protocol::read_frame(reader) {
            Ok(request) => request,
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error),
        };
        freelane_agent_protocol::write_frame(writer, &handle(request, launcher))?;
    }
}
#[cfg(windows)]
pub struct WindowsLauncher;
#[cfg(windows)]
impl Launcher for WindowsLauncher {
    fn video_support(&self) -> bool {
        windows::video_available()
    }
    fn video_action(
        &mut self,
        operation: &freelane_agent_protocol::video_control::Operation,
    ) -> io::Result<freelane_agent_protocol::video_control::Reply> {
        windows::video_operation(operation)
    }
    fn window_input_support(&self) -> bool {
        true
    }
    fn window_identity_support(&self) -> bool {
        true
    }
    fn catalog_support(&self) -> bool {
        true
    }
    fn catalog(&mut self, cursor: u16) -> io::Result<freelane_agent_protocol::AppPage> {
        catalog::page(catalog::entries()?, cursor)
    }
    fn open_document(&mut self, p: &std::path::Path) -> io::Result<()> {
        documents::open(p)
    }
    fn clipboard_support(&self) -> bool {
        clipboard::available()
    }
    fn clipboard_action(&mut self, action: &Action) -> io::Result<Outcome> {
        clipboard::handle(action)
    }
    fn window_support(&self) -> bool {
        true
    }
    fn window_action(&mut self, action: &Action) -> io::Result<Outcome> {
        windows::handle(action)
    }
    fn launch(&mut self, executable: &str, arguments: &[String]) -> io::Result<u32> {
        if !std::path::Path::new(executable).is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Executable must be an absolute Windows path",
            ));
        }
        if !std::path::Path::new(executable)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Use an executable .exe path; batch files require an explicit interpreter",
            ));
        }
        // Command uses CreateProcess with literal argument quoting; no cmd.exe/PowerShell intermediary.
        let child = std::process::Command::new(executable)
            .args(arguments)
            // A child must not consume RPC input or inject console text into --stdio replies.
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let id = child.id();
        drop(child);
        Ok(id)
    }
}
#[cfg(target_os = "linux")]
pub struct LinuxLauncher;
#[cfg(target_os = "linux")]
impl Launcher for LinuxLauncher {
    fn catalog_support(&self) -> bool {
        true
    }
    fn catalog(&mut self, cursor: u16) -> io::Result<freelane_agent_protocol::AppPage> {
        catalog::page(catalog::entries()?, cursor)
    }
    fn share_status_support(&self) -> bool {
        true
    }
    fn share_status(&mut self) -> io::Result<Vec<freelane_agent_protocol::SharedMount>> {
        mounts::current()
    }
    fn open_document(&mut self, p: &std::path::Path) -> io::Result<()> {
        documents::open(p)
    }
    fn clipboard_support(&self) -> bool {
        clipboard::available()
    }
    fn clipboard_action(&mut self, action: &Action) -> io::Result<Outcome> {
        clipboard::handle(action)
    }
    fn launch(&mut self, executable: &str, arguments: &[String]) -> io::Result<u32> {
        let path = std::path::Path::new(executable);
        if !path.is_absolute() || !path.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Use an absolute Linux executable file path",
            ));
        }
        let mut child = std::process::Command::new(path)
            .args(arguments)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let pid = child.id();
        // Reap asynchronously: RPC must not wait for a long-lived GUI application.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake(Vec<(String, Vec<String>)>);
    impl Launcher for Fake {
        fn launch(&mut self, exe: &str, args: &[String]) -> io::Result<u32> {
            self.0.push((exe.into(), args.into()));
            Ok(42)
        }
    }
    #[test]
    fn window_capability_handshake_preserves_v1_ping_and_rejects_v1_mutations() {
        let mut fake = Fake(vec![]);
        let ping = |version| Request {
            version,
            id: 3,
            action: Action::Ping,
        };
        assert!(matches!(
            handle(ping(VERSION), &mut fake).outcome,
            Outcome::Pong { .. }
        ));
        let response = handle(ping(freelane_agent_protocol::WINDOW_VERSION), &mut fake);
        assert_eq!(response.version, freelane_agent_protocol::WINDOW_VERSION);
        assert_eq!(
            response.outcome,
            Outcome::Capabilities {
                window_snapshots: false
            }
        );
        assert!(matches!(
            handle(
                Request {
                    version: VERSION,
                    id: 4,
                    action: Action::Windows { pid: 42 }
                },
                &mut fake
            )
            .outcome,
            Outcome::Failed { .. }
        ));
        assert!(fake.0.is_empty());
    }
    #[test]
    fn invalid_request_never_reaches_process_launcher() {
        let mut fake = Fake(vec![]);
        let reply = handle(
            Request {
                version: 99,
                id: 3,
                action: Action::Launch {
                    executable: "x".into(),
                    arguments: vec![],
                },
            },
            &mut fake,
        );
        assert!(matches!(reply.outcome, Outcome::Failed { .. }));
        assert!(fake.0.is_empty());
    }
    #[test]
    fn launch_preserves_arguments_and_returns_correlated_pid() {
        let mut fake = Fake(vec![]);
        let reply = handle(
            Request {
                version: VERSION,
                id: 17,
                action: Action::Launch {
                    executable: "C:\\Program Files\\App\\app.exe".into(),
                    arguments: vec!["a & b".into()],
                },
            },
            &mut fake,
        );
        assert_eq!(reply.id, 17);
        assert_eq!(reply.outcome, Outcome::Launched { pid: 42 });
        assert_eq!(fake.0[0].1, vec!["a & b"]);
    }
    #[test]
    fn server_handles_multiple_framed_requests() {
        let mut input = Vec::new();
        for id in [1, 2] {
            freelane_agent_protocol::write_frame(
                &mut input,
                &Request {
                    version: VERSION,
                    id,
                    action: Action::Ping,
                },
            )
            .unwrap();
        }
        let mut output = Vec::new();
        serve(&mut input.as_slice(), &mut output, &mut Fake(vec![])).unwrap();
        let mut reader = output.as_slice();
        for id in [1, 2] {
            let reply: Response = freelane_agent_protocol::read_frame(&mut reader).unwrap();
            assert_eq!(reply.id, id);
            assert!(matches!(reply.outcome, Outcome::Pong { .. }));
        }
    }
}

#[cfg(test)]
mod discovery_tests {
    use super::*;
    struct NoLaunch;
    impl Launcher for NoLaunch {
        fn launch(&mut self, _: &str, _: &[String]) -> io::Result<u32> {
            panic!("Discovery must not launch anything")
        }
    }
    #[test]
    fn v3_discovery_reports_versions_and_actual_features_without_mutating() {
        let mut launcher = NoLaunch;
        let response = handle(
            Request {
                version: 3,
                id: 91,
                action: Action::Ping,
            },
            &mut launcher,
        );
        let json = serde_json::to_value(response).unwrap();
        assert_eq!(json["version"], 3);
        assert_eq!(json["outcome"]["status"], "agent_info");
        assert_eq!(
            json["outcome"]["info"]["protocols"],
            serde_json::json!([1, 2, 3])
        );
        assert_eq!(
            json["outcome"]["info"]["features"],
            serde_json::json!([{"name":"launch","protocol":1}])
        );
        assert_eq!(json["outcome"]["info"]["max_frame"], 65536);
        let response = handle(
            Request {
                version: 3,
                id: 92,
                action: Action::Launch {
                    executable: "must-not-run".into(),
                    arguments: vec![],
                },
            },
            &mut launcher,
        );
        assert!(matches!(response.outcome, Outcome::Failed { .. }));
    }
}

/// Explicitly configured inbox; transfer state belongs to this agent session.
pub struct TransferLauncher<L> {
    inner: L,
    outbox: export::Outbox,
    inbox_path: Option<std::path::PathBuf>,
    outbox_path: Option<std::path::PathBuf>,
    root: Option<cap_std::fs::Dir>,
    completed: Option<(String, String, u64, String)>,
    active: Option<(String, transfer::Transfer, std::time::Instant)>,
}
impl<L> TransferLauncher<L> {
    pub fn new(inner: L, root: Option<&std::path::Path>) -> io::Result<Self> {
        Ok(Self {
            inner,
            outbox: export::Outbox::default(),
            inbox_path: root.map(std::path::Path::to_path_buf),
            outbox_path: None,
            root: root
                .map(|p| cap_std::fs::Dir::open_ambient_dir(p, cap_std::ambient_authority()))
                .transpose()?,
            active: None,
            completed: None,
        })
    }
    /// Start independently usable directions; missing roots never prevent other agent RPC.
    pub fn configured(inner: L, selected: &settings::Settings) -> Self {
        Self {
            inner,
            root: selected.inbox.as_ref().and_then(|p| {
                cap_std::fs::Dir::open_ambient_dir(p, cap_std::ambient_authority()).ok()
            }),
            outbox: export::Outbox::new(selected.outbox.as_deref()).unwrap_or_default(),
            inbox_path: selected.inbox.clone(),
            outbox_path: selected.outbox.clone(),
            active: None,
            completed: None,
        }
    }
    pub fn with_export_root(mut self, root: Option<&std::path::Path>) -> io::Result<Self> {
        self.outbox = export::Outbox::new(root)?;
        self.outbox_path = root.map(std::path::Path::to_path_buf);
        Ok(self)
    }
    fn expire_inbox(&mut self) {
        if self
            .active
            .as_ref()
            .is_some_and(|(_, _, time)| time.elapsed() > std::time::Duration::from_secs(300))
        {
            self.active = None;
        }
    }
    fn folder_report(&self) -> freelane_agent_protocol::transfer_folders::Report {
        use freelane_agent_protocol::transfer_folders::{Report, State};
        let state = |configured: bool, opened: bool| {
            if !configured {
                State::Disabled
            } else if opened {
                State::Ready
            } else {
                State::Unavailable
            }
        };
        Report {
            inbox: state(self.inbox_path.is_some(), self.root.is_some()),
            outbox: state(self.outbox_path.is_some(), self.outbox.enabled()),
            busy: self
                .active
                .as_ref()
                .is_some_and(|(_, _, time)| time.elapsed() <= std::time::Duration::from_secs(300))
                || self.outbox.busy(),
        }
    }
    pub fn disconnect(&mut self) {
        self.outbox.disconnect();
        self.active = None;
        self.completed = None;
    }
}
impl<L: Launcher> Launcher for TransferLauncher<L> {
    fn video_support(&self) -> bool {
        self.inner.video_support()
    }
    fn video_action(
        &mut self,
        operation: &freelane_agent_protocol::video_control::Operation,
    ) -> io::Result<freelane_agent_protocol::video_control::Reply> {
        self.inner.video_action(operation)
    }

    fn transfer_folders_support(&self) -> bool {
        true
    }
    fn transfer_folders(
        &mut self,
        reconnect: bool,
    ) -> io::Result<freelane_agent_protocol::transfer_folders::Report> {
        if reconnect {
            self.expire_inbox();
            self.outbox.expire();
            if self.active.is_some() || self.outbox.busy() {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            if self.root.is_none() {
                if let Some(path) = &self.inbox_path {
                    self.root =
                        cap_std::fs::Dir::open_ambient_dir(path, cap_std::ambient_authority()).ok();
                }
            }
            if !self.outbox.enabled() {
                if let Some(path) = &self.outbox_path {
                    if let Ok(outbox) = export::Outbox::new(Some(path)) {
                        self.outbox = outbox;
                    }
                }
            }
        }
        Ok(self.folder_report())
    }

    fn export_catalog_support(&self) -> bool {
        self.outbox.enabled()
    }
    fn export_catalog(&mut self) -> io::Result<freelane_agent_protocol::export_catalog::Page> {
        self.outbox.catalog()
    }

    fn export_support(&self) -> bool {
        self.outbox.enabled()
    }
    fn export_action(
        &mut self,
        operation: &freelane_agent_protocol::export::Operation,
    ) -> io::Result<Outcome> {
        self.outbox.action(operation)
    }
    fn catalog_support(&self) -> bool {
        self.inner.catalog_support()
    }
    fn catalog(&mut self, cursor: u16) -> io::Result<freelane_agent_protocol::AppPage> {
        self.inner.catalog(cursor)
    }
    fn share_status_support(&self) -> bool {
        self.inner.share_status_support()
    }
    fn share_status(&mut self) -> io::Result<Vec<freelane_agent_protocol::SharedMount>> {
        self.inner.share_status()
    }
    fn launch(&mut self, e: &str, a: &[String]) -> io::Result<u32> {
        self.inner.launch(e, a)
    }
    fn window_input_support(&self) -> bool {
        self.inner.window_input_support()
    }
    fn window_identity_support(&self) -> bool {
        self.inner.window_identity_support()
    }
    fn window_support(&self) -> bool {
        self.inner.window_support()
    }
    fn window_action(&mut self, a: &Action) -> io::Result<Outcome> {
        self.inner.window_action(a)
    }
    fn clipboard_support(&self) -> bool {
        self.inner.clipboard_support()
    }
    fn clipboard_action(&mut self, a: &Action) -> io::Result<Outcome> {
        self.inner.clipboard_action(a)
    }
    fn transfer_support(&self) -> bool {
        self.root.is_some()
    }
    fn transfer_action(
        &mut self,
        a: &freelane_agent_protocol::TransferAction,
    ) -> io::Result<Outcome> {
        use freelane_agent_protocol::TransferAction as T;
        a.validate()?;
        self.expire_inbox();
        let root = self.root.as_ref().ok_or(io::ErrorKind::Unsupported)?;
        if let T::Open { token } = a {
            let (id, name, size, hash) = self.completed.as_ref().ok_or(io::ErrorKind::NotFound)?;
            if id != token {
                return Err(io::ErrorKind::NotFound.into());
            }
            let file = root.open(name)?.into_std();
            let path = documents::verified_path(file, name, *size, hash)?;
            self.inner.open_document(&path)?;
            return Ok(Outcome::DocumentOpened);
        }
        if let T::Begin { name, size, sha256 } = a {
            if self.active.is_some() {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let mut random = [0; 16];
            getrandom::fill(&mut random).map_err(|_| io::ErrorKind::Other)?;
            let token: String = random.iter().map(|b| format!("{b:02x}")).collect();
            self.active = Some((
                token.clone(),
                transfer::Transfer::begin_dir(root.try_clone()?, name, *size, sha256)?,
                std::time::Instant::now(),
            ));
            return Ok(Outcome::TransferReady { token });
        }
        let token = match a {
            T::Chunk { token, .. } | T::Finish { token } | T::Cancel { token } => token,
            _ => unreachable!(),
        };
        if !self.active.as_ref().is_some_and(|(id, _, _)| id == token) {
            return Err(io::ErrorKind::NotFound.into());
        }
        match a {
            T::Chunk {
                offset, data_hex, ..
            } => {
                let data: Vec<u8> = data_hex
                    .as_bytes()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
                    .collect();
                let (_, t, time) = self.active.as_mut().unwrap();
                t.write(*offset, &data)?;
                *time = std::time::Instant::now();
                Ok(Outcome::TransferProgress {
                    received: t.received(),
                })
            }
            T::Finish { .. } => {
                let (token, t, _) = self.active.take().unwrap();
                let (name, size, hash) = t.receipt();
                t.finish()?;
                self.completed = Some((token, name, size, hash));
                Ok(Outcome::TransferComplete)
            }
            T::Cancel { .. } => {
                self.active = None;
                Ok(Outcome::TransferCanceled)
            }
            _ => unreachable!(),
        }
    }
}
