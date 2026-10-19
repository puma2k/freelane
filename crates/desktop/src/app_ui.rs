//! Separate native viewer process per saved Windows application; native video with snapshot fallback.
use super::viewer_schedule::{Decision, Schedule};
use eframe::egui;
use freelane::{
    agent,
    app_launcher::{self, AppProfile},
};
use freelane_agent_protocol::window_input::{Button, Gesture, Position};
use freelane_agent_protocol::{Action, Outcome, WindowInfo, WindowTarget};
use std::{
    collections::VecDeque,
    path::Path,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};
enum Event {
    Opened(u32, Vec<WindowInfo>, bool),
    Frame(WindowTarget, super::app_frame::Decoded),
    Windows(Vec<WindowInfo>),
    Input,
    Video(WindowTarget, Result<freelane::video::LiveSession, String>),
}
type Reply = Result<Event, String>;
struct Viewer {
    language: super::Language,
    profile: AppProfile,
    attach_pid: Option<u32>,
    pid: Option<u32>,
    windows: Vec<WindowInfo>,
    selected: usize,
    texture: Option<egui::TextureHandle>,
    frame_key: Option<std::sync::Arc<super::app_frame::Key>>,
    worker: Option<Receiver<Reply>>,
    queue: VecDeque<Action>,
    schedule: Schedule,
    notice: String,
    started: bool,
    live: bool,
    text: String,
    extended: bool,
    drag: Option<(WindowTarget, Position, Button)>,
    guest_width: u16,
    guest_height: u16,
    video: crate::viewer_video::ViewerVideo,
}
impl Viewer {
    fn task(&mut self, ctx: &egui::Context, f: impl FnOnce() -> Reply + Send + 'static) {
        let (sender, receiver) = mpsc::channel();
        self.worker = Some(receiver);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(f());
            ctx.request_repaint();
        });
    }
    fn target(&self) -> Option<WindowTarget> {
        self.windows.get(self.selected).map(|w| w.target.clone())
    }
    fn enqueue(&mut self, action: Action) {
        if matches!(action, Action::WindowResize { .. }) {
            self.video.invalidate();
        }
        if self.queue.len() < 32 {
            self.queue.push_back(action);
        } else {
            self.notice = "Input queue full; new action was not sent".into();
        }
    }
    // Returns false when a stale frame should skip this render turn.
    fn apply_reply(&mut self, ctx: &egui::Context, reply: Reply) -> bool {
        self.worker = None;
        match reply {
            Ok(Event::Opened(pid, windows, extended)) => {
                self.extended = extended;
                self.pid = Some(pid);
                self.windows = windows;
                self.notice = "Application opened; capturing window".into();
                self.schedule.reset(Instant::now());
            }
            Ok(Event::Frame(target, frame)) => {
                if self.target().as_ref() != Some(&target) {
                    self.schedule.reset(Instant::now());
                    return false;
                }

                if let Some(image) = frame.image {
                    self.texture =
                        Some(ctx.load_texture("app-frame", image, egui::TextureOptions::LINEAR));
                } else if self.texture.is_none() {
                    self.frame_key = None;
                    let key = frame.key;
                    self.task(ctx, move || {
                        super::app_frame::restore(key).map(|frame| Event::Frame(target, frame))
                    });
                    return true;
                }
                self.frame_key = Some(frame.key);
                self.notice.clear();
                self.schedule.frame_completed(Instant::now());
            }
            Ok(Event::Windows(windows)) => {
                self.video.invalidate();
                self.queue.clear();
                self.drag = None;
                self.windows = windows;
                self.selected = 0;
                self.texture = None;
                self.frame_key = None;
                self.schedule.reset(Instant::now());
            }
            Ok(Event::Video(target, result)) => {
                self.video.accept(&target, result);
            }
            Ok(Event::Input) => {
                self.schedule
                    .input_completed(Instant::now(), self.queue.is_empty());
            }
            Err(e) => {
                self.frame_key = None;
                self.notice = if self.queue.is_empty() {
                    e
                } else {
                    format!(
                        "{e}. Discarded {} unsent actions; inspect the application before retrying",
                        self.queue.len()
                    )
                };
                self.live = false;
                self.video.requested = false;
                self.video.invalidate();
                self.queue.clear();
            }
        }
        true
    }

    fn capture(&mut self, ctx: &egui::Context) {
        let Some(target) = self.target() else {
            return;
        };
        let vm = self.profile.vm.clone();
        let previous = self.texture.as_ref().and(self.frame_key.clone());
        self.task(ctx, move || {
            let action = Action::CaptureWindow {
                target: target.clone(),
            };
            let reply = match agent::window_request(
                &vm,
                Action::CaptureWindow {
                    target: target.clone(),
                },
            ) {
                Ok(reply) => reply,
                Err(error)
                    if error.to_string()
                        == "Guest agent: Window capture timed out; guest may be unresponsive" =>
                {
                    // A guest-reported capture timeout acknowledges this read-only RPC.
                    // Retry one snapshot after a pause; never retry input or transport errors.
                    std::thread::sleep(Duration::from_secs(1));
                    agent::window_request(&vm, action).map_err(|e| e.to_string())?
                }
                Err(error) => return Err(error.to_string()),
            };
            let Outcome::WindowFrame {
                width,
                height,
                jpeg_hex,
                ..
            } = reply.outcome
            else {
                return Err("Guest did not return a window frame".into());
            };
            let decoded = super::app_frame::decode(
                target.clone(),
                width,
                height,
                jpeg_hex,
                previous.as_ref(),
            )?;
            Ok(Event::Frame(target, decoded))
        });
    }
}
impl eframe::App for Viewer {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        self.video.maintain(self.target(), minimized);
        if !self.started {
            self.started = true;
            let p = self.profile.clone();
            let attach_pid = self.attach_pid;
            self.notice = "Waiting for Windows and agent; launch will be sent once".into();
            self.task(&ctx, move || {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                let cli = exe.parent().ok_or("CLI not found")?.join("freelane");
                let (pid, windows) = if let Some(pid) = attach_pid {
                    match agent::window_inventory(&p.vm, pid)
                        .map_err(|e| e.to_string())?
                        .outcome
                    {
                        Outcome::Windows { windows } if !windows.is_empty() => (pid, windows),
                        _ => {
                            return Err(
                                "Process has no interactive window; launch was not sent".into()
                            )
                        }
                    }
                } else {
                    app_launcher::open(&p, &cli).map_err(|e| e.to_string())?
                };
                let extended = freelane::agent_capabilities::inspect(&p.vm)
                    .ok()
                    .is_some_and(|i| {
                        i.supports(
                            "window_input",
                            freelane_agent_protocol::window_input::VERSION,
                        ) == Some(true)
                    });
                Ok(Event::Opened(pid, windows, extended))
            });
        }
        if let Some(receiver) = &self.worker {
            let ready = match receiver.try_recv() {
                Ok(reply) => Some(reply),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                    "Viewer worker stopped; inspect before retrying any mutation".into(),
                )),
            };
            if let Some(reply) = ready {
                if !self.apply_reply(&ctx, reply) {
                    return;
                }
            }
        }
        let lang = self.language;
        egui::Panel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::WHITE)
                    .inner_margin(16),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(&self.profile.name);
                    egui::ComboBox::from_id_salt("viewer-language")
                        .selected_text(if self.language == super::Language::English {
                            "English"
                        } else {
                            "Tiếng Việt"
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.language,
                                super::Language::English,
                                "English",
                            );
                            ui.selectable_value(
                                &mut self.language,
                                super::Language::Vietnamese,
                                "Tiếng Việt",
                            );
                        });
                });
                ui.small(lang.text("Windows app viewer"));
                ui.horizontal_wrapped(|ui| {
                    let old = self.selected;
                    egui::ComboBox::from_id_salt("guest-window")
                        .selected_text(
                            self.windows
                                .get(self.selected)
                                .map(|w| w.title.as_str())
                                .unwrap_or(lang.text("No windows")),
                        )
                        .show_ui(ui, |ui| {
                            for (index, window) in self.windows.iter().enumerate() {
                                ui.selectable_value(&mut self.selected, index, &window.title);
                            }
                        });
                    if old != self.selected {
                        self.video.invalidate();
                        self.drag = None;
                        self.texture = None;
                        self.frame_key = None;
                        self.queue.clear();
                        self.schedule.reset(Instant::now());
                    }
                    if crate::viewer_video::ViewerVideo::supported() {
                        if ui
                            .checkbox(&mut self.video.requested, lang.text("Live video"))
                            .changed()
                        {
                            self.video.invalidate();
                        }
                        ui.label(lang.text(if self.video.ready() {
                            "Video"
                        } else {
                            "Snapshots"
                        }))
                        .on_hover_text(lang.message(self.video.status()));
                    }
                    if !self.video.active() {
                        ui.checkbox(&mut self.live, lang.text("Refresh every second"));
                    }
                    if ui
                        .add_enabled(
                            self.worker.is_none()
                                && self.target().is_some()
                                && !self.video.active(),
                            egui::Button::new(lang.text("Refresh frame")),
                        )
                        .clicked()
                    {
                        self.capture(&ctx);
                    }
                    if ui
                        .add_enabled(
                            self.worker.is_none() && self.pid.is_some(),
                            egui::Button::new(lang.text("Refresh windows")),
                        )
                        .clicked()
                    {
                        let vm = self.profile.vm.clone();
                        let pid = self.pid.unwrap();
                        self.task(&ctx, move || {
                            match agent::window_inventory(&vm, pid)
                                .map_err(|e| e.to_string())?
                                .outcome
                            {
                                Outcome::Windows { windows } => Ok(Event::Windows(windows)),
                                _ => Err("Invalid window response".into()),
                            }
                        });
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.text)
                            .hint_text(lang.text("Text for focused guest control")),
                    );
                    if ui
                        .add_enabled(
                            self.target().is_some() && !self.text.is_empty(),
                            egui::Button::new(lang.text("Send text")),
                        )
                        .clicked()
                    {
                        if self.text.len() <= 1024 {
                            let text = std::mem::take(&mut self.text);
                            self.enqueue(Action::WindowText {
                                target: self.target().unwrap(),
                                text,
                            });
                        } else {
                            self.notice = "Text is limited to 1024 bytes per request".into();
                        }
                    }
                });
                if self.extended {
                    ui.horizontal_wrapped(|ui| {
                        for (label, key) in [("Copy in guest", 67), ("Paste in guest", 86)] {
                            if ui
                                .add_enabled(
                                    self.target().is_some(),
                                    egui::Button::new(lang.text(label)),
                                )
                                .clicked()
                            {
                                self.enqueue(Action::WindowShortcut {
                                    target: self.target().unwrap(),
                                    key,
                                    modifiers: 1,
                                });
                            }
                        }
                        ui.label(lang.text("Guest window size"));
                        ui.add(egui::DragValue::new(&mut self.guest_width).range(64..=8192));
                        ui.label("×");
                        ui.add(egui::DragValue::new(&mut self.guest_height).range(64..=8192));
                        if ui
                            .add_enabled(
                                self.target().is_some(),
                                egui::Button::new(lang.text("Resize window")),
                            )
                            .clicked()
                        {
                            self.enqueue(Action::WindowResize {
                                target: self.target().unwrap(),
                                width: self.guest_width,
                                height: self.guest_height,
                            });
                        }
                    });
                }
                if self.worker.is_some() {
                    ui.spinner();
                }
                ui.label(lang.message(&self.notice));
            });
        self.video.maintain(self.target(), minimized);
        self.video.prepare(frame);
        egui::CentralPanel::default().show(ui, |ui| {
            let response = if let Some((width, height)) = self.video.dimensions() {
                let available = ui.available_size();
                let scale = (available.x / width as f32)
                    .min(available.y / height as f32)
                    .max(0.001);
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(width as f32 * scale, height as f32 * scale),
                    egui::Sense::click_and_drag(),
                );
                self.video.render(&ctx, rect);
                Some(response)
            } else {
                self.texture.as_ref().map(|texture| {
                    ui.add(
                        egui::Image::new(texture)
                            .max_size(ui.available_size())
                            .sense(egui::Sense::click_and_drag()),
                    )
                })
            };
            if let Some(response) = response {
                let position = |pos: egui::Pos2| Position {
                    x: ((pos.x - response.rect.left()) / response.rect.width() * 32767.0)
                        .clamp(0.0, 32767.0) as u16,
                    y: ((pos.y - response.rect.top()) / response.rect.height() * 32767.0)
                        .clamp(0.0, 32767.0) as u16,
                };
                for (native, button) in [
                    (egui::PointerButton::Primary, Button::Primary),
                    (egui::PointerButton::Secondary, Button::Secondary),
                ] {
                    if response.clicked_by(native) {
                        response.request_focus();
                        if let (Some(pos), Some(target)) =
                            (response.interact_pointer_pos(), self.target())
                        {
                            let at = position(pos);
                            if self.extended {
                                // egui emits the first single click before the second click arrives.
                                // The second event emits only double-click/up, avoiding a third click.
                                self.enqueue(Action::WindowPointer {
                                    target,
                                    gesture: Gesture::Click {
                                        at,
                                        button,
                                        count: if response.double_clicked_by(native) {
                                            2
                                        } else {
                                            1
                                        },
                                    },
                                });
                            } else if button == Button::Primary {
                                self.enqueue(Action::WindowClick {
                                    target,
                                    x: at.x,
                                    y: at.y,
                                });
                            }
                        }
                    }
                    if self.extended && response.drag_started_by(native) {
                        response.request_focus();
                        if let (Some(origin), Some(target)) =
                            (ctx.input(|i| i.pointer.press_origin()), self.target())
                        {
                            self.drag = Some((target, position(origin), button));
                        }
                    }
                    if response.drag_stopped_by(native) {
                        if let Some((target, from, drag_button)) = self.drag.take() {
                            if drag_button == button && self.target().as_ref() == Some(&target) {
                                if let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) {
                                    self.enqueue(Action::WindowPointer {
                                        target,
                                        gesture: Gesture::Drag {
                                            from,
                                            to: position(pos),
                                            button,
                                        },
                                    });
                                }
                            }
                        }
                    }
                }
                for event in ctx.input(|i| i.events.clone()) {
                    if self.extended && response.hovered() {
                        if let egui::Event::MouseWheel { unit, delta, .. } = event {
                            if let (Some(pos), Some(target)) =
                                (ctx.input(|i| i.pointer.hover_pos()), self.target())
                            {
                                for (horizontal, value) in [(false, delta.y), (true, delta.x)] {
                                    let delta =
                                        super::app_input::wheel_delta(unit, horizontal, value);
                                    if delta != 0 {
                                        self.enqueue(Action::WindowPointer {
                                            target: target.clone(),
                                            gesture: Gesture::Wheel {
                                                at: position(pos),
                                                horizontal,
                                                delta,
                                            },
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                if response.has_focus() {
                    if let Some(target) = self.target() {
                        for action in super::app_input::key_actions(
                            &ctx.input(|i| i.events.clone()),
                            &target,
                            self.extended,
                        ) {
                            self.enqueue(action);
                        }
                    }
                }
            } else {
                ui.label(
                    lang.text("No window frame; run the updated agent in the Windows user session"),
                );
            }
            ui.small(lang.text("Closing this viewer keeps the guest application and VM running"));
        });
        match self.schedule.choose(
            Instant::now(),
            self.worker.is_some(),
            self.live && !self.video.active() && !minimized,
            self.target().is_some(),
            !self.queue.is_empty(),
        ) {
            Decision::Input => {
                if let Some(action) = self.queue.pop_front() {
                    let vm = self.profile.vm.clone();
                    self.task(&ctx, move || {
                        agent::window_request(&vm, action).map_err(|e| e.to_string())?;
                        Ok(Event::Input)
                    });
                }
            }
            Decision::Capture => self.capture(&ctx),
            Decision::Idle => {}
        }
        if self.worker.is_none() && self.queue.is_empty() {
            if let Some(target) = self.video.begin() {
                let vm = self.profile.vm.clone();
                let wake = ctx.clone();
                self.task(&ctx, move || {
                    let result = freelane::video::LiveSession::start(
                        &vm,
                        target.clone(),
                        std::sync::Arc::new(move || wake.request_repaint()),
                    )
                    .map_err(|e| e.to_string());
                    Ok(Event::Video(target, result))
                });
            }
        }
        let repaint = if self.video.active() && !self.video.ready() {
            Duration::from_millis(100)
        } else if self.worker.is_some() || !self.queue.is_empty() {
            Duration::from_millis(250) // Observe a disconnected worker even if it never sends a wakeup.
        } else if self.live && self.target().is_some() && !self.video.active() && !minimized {
            self.schedule
                .deadline()
                .saturating_duration_since(Instant::now())
                .max(Duration::from_millis(16))
        } else {
            Duration::from_secs(5) // Native input and worker completion still wake the event loop immediately.
        };
        ctx.request_repaint_after(repaint);
    }
}
pub fn run(path: &Path, attach_pid: Option<u32>, language: super::Language) -> eframe::Result {
    let profile = match AppProfile::load(path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return Err(eframe::Error::AppCreation(Box::new(e)));
        }
    };
    let title = profile.name.clone();
    eframe::run_native(
        &title,
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([900.0, 650.0])
                .with_min_inner_size([400.0, 300.0]),
            ..Default::default()
        },
        Box::new(move |creation| {
            super::interface::install_style(&creation.egui_ctx);
            Ok(Box::new(Viewer {
                language,
                profile,
                attach_pid,
                pid: None,
                windows: Vec::new(),
                selected: 0,
                texture: None,
                frame_key: None,
                worker: None,
                queue: VecDeque::new(),
                schedule: Schedule::new(Instant::now()),
                notice: String::new(),
                started: false,
                live: true,
                text: String::new(),
                extended: false,
                drag: None,
                guest_width: 1280,
                guest_height: 720,
                video: crate::viewer_video::ViewerVideo::default(),
            }))
        }),
    )
}

#[cfg(test)]
mod scheduling_tests {
    use super::*;
    fn target(handle: u64) -> WindowTarget {
        WindowTarget {
            handle,
            pid: 2,
            created: 3,
            incarnation: 4,
        }
    }
    fn decoded(
        target: WindowTarget,
        previous: Option<&std::sync::Arc<super::super::app_frame::Key>>,
    ) -> super::super::app_frame::Decoded {
        let mut data = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, 90)
            .encode(&[255, 0, 0], 1, 1, image::ExtendedColorType::Rgb8)
            .unwrap();
        super::super::app_frame::decode(
            target,
            1,
            1,
            data.iter().map(|b| format!("{b:02x}")).collect(),
            previous,
        )
        .unwrap()
    }
    fn viewer() -> Viewer {
        Viewer {
            language: super::super::Language::English,
            profile: AppProfile {
                name: "Fixture".into(),
                vm: "/unused/vm.toml".into(),
                executable: r"C:\Windows\notepad.exe".into(),
                arguments: vec![],
                boot_timeout_seconds: 10,
            },
            attach_pid: None,
            pid: Some(2),
            windows: vec![
                WindowInfo {
                    target: target(1),
                    title: "One".into(),
                },
                WindowInfo {
                    target: target(2),
                    title: "Two".into(),
                },
            ],
            selected: 0,
            texture: None,
            frame_key: None,
            worker: None,
            queue: VecDeque::new(),
            schedule: Schedule::new(Instant::now()),
            notice: String::new(),
            started: true,
            live: false,
            text: String::new(),
            extended: true,
            drag: None,
            guest_width: 1280,
            guest_height: 720,
            video: crate::viewer_video::ViewerVideo::default(),
        }
    }
    fn deliver(viewer: &mut Viewer, reply: Reply) {
        let (tx, rx) = mpsc::channel();
        tx.send(reply).unwrap();
        viewer.worker = Some(rx);
        let ctx = egui::Context::default();
        viewer.apply_reply(&ctx, viewer.worker.as_ref().unwrap().recv().unwrap());
    }

    #[test]
    fn unavailable_video_preserves_queued_input_and_snapshot_refresh() {
        let mut v = viewer();
        v.live = true;
        v.video.requested = true;
        v.video.maintain(v.target(), false);
        v.video.begin();
        v.enqueue(Action::WindowText {
            target: target(1),
            text: "queued once".into(),
        });
        deliver(&mut v, Ok(Event::Video(target(1), Err("No helper".into()))));
        assert_eq!(v.queue.len(), 1);
        assert!(v.live);
        assert!(!v.video.active());
        assert_eq!(v.video.status(), "No helper");
    }
    #[test]
    fn stale_frame_cannot_replace_selected_window_texture() {
        let mut v = viewer();
        v.selected = 1;
        deliver(
            &mut v,
            Ok(Event::Frame(target(1), decoded(target(1), None))),
        );
        assert!(v.texture.is_none());
        assert!(v.worker.is_none());
        assert_eq!(v.target(), Some(target(2)));
    }
    #[test]
    fn failed_input_discards_pending_actions_and_stops_live_without_replay() {
        let mut v = viewer();
        let ctx = egui::Context::default();
        v.apply_reply(&ctx, Ok(Event::Frame(target(1), decoded(target(1), None))));
        v.live = true;
        v.enqueue(Action::WindowText {
            target: target(1),
            text: "unsent".into(),
        });
        deliver(&mut v, Err("Transport unavailable".into()));
        assert!(!v.live);
        assert!(v.frame_key.is_none());
        assert!(v.queue.is_empty());
        assert!(v.worker.is_none());
        assert!(v.notice.contains("Discarded 1 unsent"));
    }
    #[test]
    fn inventory_change_clears_old_input_drag_and_texture() {
        let mut v = viewer();
        let ctx = egui::Context::default();
        v.apply_reply(&ctx, Ok(Event::Frame(target(1), decoded(target(1), None))));
        assert!(v.texture.is_some() && v.frame_key.is_some());
        v.enqueue(Action::WindowKey {
            target: target(1),
            key: 13,
        });
        v.drag = Some((target(1), Position { x: 0, y: 0 }, Button::Primary));
        deliver(
            &mut v,
            Ok(Event::Windows(vec![WindowInfo {
                target: target(2),
                title: "Two".into(),
            }])),
        );
        assert!(v.queue.is_empty());
        assert!(v.drag.is_none());
        assert!(v.texture.is_none());
        assert!(v.frame_key.is_none());
        assert_eq!(v.target(), Some(target(2)));
    }
    #[test]
    fn full_input_queue_preserves_first_32_actions_and_refuses_new_action() {
        let mut v = viewer();
        for n in 0..33 {
            v.enqueue(Action::WindowText {
                target: target(1),
                text: n.to_string(),
            });
        }
        assert_eq!(v.queue.len(), 32);
        for n in 0..32 {
            assert!(
                matches!(v.queue.pop_front(),Some(Action::WindowText{text,..}) if text==n.to_string())
            );
        }
        assert!(v.notice.contains("not sent"));
    }
    #[test]
    fn identical_frame_keeps_existing_texture_allocation() {
        let mut v = viewer();
        let ctx = egui::Context::default();
        v.apply_reply(&ctx, Ok(Event::Frame(target(1), decoded(target(1), None))));
        let before = v.texture.as_ref().unwrap().id();
        let next = decoded(target(1), v.frame_key.as_ref());
        v.apply_reply(&ctx, Ok(Event::Frame(target(1), next)));
        assert_eq!(v.texture.as_ref().unwrap().id(), before);
    }
    #[test]
    fn manual_unchanged_reply_restores_missing_texture_even_with_live_off() {
        let mut v = viewer();
        let ctx = egui::Context::default();
        assert!(!v.live);
        let first = decoded(target(1), None);
        let same = decoded(target(1), Some(&first.key));
        assert!(same.image.is_none());
        v.apply_reply(&ctx, Ok(Event::Frame(target(1), same)));
        assert!(
            v.worker.is_some(),
            "Cacheless manual reply needs a local decode worker"
        );
        let result = v
            .worker
            .as_ref()
            .unwrap()
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
        v.apply_reply(&ctx, result);
        assert!(v.texture.is_some() && v.frame_key.is_some());
        assert!(v.worker.is_none() && !v.live);
    }
    #[test]
    fn local_restore_cannot_display_a_target_selected_away_from() {
        let mut v = viewer();
        let ctx = egui::Context::default();
        let first = decoded(target(1), None);
        let same = decoded(target(1), Some(&first.key));
        v.apply_reply(&ctx, Ok(Event::Frame(target(1), same)));
        v.selected = 1;
        let result = v
            .worker
            .as_ref()
            .unwrap()
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
        assert!(!v.apply_reply(&ctx, result));
        assert!(v.texture.is_none() && v.frame_key.is_none());
        assert!(v.worker.is_none());
        assert_eq!(v.target(), Some(target(2)));
    }
}
