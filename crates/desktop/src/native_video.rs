//! Native-video prototype; app session wiring is a separate layer.
#![allow(dead_code)]
use freelane_agent_protocol::video::{Binding, Kind, Record};
use std::io;
#[derive(Default)]
pub struct StreamCheck {
    configuration: Option<(Binding, u16, u16)>,
    last: Option<(u64, u64)>,
}
impl StreamCheck {
    pub fn configuration(&mut self, record: &Record) -> io::Result<()> {
        record.validate()?;
        if self.configuration.is_some() || record.kind != Kind::Configuration {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.configuration = Some((record.binding, record.width, record.height));
        Ok(())
    }
    pub fn frame(&self, record: &Record) -> io::Result<()> {
        record.validate()?;
        if self.configuration != Some((record.binding, record.width, record.height))
            || record.pts_us > i64::MAX as u64
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let Kind::AccessUnit { idr } = record.kind else {
            return Err(io::ErrorKind::InvalidInput.into());
        };
        if self.last.is_none() && !idr
            || self
                .last
                .is_some_and(|(sequence, pts)| record.sequence <= sequence || record.pts_us <= pts)
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(())
    }
    pub fn accepted(&mut self, record: &Record) {
        self.last = Some((record.sequence, record.pts_us));
    }
}
#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub submitted: u64,
    pub decoded: u64,
    pub presented: u64,
    pub superseded: u64,
    pub inflight: u64,
    pub hardware: bool,
    pub error: u64,
    pub rendering: bool,
}
#[derive(Debug, PartialEq)]
pub enum Admission {
    Accepted,
    Busy,
}

#[cfg(all(target_os = "macos", feature = "native-video"))]
mod platform {
    use super::*;
    use std::{ffi::c_void, marker::PhantomData, ptr::NonNull, rc::Rc};
    unsafe extern "C" {
        fn freelane_video_create(view: *mut c_void) -> *mut c_void;
        fn freelane_video_configure(
            context: *mut c_void,
            bytes: *const u8,
            length: usize,
            width: u32,
            height: u32,
        ) -> i32;
        fn freelane_video_submit(
            context: *mut c_void,
            bytes: *const u8,
            length: usize,
            pts: u64,
            idr: i32,
        ) -> i32;
        fn freelane_video_layout(
            context: *mut c_void,
            x: f64,
            y: f64,
            width: f64,
            height: f64,
            visible: i32,
        );
        fn freelane_video_stats(context: *mut c_void, values: *mut u64);
        fn freelane_video_destroy(context: *mut c_void);
    }
    pub struct Surface {
        context: NonNull<c_void>,
        check: StreamCheck,
        _ui: PhantomData<Rc<()>>,
    }
    impl Surface {
        /// # Safety
        /// `view` must be a live NSView borrowed from the current UI-thread window.
        pub unsafe fn attach(view: NonNull<c_void>) -> io::Result<Self> {
            if unsafe { libc::pthread_main_np() } != 1 {
                return Err(io::Error::other("Native video requires the Mac UI thread"));
            }
            let context = NonNull::new(unsafe { freelane_video_create(view.as_ptr()) })
                .ok_or_else(|| io::Error::other("Native surface unavailable"))?;
            Ok(Self {
                context,
                check: StreamCheck::default(),
                _ui: PhantomData,
            })
        }
        pub fn configure(&mut self, record: &Record) -> io::Result<()> {
            let mut check = StreamCheck::default();
            check.configuration(record)?;
            if self.check.configuration.is_some() {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let result = unsafe {
                freelane_video_configure(
                    self.context.as_ptr(),
                    record.payload.as_ptr(),
                    record.payload.len(),
                    record.width.into(),
                    record.height.into(),
                )
            };
            if result != 1 {
                return Err(io::Error::other("Native decoder configuration unavailable"));
            }
            self.check = check;
            Ok(())
        }
        pub fn submit(&mut self, record: &Record) -> io::Result<Admission> {
            self.check.frame(record)?;
            let Kind::AccessUnit { idr } = record.kind else {
                unreachable!()
            };
            match unsafe {
                freelane_video_submit(
                    self.context.as_ptr(),
                    record.payload.as_ptr(),
                    record.payload.len(),
                    record.pts_us,
                    idr.into(),
                )
            } {
                1 => {
                    self.check.accepted(record);
                    Ok(Admission::Accepted)
                }
                0 => Ok(Admission::Busy),
                _ => Err(io::Error::other("Native video decode failed")),
            }
        }
        pub fn place(&mut self, x: f64, y: f64, width: f64, height: f64, visible: bool) {
            unsafe {
                freelane_video_layout(self.context.as_ptr(), x, y, width, height, visible.into())
            }
        }
        pub fn stats(&self) -> Stats {
            let mut values = [0; 8];
            unsafe { freelane_video_stats(self.context.as_ptr(), values.as_mut_ptr()) };
            Stats {
                submitted: values[0],
                decoded: values[1],
                presented: values[2],
                superseded: values[3],
                inflight: values[4],
                hardware: values[5] == 1,
                error: values[6],
                rendering: values[7] == 1,
            }
        }
    }
    impl Drop for Surface {
        fn drop(&mut self) {
            unsafe { freelane_video_destroy(self.context.as_ptr()) }
        }
    }
}
#[cfg(all(target_os = "macos", feature = "native-video"))]
pub use platform::Surface;
