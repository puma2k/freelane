mod app_frame;
mod app_input;
mod app_ui;
mod catalog_ui;
mod clipboard_ui;
mod file_picker;
mod folder_ui;
mod i18n;
mod interface;
mod interface_assets;
mod interface_library;
mod interface_preview;
mod mobile_ui;
mod native_video;
mod recovery_ui;
mod resource_ui;
mod settings_io;
mod setup;
mod sharing_ui;
mod transfer_ui;
mod viewer_schedule;
mod viewer_video;
mod wizard;
use eframe::egui;
use freelane::{agent, qemu, Config, GuestOs};
use freelane_agent_protocol::{Action, Outcome};
use i18n::Language;
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
struct VmInfo {
    config: Config,
    original: String,
    path: PathBuf,
    name: String,
    cpus: u16,
    memory: u64,
    guest_agent: bool,
    guest_os: GuestOs,
    acceleration: &'static str,
}
enum Event {
    TransferFolders(
        PathBuf,
        Result<freelane_agent_protocol::transfer_folders::Report, String>,
    ),
    OutboxCatalog(
        PathBuf,
        Result<freelane_agent_protocol::export_catalog::Page, String>,
    ),
    Resources(PathBuf, serde_json::Value),
    CheckpointPicked(Option<String>),
    CheckpointInfo(String, Result<freelane::checkpoint::Info, String>),
    CheckpointCreated(PathBuf),
    CheckpointRestored(PathBuf, Vec<freelane::library::Entry>),
    AppCatalog(PathBuf, Vec<freelane_agent_protocol::AppEntry>),
    SharedPicked(Option<String>),
    SharedReport(String, qemu::ShareMode, freelane::sharing::Report),
    ClipboardAuto(
        clipboard_ui::Session,
        freelane_agent_protocol::clipboard_sync::SyncState,
        Arc<std::sync::atomic::AtomicBool>,
        Result<freelane_agent_protocol::clipboard_sync::Decision, String>,
    ),
    TransferDone(String),
    Clipboard(clipboard_ui::Session, String),
    ClipboardFailed(String),
    Loaded(VmInfo, Option<String>),
    Created(PathBuf),
    LibraryRepaired(Vec<freelane::library::Entry>),
    AgentCapabilities(Option<freelane::agent_capabilities::Report>, String),
    SettingsPreview(freelane::settings_bundle::Bundle),
    WizardPicked(wizard::Field, Option<String>),
    WizardTools(freelane::host_tools::Tools),
    WizardStorage(String, Result<freelane::storage::Destination, String>),
    MobileLoaded(PathBuf, freelane::mobile::Profile),
    MobileNotice(String),
    MobileFrame(egui::ColorImage),
    Saved(VmInfo, String, Option<String>),
    Status(String),
    Readiness(freelane::readiness::Report),
    Frame(egui::ColorImage, bool),
    Notice(String),
}
type WorkResult = Result<Event, String>;

#[derive(Default)]
struct Desktop {
    transfer_open_document: bool,
    transfer_export_name: String,
    transfer_folder_status: Option<(PathBuf, freelane_agent_protocol::transfer_folders::Report)>,
    transfer_catalog: Option<(PathBuf, freelane_agent_protocol::export_catalog::Page)>,
    transfer_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    transfer_progress: Option<mpsc::Receiver<(u64, u64)>>,
    transfer_status: Option<(u64, u64)>,
    wizard: wizard::Wizard,
    clipboard_auto_enabled: bool,
    clipboard_auto_paused: bool,
    clipboard_auto_checked: bool,
    clipboard_auto_permission: Option<Arc<std::sync::atomic::AtomicBool>>,
    clipboard_auto_policy: freelane_agent_protocol::clipboard_sync::SyncState,
    clipboard_auto_at: Option<Instant>,
    clipboard_enabled: bool,
    clipboard_permission: Option<Arc<std::sync::atomic::AtomicBool>>,
    clipboard_session: Option<clipboard_ui::Session>,
    setup_progress: Option<mpsc::Receiver<String>>,
    setup_status: Option<String>,
    setup_cancel: Option<setup::Cancel>,
    start_created: Option<PathBuf>,
    mobile_open: bool,
    mobile_path: String,
    mobile_profile: Option<(PathBuf, freelane::mobile::Profile)>,
    mobile_application: String,
    mobile_package: String,
    mobile_notice: String,
    mobile_texture: Option<egui::TextureHandle>,
    mobile_child: Option<Child>,
    mobile_errors: Option<Arc<Mutex<Vec<u8>>>>,
    creation_progress: Option<mpsc::Receiver<freelane::provision::Stage>>,
    creation_stage: Option<freelane::provision::Stage>,
    agent_status: String,
    agent_info: Option<freelane::agent_capabilities::Report>,
    language: Language,
    library: Vec<(PathBuf, String, GuestOs)>,
    library_search: String,
    library_notice: String,
    open_dialog: bool,
    view: interface::View,
    settings_tab: interface::SettingsTab,
    settings_open: bool,
    settings: Option<Config>,
    installer_path: String,
    shared_report: Option<freelane::sharing::Report>,
    shared_path: String,
    draft_path: String,
    resources: Option<serde_json::Value>,
    resource_capacity: Option<freelane::resource_presets::HostCapacity>,
    checkpoint_label: String,
    checkpoint_source: String,
    checkpoint_info: Option<freelane::checkpoint::Info>,
    app_catalog: Vec<freelane_agent_protocol::AppEntry>,
    app_catalog_search: String,
    app_name: String,
    app_profile_output: String,
    dock_output: String,
    app_path: String,
    app_arguments: String,
    vm: Option<VmInfo>,
    status: String,
    notice: String,
    pending: Option<Receiver<WorkResult>>,
    texture: Option<egui::TextureHandle>,
    child: Option<Child>,
    child_errors: Option<Arc<Mutex<Vec<u8>>>>,
    refresh_at: Option<Instant>,
    auto_display: bool,
    display_at: Option<Instant>,
}
impl Desktop {
    fn save_app_profile(&mut self, ctx: &egui::Context, open: bool) {
        let Some(vm) = self.vm.as_ref() else {
            return;
        };
        let profile = freelane::app_launcher::AppProfile {
            name: self.app_name.trim().into(),
            vm: vm.path.clone(),
            executable: self.app_path.trim().into(),
            arguments: if self.app_arguments.is_empty() {
                Vec::new()
            } else {
                self.app_arguments.lines().map(str::to_owned).collect()
            },
            boot_timeout_seconds: 600,
        };
        let output = PathBuf::from(self.app_profile_output.trim());
        let language = if self.language == Language::English {
            "en"
        } else {
            "vi"
        };
        self.work(ctx, move || {
            profile.save(&output).map_err(|e| e.to_string())?;
            if open {
                let executable = std::env::current_exe().map_err(|e| e.to_string())?;
                Command::new(executable)
                    .args([
                        "--app",
                        output.to_str().ok_or("Profile path must be UTF-8")?,
                        "--lang",
                        language,
                    ])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|e| e.to_string())?;
            }
            Ok(Event::Notice(format!(
                "Đã lưu {}. Cửa sổ riêng dùng agent Windows mới; chưa phải video trực tiếp.",
                output.display()
            )))
        });
    }
    fn open_saved_app(&mut self, ctx: &egui::Context) {
        let path = PathBuf::from(self.app_profile_output.trim());
        let language = if self.language == Language::English {
            "en"
        } else {
            "vi"
        };
        self.work(ctx, move || {
            let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
            freelane::app_launcher::AppProfile::load(&path).map_err(|e| e.to_string())?;
            Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
                .arg("--app")
                .arg(&path)
                .args(["--lang", language])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| e.to_string())?;
            Ok(Event::Notice(
                "Đã mở viewer riêng; xem trạng thái trong cửa sổ ứng dụng.".into(),
            ))
        });
    }
    fn export_app_dock(&mut self, ctx: &egui::Context) {
        let source = PathBuf::from(self.app_profile_output.trim());
        let output = PathBuf::from(self.dock_output.trim());
        self.work(ctx, move || {
            let profile =
                freelane::app_launcher::AppProfile::load(&source).map_err(|e| e.to_string())?;
            let desktop = std::env::current_exe().map_err(|e| e.to_string())?;
            let cli = desktop
                .parent()
                .ok_or("Cannot locate CLI")?
                .join("freelane");
            freelane::app_launcher::export_dock(&profile, &output, &desktop, &cli)
                .map_err(|e| e.to_string())?;
            Ok(Event::Notice(format!(
                "Đã tạo {}. Kéo launcher này vào Dock.",
                output.display()
            )))
        });
    }
    fn work(&mut self, ctx: &egui::Context, task: impl FnOnce() -> WorkResult + Send + 'static) {
        if self.pending.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(task());
            ctx.request_repaint();
        });
    }
    fn probe_agent_capabilities(&mut self, ctx: &egui::Context) {
        let Some(vm) = &self.vm else {
            return;
        };
        let path = vm.path.clone();
        self.work(ctx,move || Ok(match freelane::agent_capabilities::inspect(&path) {
            Ok(info)=>Event::AgentCapabilities(Some(info),String::new()),
            Err(_)=>Event::AgentCapabilities(None,"Agent capabilities unavailable. Check the channel and guest agent installation.".into()),
        }));
    }
    fn repair_bookmark(&mut self, ctx: &egui::Context, old: PathBuf) {
        self.work(ctx, move || {
            let result: WorkResult = (|| {
                let Some(new) = file_picker::choose(false)? else {
                    return Ok(Event::Notice(String::new()));
                };
                let library = freelane::library::default_path().map_err(|e| e.to_string())?;
                freelane::library::relink(&library, &old, Path::new(&new))
                    .map_err(|e| e.to_string())?;
                freelane::library::read(&library)
                    .map(Event::LibraryRepaired)
                    .map_err(|e| e.to_string())
            })();
            Ok(result.unwrap_or_else(Event::Notice))
        });
    }
    fn open(&mut self, ctx: &egui::Context) {
        let path = PathBuf::from(self.draft_path.trim());
        self.work(ctx, move || {
            let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
            if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 1024 * 1024 {
                return Err("VM file exceeds 1 MiB".into());
            }
            let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let config = Config::parse(&text).map_err(|e| e.to_string())?;
            config
                .check_images(path.parent().unwrap_or(Path::new(".")))
                .map_err(|e| e.to_string())?;
            let acceleration = match config.runtime.as_ref() {
                Some(runtime) => match runtime.accelerator {
                    qemu::Accelerator::Hvf => "Hardware (HVF; requires Apple Silicon)",
                    qemu::Accelerator::Tcg => match runtime.tcg_threading {
                        None => "Software emulation (TCG; default threading)",
                        Some(qemu::TcgThreading::Single) => {
                            "Software emulation (TCG; single thread)"
                        }
                        Some(qemu::TcgThreading::Multi) => {
                            "Software emulation (TCG; multiple threads)"
                        }
                    },
                },
                None => "Runtime not configured",
            };
            let warning = freelane::library::default_path()
                .and_then(|file| {
                    freelane::library::record(
                        &file,
                        freelane::library::Entry {
                            path: path.clone(),
                            name: config.name.clone(),
                            guest_os: config.guest_os,
                        },
                    )
                })
                .err()
                .map(|e| e.to_string());
            Ok(Event::Loaded(
                VmInfo {
                    path,
                    config: config.clone(),
                    original: text,
                    name: config.name,
                    guest_os: config.guest_os,
                    cpus: config.cpus,
                    memory: config.memory_mib,
                    acceleration,
                    guest_agent: config
                        .runtime
                        .as_ref()
                        .is_some_and(|runtime| runtime.guest_agent),
                },
                warning,
            ))
        });
    }
    fn save_settings(&mut self, ctx: &egui::Context) {
        let (Some(vm), Some(mut config)) = (self.vm.clone(), self.settings.clone()) else {
            return;
        };
        config.installer = (!self.installer_path.trim().is_empty())
            .then(|| PathBuf::from(self.installer_path.trim()));
        if let Some(runtime) = config.runtime.as_mut() {
            runtime.shared_directory = (!self.shared_path.trim().is_empty())
                .then(|| PathBuf::from(self.shared_path.trim()));
        }
        self.work(ctx, move || {
            let backup =
                qemu::save_config(&vm.path, &vm.original, &config).map_err(|e| e.to_string())?;
            let mut updated = vm;
            updated.original = std::fs::read_to_string(&updated.path).map_err(|e| e.to_string())?;
            updated.name = config.name.clone();
            updated.cpus = config.cpus;
            updated.memory = config.memory_mib;
            updated.guest_agent = config.runtime.as_ref().is_some_and(|r| r.guest_agent);
            updated.acceleration = match config.runtime.as_ref().map(|r| r.accelerator) {
                Some(qemu::Accelerator::Hvf) => "Hardware (HVF; requires Apple Silicon)",
                Some(qemu::Accelerator::Tcg) => {
                    "Software emulation (TCG; see settings for threading)"
                }
                None => "Runtime not configured",
            };
            updated.config = config;
            let warning = freelane::library::default_path()
                .and_then(|file| {
                    freelane::library::record(
                        &file,
                        freelane::library::Entry {
                            path: updated.path.clone(),
                            name: updated.name.clone(),
                            guest_os: updated.guest_os,
                        },
                    )
                })
                .err()
                .map(|e| e.to_string());
            Ok(Event::Saved(
                updated,
                format!(
                    "Settings saved for next start. Backup: {}",
                    backup.display()
                ),
                warning,
            ))
        });
    }
    fn control(&mut self, ctx: &egui::Context, action: &'static str) {
        let Some(vm) = &self.vm else { return };
        let path = vm.path.clone();
        self.work(ctx, move || {
            if action == "status" {
                return freelane::readiness::inspect(&path).map(Event::Readiness).map_err(|e|e.to_string());
            }
            let reply = qemu::control(&path, action).map_err(|e| {
                if action == "status" {
                    format!("Status unavailable. If the VM is powered off, use Start; otherwise inspect its runtime log. Details: {e}")
                } else {
                    e.to_string()
                }
            })?;
            Ok(match action {
                "status" => Event::Status(reply["status"].as_str().unwrap_or("unknown").to_owned()),
                "shutdown" => {
                    Event::Notice("Guest shutdown requested. Refresh to check completion.".into())
                }
                "stop" => {
                    Event::Notice("Force stop requested. Refresh to check completion.".into())
                }
                "pause" => Event::Status("paused".into()),
                "resume" => Event::Status("running".into()),
                _ => Event::Notice("Request sent".into()),
            })
        });
    }
    fn capture(&mut self, ctx: &egui::Context, announce: bool) {
        let Some(vm) = &self.vm else { return };
        let path = vm.path.clone();
        self.work(ctx, move || {
            let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
            let frame = temp.path().join("frame.ppm");
            qemu::screenshot(&path, &frame).map_err(|e| e.to_string())?;
            if std::fs::metadata(&frame).map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
                return Err("Display frame exceeds 64 MiB".into());
            }
            let mut reader = image::ImageReader::open(frame).map_err(|e| e.to_string())?;
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(64 * 1024 * 1024);
            limits.max_image_width = Some(8192);
            limits.max_image_height = Some(8192);
            reader.limits(limits);
            let rgb = reader.decode().map_err(|e| e.to_string())?.into_rgb8();
            Ok(Event::Frame(
                egui::ColorImage::from_rgb(
                    [rgb.width() as usize, rgb.height() as usize],
                    rgb.as_raw(),
                ),
                announce,
            ))
        });
    }
    fn guest_request(&mut self, ctx: &egui::Context, launch: bool) {
        let Some(vm) = &self.vm else { return };
        let path = vm.path.clone();
        let action = if launch {
            Action::Launch {
                executable: self.app_path.trim().to_owned(),
                arguments: if self.app_arguments.is_empty() {
                    Vec::new()
                } else {
                    self.app_arguments.lines().map(str::to_owned).collect()
                },
            }
        } else {
            Action::Ping
        };
        self.work(ctx, move || {
            let notice = match agent::request(&path, action) {
                Ok(response) => match response.outcome {
                    Outcome::Pong { os, arch } => format!("Guest responded: {os} / {arch}"),
                    Outcome::Launched { pid } => format!("Guest reported process started (PID {pid}). Check its guest window."),
                    Outcome::Failed { message } => format!("Guest request failed: {message}"),
                    _ => "Unexpected guest launch/ping response".into(),
                },
                Err(error) => {
                    let followup = if launch { " If launching timed out, check the guest before trying again; execution outcome may be unknown." } else { "" };
                    format!("Guest request failed: {error}.{followup}")
                },
            };
            Ok(Event::Notice(notice))
        });
    }
    fn click(&mut self, ctx: &egui::Context, x: u16, y: u16) {
        let Some(vm) = &self.vm else { return };
        let path = vm.path.clone();
        self.work(ctx, move || {
            let notice = match qemu::click(&path, x, y) {
                Ok(()) => "Mouse click sent. Check the updated display.".into(),
                Err(error) => error.to_string(),
            };
            Ok(Event::Notice(notice))
        });
    }
    fn start(&mut self) {
        let Some(vm) = &self.vm else { return };
        let result = std::env::current_exe().map_err(|e| e.to_string()).and_then(|exe| {
            let cli = exe.parent().ok_or("Cannot locate FreeLane CLI")?.join("freelane");
            Command::new(cli).args(["start"]).arg(&vm.path)
                .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped())
                .spawn().map_err(|e| format!("Cannot start VM: {e}. Build the FreeLane CLI alongside the desktop binary."))
        });
        match result {
            Ok(mut child) => {
                let errors = Arc::new(Mutex::new(Vec::new()));
                if let Some(mut stderr) = child.stderr.take() {
                    let captured = Arc::clone(&errors);
                    // Drain continuously: a full stderr pipe must never stall the VM.
                    std::thread::spawn(move || {
                        let mut chunk = [0u8; 4096];
                        while let Ok(count) = stderr.read(&mut chunk) {
                            if count == 0 {
                                break;
                            }
                            if let Ok(mut bytes) = captured.lock() {
                                let keep = count.min((16 * 1024usize).saturating_sub(bytes.len()));
                                bytes.extend_from_slice(&chunk[..keep]);
                            }
                        }
                    });
                }
                self.child_errors = Some(errors);
                self.child = Some(child);
                self.status = "Starting; awaiting VM status".into();
                self.refresh_at = Some(Instant::now() + Duration::from_secs(2));
            }
            Err(error) => self.notice = error,
        }
    }
    fn receive(&mut self, ctx: &egui::Context) {
        if let Some(progress) = &self.setup_progress {
            while let Ok(message) = progress.try_recv() {
                self.setup_status = Some(message);
            }
        }
        if let Some(rx) = &self.transfer_progress {
            while let Ok(progress) = rx.try_recv() {
                self.transfer_status = Some(progress);
            }
        }
        self.poll_mobile(ctx);
        if let Some(receiver) = &self.creation_progress {
            while let Ok(stage) = receiver.try_recv() {
                self.creation_stage = Some(stage);
            }
        }
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    self.creation_progress = None;
                    self.creation_stage = None;
                    match result {
                        Ok(Event::CheckpointPicked(path)) => {
                            if let Some(path) = path {
                                self.checkpoint_source = path;
                                self.inspect_checkpoint(ctx);
                            }
                        }
                        Ok(Event::CheckpointInfo(source, info)) => {
                            if self.checkpoint_source == source {
                                match info {
                                    Ok(info) => {
                                        self.checkpoint_info = Some(info);
                                        self.notice.clear();
                                    }
                                    Err(error) => {
                                        self.checkpoint_info = None;
                                        self.notice = error;
                                    }
                                }
                            }
                        }
                        Ok(Event::CheckpointCreated(path)) => {
                            self.checkpoint_source = path.display().to_string();
                            self.notice = "Checkpoint saved".into();
                            self.inspect_checkpoint(ctx);
                        }
                        Ok(Event::CheckpointRestored(path, entries)) => {
                            self.library = entries
                                .into_iter()
                                .map(|e| (e.path, e.name, e.guest_os))
                                .collect();
                            self.notice = format!("Restored new VM: {}", path.display());
                        }
                        Ok(Event::Resources(path, value)) => {
                            if self.vm.as_ref().is_some_and(|vm| vm.path == path) {
                                self.resources = Some(value);
                            }
                        }
                        Ok(Event::TransferFolders(path, result)) => {
                            self.apply_folder_report(path, result)
                        }
                        Ok(Event::OutboxCatalog(path, result)) => {
                            self.apply_outbox_catalog(path, result)
                        }
                        Ok(Event::AppCatalog(path, apps)) => {
                            if self.vm.as_ref().is_some_and(|vm| vm.path == path) {
                                self.app_catalog = apps;
                                self.notice.clear();
                            }
                        }
                        Ok(Event::SharedPicked(path)) => {
                            if let Some(path) = path {
                                self.shared_path = path;
                                self.shared_report = None;
                            }
                        }
                        Ok(Event::SharedReport(selected, mode, report)) => {
                            if selected == self.shared_path
                                && self
                                    .settings
                                    .as_ref()
                                    .and_then(|c| c.runtime.as_ref())
                                    .is_some_and(|r| r.share_mode == mode)
                            {
                                self.shared_report = Some(report);
                            }
                        }
                        Ok(Event::TransferDone(message)) => {
                            self.transfer_cancel = None;
                            self.transfer_progress = None;
                            self.transfer_status = None;
                            self.notice = message;
                        }
                        Ok(Event::ClipboardAuto(session, policy, permission, result)) => {
                            self.clipboard_session = Some(session);
                            if self
                                .clipboard_auto_permission
                                .as_ref()
                                .is_some_and(|current| Arc::ptr_eq(current, &permission))
                                && permission.load(std::sync::atomic::Ordering::Relaxed)
                            {
                                self.clipboard_auto_policy = policy;
                                self.clipboard_auto_checked = true;
                                use freelane_agent_protocol::clipboard_sync::Decision;
                                match result {
                                    Ok(Decision::Conflict) => {
                                        self.clipboard_auto_paused = true;
                                        self.notice="Both clipboards changed. Choose Send or Copy to continue".into();
                                    }
                                    Ok(Decision::Restarted) | Err(_) => {
                                        self.stop_clipboard_auto();
                                        self.notice="Clipboard synchronization stopped. Enable it again after reconnecting".into();
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Ok(Event::ClipboardFailed(message)) => self.notice = message,
                        Ok(Event::Clipboard(session, message)) => {
                            self.clipboard_auto_policy = Default::default();
                            self.clipboard_auto_paused = false;
                            self.clipboard_session = Some(session);
                            self.notice = message;
                        }
                        Ok(Event::WizardStorage(path, result)) => {
                            self.wizard.storage_received(&path, result)
                        }
                        Ok(Event::WizardTools(tools)) => {
                            self.wizard.apply_tools(
                                tools.firmware.display().to_string(),
                                tools.variables.display().to_string(),
                                tools.image.display().to_string(),
                            );
                            self.notice = "VM components ready".into();
                        }
                        Ok(Event::WizardPicked(field, value)) => {
                            if let Some(value) = value {
                                self.wizard.picked(field, value);
                            }
                        }
                        Ok(Event::SettingsPreview(bundle)) => {
                            if let Some(target) = &self.settings {
                                match bundle.apply(target) {
                                    Ok(updated) => {
                                        self.shared_report = None;
                                        self.installer_path = updated
                                            .installer
                                            .as_ref()
                                            .map(|p| p.display().to_string())
                                            .unwrap_or_default();
                                        self.shared_path = updated
                                            .runtime
                                            .as_ref()
                                            .and_then(|r| r.shared_directory.as_ref())
                                            .map(|p| p.display().to_string())
                                            .unwrap_or_default();
                                        self.settings = Some(updated);
                                        self.notice =
                                            "Settings imported for review. Save changes to apply."
                                                .into();
                                    }
                                    Err(error) => self.notice = error.to_string(),
                                }
                            }
                        }
                        Ok(Event::LibraryRepaired(entries)) => {
                            self.library = entries
                                .into_iter()
                                .map(|entry| (entry.path, entry.name, entry.guest_os))
                                .collect();
                            self.library_notice.clear();
                            self.notice =
                                "VM bookmark reconnected. Open the VM to check its runtime.".into();
                        }
                        Ok(Event::Created(path)) => {
                            let cancelled = self.setup_cancel.as_ref().is_some_and(|cancel| {
                                cancel.load(std::sync::atomic::Ordering::Relaxed)
                            });
                            self.setup_cancel = None;
                            self.setup_progress = None;
                            self.setup_status = None;
                            self.start_created = (!cancelled).then_some(path.clone());
                            self.wizard.open = false;
                            self.draft_path = path.display().to_string();
                            self.open(ctx);
                        }
                        Ok(Event::MobileLoaded(path, profile)) => {
                            self.mobile_profile = Some((path, profile));
                            self.mobile_texture = None;
                            self.mobile_notice.clear();
                        }
                        Ok(Event::MobileNotice(notice)) => self.mobile_notice = notice,
                        Ok(Event::MobileFrame(frame)) => {
                            self.mobile_texture = Some(ctx.load_texture(
                                "mobile-frame",
                                frame,
                                egui::TextureOptions::LINEAR,
                            ))
                        }
                        Ok(Event::Loaded(vm, warning)) => {
                            self.transfer_folder_status = None;
                            self.transfer_catalog = None;
                            self.transfer_export_name.clear();
                            self.app_catalog.clear();
                            self.app_catalog_search.clear();
                            self.resources = None;
                            self.shared_report = None;
                            self.stop_clipboard_auto();
                            self.clipboard_enabled = false;
                            if let Some(permission) = &self.clipboard_permission {
                                permission.store(false, std::sync::atomic::Ordering::Relaxed);
                            }
                            self.clipboard_permission = None;
                            self.library_notice = warning.unwrap_or_default();
                            self.settings = Some(vm.config.clone());
                            self.installer_path = vm
                                .config
                                .installer
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_default();
                            self.shared_path = vm
                                .config
                                .runtime
                                .as_ref()
                                .and_then(|r| r.shared_directory.as_ref())
                                .map(|p| p.display().to_string())
                                .unwrap_or_default();
                            if let Some(entry) =
                                self.library.iter_mut().find(|entry| entry.0 == vm.path)
                            {
                                entry.1 = vm.name.clone();
                                entry.2 = vm.guest_os;
                            } else {
                                self.library
                                    .push((vm.path.clone(), vm.name.clone(), vm.guest_os));
                            }
                            self.open_dialog = false;
                            let start = self
                                .start_created
                                .take()
                                .is_some_and(|path| path == vm.path);
                            self.vm = Some(vm);
                            self.texture = None;
                            self.auto_display = false;
                            self.display_at = None;
                            self.notice.clear();
                            self.status = "Not checked".into();
                            self.agent_status = "not_checked".into();
                            self.agent_info = None;
                            if start {
                                self.view = interface::View::Display;
                                self.auto_display = true;
                                self.display_at = Some(Instant::now() + Duration::from_secs(5));
                                self.start();
                            } else {
                                self.control(ctx, "status");
                            }
                        }
                        Ok(Event::Saved(vm, notice, warning)) => {
                            self.library_notice = warning.unwrap_or_default();
                            if let Some(entry) =
                                self.library.iter_mut().find(|entry| entry.0 == vm.path)
                            {
                                entry.1 = vm.name.clone();
                                entry.2 = vm.guest_os;
                            }
                            self.vm = Some(vm);
                            self.notice = notice;
                            self.settings_open = false;
                            self.status = "Not checked".into();
                            self.agent_status = "not_checked".into();
                            self.agent_info = None;
                            self.texture = None;
                            self.auto_display = false;
                        }
                        Ok(Event::AgentCapabilities(info, notice)) => {
                            self.agent_status = if info.is_some() {
                                "connected"
                            } else if self.status == "running" {
                                "unavailable"
                            } else {
                                "not_checked"
                            }
                            .into();
                            self.agent_info = info;
                            self.notice = notice;
                        }
                        Ok(Event::Readiness(report)) => {
                            if report.agent != "connected" || report.backend != "running" {
                                self.agent_info = None;
                            }
                            self.status = report.backend;
                            self.agent_status = report.agent;
                            self.notice.clear();
                        }
                        Ok(Event::Status(status)) => {
                            self.status = status;
                            self.agent_status = "not_checked".into();
                            self.agent_info = None;
                            self.notice.clear();
                        }
                        Ok(Event::Frame(image, announce)) => {
                            self.texture = Some(ctx.load_texture(
                                "guest-frame",
                                image,
                                egui::TextureOptions::LINEAR,
                            ));
                            if announce {
                                self.notice = "Display snapshot updated".into();
                            }
                        }
                        Ok(Event::Notice(notice)) => self.notice = notice,
                        Err(error) => {
                            self.setup_cancel = None;
                            self.setup_progress = None;
                            self.setup_status = None;
                            self.start_created = None;
                            self.auto_display = false;
                            self.status = "Unknown".into();
                            self.agent_status = "not_checked".into();
                            self.agent_info = None;
                            self.notice = error;
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.auto_display = false;
                    self.pending = None;
                    self.notice = "VM worker disconnected".into();
                    self.agent_info = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(100))
                }
            }
        }
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(Some(exit)) => {
                    let bytes = self
                        .child_errors
                        .take()
                        .and_then(|errors| errors.lock().ok().map(|bytes| bytes.clone()))
                        .unwrap_or_default();
                    self.notice = format!(
                        "VM process ended ({exit}). {}",
                        String::from_utf8_lossy(&bytes)
                    );
                    self.child = None;
                    self.auto_display = false;
                    self.status = "Unknown".into();
                    self.agent_status = "not_checked".into();
                    self.agent_info = None;
                    self.refresh_at = None;
                }
                Ok(None) => ctx.request_repaint_after(Duration::from_millis(500)),
                Err(error) => self.notice = error.to_string(),
            }
        }
        if self.pending.is_none() && self.refresh_at.is_some_and(|at| Instant::now() >= at) {
            self.refresh_at = None;
            self.control(ctx, "status");
        }
        if self.auto_display && self.vm.is_some() {
            let now = Instant::now();
            if self.pending.is_none() && self.display_at.is_none_or(|at| now >= at) {
                self.display_at = Some(now + Duration::from_secs(5));
                self.capture(ctx, false);
            }
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }
}
impl eframe::App for Desktop {
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.stop_clipboard_auto();
        if let Some(cancel) = &self.transfer_cancel {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(permission) = &self.clipboard_permission {
            permission.store(false, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(cancel) = &self.setup_cancel {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.receive(&ctx);
        self.poll_clipboard_auto(&ctx);
        self.draw(ui, &ctx);
        self.mobile_ui(&ctx);
        self.wizard_ui(&ctx);
    }
}
fn main() -> eframe::Result {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut app_language = Language::Vietnamese;
    if let Some(index) = args.iter().position(|a| a == "--lang") {
        app_language = match args.get(index + 1).map(String::as_str) {
            Some("en") => Language::English,
            Some("vi") => Language::Vietnamese,
            _ => {
                eprintln!("Use --lang vi or --lang en");
                std::process::exit(2);
            }
        };
        args.drain(index..index + 2);
    }
    if let [flag, path, attach, pid] = args.as_slice() {
        if flag == "--app" && attach == "--pid" {
            let pid = match pid.parse::<u32>() {
                Ok(p) if p > 0 => p,
                _ => {
                    eprintln!("PID must be positive");
                    std::process::exit(2);
                }
            };
            return app_ui::run(Path::new(path), Some(pid), app_language);
        }
    }
    if let [flag, path] = args.as_slice() {
        if flag == "--app" {
            return app_ui::run(Path::new(path), None, app_language);
        }
    }
    if args.is_empty() {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(contents) = exe.parent().and_then(|p| p.parent()) {
                let profile = contents.join("Resources/app.toml");
                if profile.is_file() {
                    return app_ui::run(&profile, None, app_language);
                }
            }
        }
    }

    let mut paths = Vec::new();
    let mut mobile_path = String::new();
    let mut language = Language::Vietnamese;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--mobile" {
            mobile_path = arguments.next().unwrap_or_default();
        } else if argument == "--lang" {
            language = match arguments.next().as_deref() {
                Some("en") => Language::English,
                Some("vi") => Language::Vietnamese,
                _ => {
                    eprintln!("Use --lang vi or --lang en");
                    std::process::exit(2);
                }
            };
        } else {
            paths.push(argument);
        }
    }
    let path = paths.first().cloned().unwrap_or_default();
    let saved_library =
        freelane::library::default_path().and_then(|file| freelane::library::read(&file));
    let library_notice = saved_library
        .as_ref()
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    let mut library: Vec<(PathBuf, String, GuestOs)> = saved_library
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.path, e.name, e.guest_os))
        .collect();
    for entry in paths {
        if let Ok(path) = std::fs::canonicalize(&entry) {
            if std::fs::metadata(&path).is_ok_and(|m| m.len() <= 1024 * 1024) {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    if let Ok(config) = Config::parse(&text) {
                        if !library
                            .iter()
                            .any(|entry: &(PathBuf, String, GuestOs)| entry.0 == path)
                        {
                            library.push((path, config.name, config.guest_os));
                        }
                    }
                }
            }
        }
    }
    eframe::run_native(
        "FreeLane",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1200.0, 820.0])
                .with_min_inner_size([800.0, 600.0]),
            ..Default::default()
        },
        Box::new(move |creation| {
            interface::install_style(&creation.egui_ctx);
            Ok(Box::new(Desktop {
                draft_path: path,
                mobile_open: !mobile_path.is_empty(),
                mobile_path,
                agent_status: "not_checked".into(),
                agent_info: None,
                language,
                library,
                library_notice,
                ..Default::default()
            }))
        }),
    )
}
