//! Owned-window native integration fixture; not the normal viewer entry point.
#[cfg(not(all(target_os = "macos", feature = "native-video")))]
fn main() {
    eprintln!("Run on macOS with --features native-video");
    std::process::exit(1);
}

#[cfg(all(target_os = "macos", feature = "native-video"))]
#[path = "../src/native_video.rs"]
mod native_video;

#[cfg(all(target_os = "macos", feature = "native-video"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use eframe::egui;
    use freelane_agent_protocol::video::{read_record, Record};
    use native_video::{Admission, Surface};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::{
        collections::VecDeque,
        io,
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };

    struct Fixture {
        records: VecDeque<Record>,
        surface: Option<Surface>,
        started: Instant,
        next: Instant,
        result: Arc<Mutex<Option<Result<serde_json::Value, String>>>>,
    }
    impl Fixture {
        fn tick(
            &mut self,
            ui: &mut egui::Ui,
            frame: &mut eframe::Frame,
        ) -> Result<bool, Box<dyn std::error::Error>> {
            if self.started.elapsed() > Duration::from_secs(15) {
                return Err(io::Error::other("Native fixture timed out").into());
            }
            if self.surface.is_none() {
                let RawWindowHandle::AppKit(handle) = frame.window_handle()?.as_raw() else {
                    return Err(io::Error::other("Expected AppKit view").into());
                };
                // eframe owns this NSView until the UI app is dropped on the same thread.
                let mut surface = unsafe { Surface::attach(handle.ns_view)? };
                surface.configure(
                    &self
                        .records
                        .pop_front()
                        .ok_or_else(|| io::Error::other("Missing configuration"))?,
                )?;
                self.surface = Some(surface);
            }
            let surface = self.surface.as_mut().unwrap();
            let rect = ui.available_rect_before_wrap().shrink(16.0);
            surface.place(
                rect.left().into(),
                rect.top().into(),
                rect.width().into(),
                rect.height().into(),
                true,
            );
            if Instant::now() >= self.next {
                if let Some(record) = self.records.front() {
                    if surface.submit(record)? == Admission::Accepted {
                        self.records.pop_front();
                        self.next = Instant::now() + Duration::from_millis(33);
                    }
                }
            }
            let stats = surface.stats();
            if stats.error != 0 {
                return Err(
                    io::Error::other(format!("Native decoder error {}", stats.error)).into(),
                );
            }
            if self.records.is_empty()
                && stats.decoded == 30
                && stats.presented > 0
                && stats.inflight == 0
                && stats.hardware
                && stats.rendering
            {
                *self.result.lock().unwrap() = Some(Ok(
                    serde_json::json!({"decoded":stats.decoded,"presented":stats.presented,"hardware":stats.hardware,"embedded_eframe_view":true,"onscreen_pixels_verified":false}),
                ));
                return Ok(true);
            }
            ui.ctx().request_repaint_after(Duration::from_millis(10));
            Ok(false)
        }
    }
    impl eframe::App for Fixture {
        fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
            // Closing is deferred by eframe: later UI passes must preserve the
            // terminal report and must not recreate a decoder on an empty stream.
            if self.result.lock().unwrap().is_some() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            let close = match self.tick(ui, frame) {
                Ok(close) => close,
                Err(error) => {
                    *self.result.lock().unwrap() = Some(Err(error.to_string()));
                    true
                }
            };
            if close {
                self.surface.take(); // Detach while eframe's NSView is still alive.
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
    let path = std::env::args()
        .nth(1)
        .ok_or("Expected owned captured-packets.flv")?;
    let bytes = std::fs::read(path)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("Fixture exceeds finite input budget".into());
    }
    let mut input = std::io::Cursor::new(bytes);
    let mut records = VecDeque::new();
    while let Some(record) = read_record(&mut input)? {
        records.push_back(record);
    }
    if records.len() != 31 {
        return Err("Expected configuration plus thirty pictures".into());
    }
    let result = Arc::new(Mutex::new(None));
    let output = result.clone();
    eframe::run_native(
        "FreeLane owned native video fixture",
        eframe::NativeOptions::default(),
        Box::new(move |_| {
            Ok(Box::new(Fixture {
                records,
                surface: None,
                started: Instant::now(),
                next: Instant::now(),
                result,
            }))
        }),
    )?;
    let report = output
        .lock()
        .unwrap()
        .take()
        .ok_or("Fixture window closed before verification")?
        .map_err(io::Error::other)?;
    println!("{report}");
    Ok(())
}
