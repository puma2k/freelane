//! Viewer lifecycle policy and UI-thread native surface. Input remains in app_ui.
#[cfg(all(target_os = "macos", feature = "native-video"))]
use super::native_video::{Admission, Surface};
use eframe::egui;
use freelane::video::LiveSession;
use freelane_agent_protocol::WindowTarget;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attempt {
    pub target: WindowTarget,
    revision: u64,
}
pub struct ViewerVideo {
    pub requested: bool,
    target: Option<WindowTarget>,
    attempted: Option<Attempt>,
    revision: u64,
    minimized: bool,
    session: Option<LiveSession>,
    retiring: Option<Arc<AtomicBool>>,
    dimensions: Option<(u16, u16)>,
    started: Option<Instant>,
    ready: bool,
    status: String,
    #[cfg(all(target_os = "macos", feature = "native-video"))]
    surface: Option<Surface>,
    #[cfg(all(target_os = "macos", feature = "native-video"))]
    pending: Option<freelane_agent_protocol::video::Record>,
}
impl Default for ViewerVideo {
    fn default() -> Self {
        Self {
            requested: Self::supported(),
            target: None,
            attempted: None,
            revision: 0,
            minimized: false,
            session: None,
            retiring: None,
            dimensions: None,
            started: None,
            ready: false,
            status: String::new(),
            #[cfg(all(target_os = "macos", feature = "native-video"))]
            surface: None,
            #[cfg(all(target_os = "macos", feature = "native-video"))]
            pending: None,
        }
    }
}
impl ViewerVideo {
    pub fn supported() -> bool {
        cfg!(all(target_os = "macos", feature = "native-video"))
    }
    pub fn maintain(&mut self, target: Option<WindowTarget>, minimized: bool) {
        if self.target != target || self.minimized != minimized {
            self.invalidate();
            self.target = target;
            self.minimized = minimized;
        }
        if self
            .retiring
            .as_ref()
            .is_some_and(|done| done.load(Ordering::Acquire))
        {
            self.retiring = None;
        }
        if self.session.as_ref().is_some_and(|s| s.ended())
            || (!self.ready
                && self
                    .started
                    .is_some_and(|t| t.elapsed() > Duration::from_secs(3)))
        {
            self.stop();
            self.status = "Video unavailable; using snapshots".into();
        }
    }
    pub fn begin(&mut self) -> Option<Attempt> {
        if !self.requested || self.minimized || self.session.is_some() || self.retiring.is_some() {
            return None;
        }
        let target = self.target.as_ref()?;
        if self.attempted.is_some() {
            return None;
        }
        let attempt = Attempt {
            target: target.clone(),
            revision: self.revision,
        };
        self.attempted = Some(attempt.clone());
        Some(attempt)
    }
    pub fn failed(&mut self, attempt: &Attempt, error: String) {
        if self.target.as_ref() == Some(&attempt.target) && self.attempted.as_ref() == Some(attempt)
        {
            self.status = error;
        }
    }
    pub fn accept(&mut self, attempt: &Attempt, result: Result<LiveSession, String>) {
        match result {
            Ok(session)
                if self.target.as_ref() == Some(&attempt.target)
                    && self.attempted.as_ref() == Some(attempt)
                    && session.target() == &attempt.target
                    && self.requested
                    && !self.minimized =>
            {
                self.session = Some(session);
                self.started = Some(Instant::now());
                self.status.clear();
            }
            Ok(session) => {
                self.wait_for_retirement(session.retirement());
                drop(session);
            }
            Err(error) => self.failed(attempt, error),
        }
    }
    pub fn wait_for_retirement(&mut self, done: Arc<AtomicBool>) {
        self.retiring = Some(done);
    }
    fn stop(&mut self) {
        #[cfg(all(target_os = "macos", feature = "native-video"))]
        {
            self.surface = None;
            self.pending = None;
        }
        self.dimensions = None;
        self.ready = false;
        self.started = None;
        if let Some(session) = self.session.take() {
            self.wait_for_retirement(session.retirement());
            drop(session);
        }
    }
    pub fn invalidate(&mut self) {
        self.stop();
        self.attempted = None;
        if let Some(next) = self.revision.checked_add(1) {
            self.revision = next;
        } else {
            self.requested = false;
        }
        self.status.clear();
    }
    pub fn active(&self) -> bool {
        self.session.is_some()
    }
    pub fn ready(&self) -> bool {
        self.ready
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    /// Native CI/diagnostic readback; does not read decoded pixels.
    #[cfg(all(target_os = "macos", feature = "native-video"))]
    #[allow(dead_code)]
    pub fn diagnostics(&self) -> Option<super::native_video::Stats> {
        self.surface.as_ref().map(|surface| surface.stats())
    }
    pub fn dimensions(&self) -> Option<(u16, u16)> {
        self.dimensions
    }
    pub fn prepare(&mut self, _frame: &eframe::Frame) {
        #[cfg(all(target_os = "macos", feature = "native-video"))]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let result = (|| -> Result<(), String> {
                if self.surface.is_some() {
                    return Ok(());
                }
                let Some(configuration) = self.session.as_ref().and_then(|s| s.configuration())
                else {
                    return Ok(());
                };
                let RawWindowHandle::AppKit(handle) =
                    _frame.window_handle().map_err(|e| e.to_string())?.as_raw()
                else {
                    return Err("Native view unavailable".into());
                };
                // Borrowed eframe NSView stays retained by Swift until detach.
                let mut surface =
                    unsafe { Surface::attach(handle.ns_view) }.map_err(|e| e.to_string())?;
                surface
                    .configure(&configuration)
                    .map_err(|e| e.to_string())?;
                self.dimensions = Some((configuration.width, configuration.height));
                self.surface = Some(surface);
                Ok(())
            })();
            if let Err(error) = result {
                self.stop();
                self.status = error;
            }
        }
    }
    pub fn render(&mut self, _ctx: &egui::Context, _rect: egui::Rect) {
        #[cfg(all(target_os = "macos", feature = "native-video"))]
        {
            let result = (|| -> Result<(), String> {
                let (Some(surface), Some(session)) = (self.surface.as_mut(), self.session.as_ref())
                else {
                    return Ok(());
                };
                let scale = _ctx.zoom_factor() as f64;
                surface.place(
                    _rect.left() as f64 * scale,
                    _rect.top() as f64 * scale,
                    _rect.width() as f64 * scale,
                    _rect.height() as f64 * scale,
                    true,
                );
                session.recover_if_needed();
                for _ in 0..2 {
                    if self.pending.is_none() {
                        self.pending = session.pop();
                    }
                    let Some(record) = self.pending.as_ref() else {
                        break;
                    };
                    match surface.submit(record).map_err(|e| e.to_string())? {
                        Admission::Accepted => {
                            self.pending = None;
                        }
                        Admission::Busy => {
                            _ctx.request_repaint_after(Duration::from_millis(8));
                            break;
                        }
                    }
                }
                let stats = surface.stats();
                if stats.error != 0 {
                    return Err("Video decoder unavailable; using snapshots".into());
                }
                self.ready =
                    stats.hardware && stats.decoded > 0 && stats.presented > 0 && stats.rendering;
                if stats.inflight > 0 || !self.ready {
                    _ctx.request_repaint_after(Duration::from_millis(16));
                }
                Ok(())
            })();
            if let Err(error) = result {
                self.stop();
                self.status = error;
            }
        }
    }
}

impl Drop for ViewerVideo {
    fn drop(&mut self) {
        self.stop();
    }
}
