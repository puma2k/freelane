//! Snapshot/input prototype for ordinary Win32 client windows in the agent's user session.
mod input;
use freelane_agent_protocol::{Action, Outcome, WindowInfo, WindowTarget};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
type Handle = isize;
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Point {
    x: i32,
    y: i32,
}
#[repr(C)]
struct GuiInfo {
    size: u32,
    flags: u32,
    active: Handle,
    focus: Handle,
    capture: Handle,
    menu: Handle,
    move_size: Handle,
    caret: Handle,
    caret_rect: Rect,
}
#[repr(C)]
#[derive(Default)]
struct FileTime {
    low: u32,
    high: u32,
}
#[repr(C)]
struct BitmapInfo {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bits: u16,
    compression: u32,
    image_size: u32,
    x: i32,
    y: i32,
    used: u32,
    important: u32,
    color: u32,
}
#[link(name = "user32")]
extern "system" {
    fn SetPropW(window: Handle, name: *const u16, value: Handle) -> i32;
    fn GetPropW(window: Handle, name: *const u16) -> Handle;
    fn EnumWindows(callback: extern "system" fn(Handle, isize) -> i32, data: isize) -> i32;
    fn IsWindow(window: Handle) -> i32;
    fn IsWindowVisible(window: Handle) -> i32;
    fn IsIconic(window: Handle) -> i32;
    fn GetWindowThreadProcessId(window: Handle, pid: *mut u32) -> u32;
    fn GetWindowTextW(window: Handle, text: *mut u16, len: i32) -> i32;
    fn GetClientRect(window: Handle, rect: *mut Rect) -> i32;
    fn GetDC(window: Handle) -> Handle;
    fn ReleaseDC(window: Handle, dc: Handle) -> i32;
    fn PrintWindow(window: Handle, dc: Handle, flags: u32) -> i32;
    fn PostMessageW(window: Handle, message: u32, wparam: usize, lparam: isize) -> i32;
    fn GetGUIThreadInfo(thread: u32, info: *mut GuiInfo) -> i32;
    fn GetAncestor(window: Handle, flags: u32) -> Handle;
    fn ChildWindowFromPointEx(window: Handle, point: Point, flags: u32) -> Handle;
    fn ClientToScreen(window: Handle, point: *mut Point) -> i32;
    fn ScreenToClient(window: Handle, point: *mut Point) -> i32;
    fn MapVirtualKeyW(code: u32, kind: u32) -> u32;
}
#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(dc: Handle) -> Handle;
    fn DeleteDC(dc: Handle) -> i32;
    fn CreateCompatibleBitmap(dc: Handle, width: i32, height: i32) -> Handle;
    fn SelectObject(dc: Handle, object: Handle) -> Handle;
    fn DeleteObject(object: Handle) -> i32;
    fn GetDIBits(
        dc: Handle,
        bitmap: Handle,
        start: u32,
        lines: u32,
        bits: *mut u8,
        info: *mut BitmapInfo,
        usage: u32,
    ) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn QueryFullProcessImageNameW(
        process: Handle,
        flags: u32,
        name: *mut u16,
        size: *mut u32,
    ) -> i32;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
    fn GetProcessTimes(
        process: Handle,
        created: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
}
fn invalid(message: &str) -> io::Error {
    io::Error::other(message)
}
fn created(pid: u32) -> io::Result<u64> {
    unsafe {
        let process = OpenProcess(0x1000, 0, pid);
        if process == 0 {
            return Err(io::Error::last_os_error());
        }
        let (mut c, mut e, mut k, mut u) = (
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
        );
        let ok = GetProcessTimes(process, &mut c, &mut e, &mut k, &mut u);
        CloseHandle(process);
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((u64::from(c.high) << 32) | u64::from(c.low))
    }
}
fn property_name() -> *const u16 {
    static NAME: std::sync::OnceLock<Vec<u16>> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        "FreeLane.Window.Incarnation.v9"
            .encode_utf16()
            .chain(Some(0))
            .collect()
    })
    .as_ptr()
}
fn identities() -> &'static std::sync::Mutex<std::collections::BTreeMap<Handle, u64>> {
    static VALUES: std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeMap<Handle, u64>>> =
        std::sync::OnceLock::new();
    VALUES.get_or_init(Default::default)
}
fn incarnation(window: Handle) -> io::Result<u64> {
    let mut values = identities()
        .lock()
        .map_err(|_| invalid("Window identity unavailable"))?;
    values.retain(|handle, token| unsafe {
        IsWindow(*handle) != 0 && GetPropW(*handle, property_name()) as u64 == *token
    });
    if let Some(token) = values.get(&window) {
        return Ok(*token);
    }
    if values.len() >= 512 {
        return Err(invalid("Window identity capacity reached"));
    }
    let token = loop {
        let value = getrandom::u64().map_err(|_| invalid("Window identity entropy unavailable"))?
            & isize::MAX as u64;
        if value != 0 {
            break value;
        }
    };
    // Window properties are removed by USER32 when this HWND is destroyed.
    // UIPI refusal fails closed; no privilege or focus bypass is attempted.
    if unsafe { SetPropW(window, property_name(), token as isize) } == 0 {
        return Err(io::Error::last_os_error());
    }
    values.insert(window, token);
    Ok(token)
}
fn video_session(
) -> &'static std::sync::Mutex<crate::video::Session<crate::video_process::ProcessEncoder>> {
    static SESSION: std::sync::OnceLock<
        std::sync::Mutex<crate::video::Session<crate::video_process::ProcessEncoder>>,
    > = std::sync::OnceLock::new();
    SESSION.get_or_init(|| {
        let helper = std::env::current_exe()
            .ok()
            .and_then(|p| {
                p.parent()
                    .map(|parent| parent.join("freelane-video-helper.exe"))
            })
            .unwrap_or_default();
        std::sync::Mutex::new(crate::video::Session::new(
            crate::video_process::ProcessEncoder::new(
                std::process::Command::new(helper),
                |target| checked(target).map(|_| ()),
            ),
        ))
    })
}
pub fn video_available() -> bool {
    video_session()
        .lock()
        .is_ok_and(|session| session.available())
}
pub fn video_operation(
    operation: &freelane_agent_protocol::video_control::Operation,
) -> io::Result<freelane_agent_protocol::video_control::Reply> {
    video_session()
        .lock()
        .map_err(|_| invalid("Video session unavailable"))?
        .action(operation)
}
fn checked(target: &WindowTarget) -> io::Result<Handle> {
    let handle = isize::try_from(target.handle).map_err(|_| invalid("Invalid HWND"))?;
    let mut pid = 0;
    unsafe {
        if IsWindow(handle) == 0
            || GetWindowThreadProcessId(handle, &mut pid) == 0
            || pid != target.pid
        {
            return Err(invalid("Window closed or identity changed"));
        }
    }
    if target.incarnation != 0 {
        let values = identities()
            .lock()
            .map_err(|_| invalid("Window identity unavailable"))?;
        if values.get(&handle) != Some(&target.incarnation)
            || unsafe { GetPropW(handle, property_name()) as u64 } != target.incarnation
        {
            return Err(invalid("Window incarnation changed; refresh inventory"));
        }
    }
    if created(pid)? != target.created {
        return Err(invalid("Process identity changed"));
    }
    Ok(handle)
}
struct List {
    guarded: bool,
    pid: u32,
    created: u64,
    windows: Vec<WindowInfo>,
}
extern "system" fn enumerate(window: Handle, data: isize) -> i32 {
    // Callback is synchronous; only the live stack List is passed to EnumWindows.
    let list = unsafe { &mut *(data as *mut List) };
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(window, &mut pid);
        if pid != list.pid || IsWindowVisible(window) == 0 {
            return 1;
        }
        let mut title = [0u16; 512];
        let count = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32);
        if count > 0 {
            let token = if list.guarded {
                match incarnation(window) {
                    Ok(value) => value,
                    Err(_) => return 1,
                }
            } else {
                0
            };
            list.windows.push(WindowInfo {
                target: WindowTarget {
                    incarnation: token,
                    handle: window as u64,
                    pid,
                    created: list.created,
                },
                title: String::from_utf16_lossy(&title[..count as usize]),
            });
        }
    }
    i32::from(list.windows.len() < 16)
}
fn list(pid: u32, guarded: bool) -> io::Result<Outcome> {
    let mut list = List {
        guarded,
        pid,
        created: created(pid)?,
        windows: Vec::new(),
    };
    unsafe {
        EnumWindows(enumerate, &mut list as *mut _ as isize);
    }
    Ok(Outcome::Windows {
        windows: list.windows,
    })
}
struct Find {
    executable: String,
    visited: usize,
    windows: Vec<WindowInfo>,
}
extern "system" fn find_window(window: Handle, data: isize) -> i32 {
    let find = unsafe { &mut *(data as *mut Find) };
    find.visited += 1;
    if find.visited > 4096 || find.windows.len() >= 16 {
        return 0;
    }
    let mut pid = 0;
    unsafe {
        if IsWindowVisible(window) == 0
            || GetWindowThreadProcessId(window, &mut pid) == 0
            || pid == 0
        {
            return 1;
        }
        let process = OpenProcess(0x1000, 0, pid);
        if process == 0 {
            return 1;
        }
        let mut path = [0u16; 2049];
        let mut size = 2049;
        let mut start = FileTime::default();
        let mut end = FileTime::default();
        let mut kernel = FileTime::default();
        let mut user = FileTime::default();
        let known = QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut size) != 0
            && size <= 2048
            && GetProcessTimes(process, &mut start, &mut end, &mut kernel, &mut user) != 0;
        CloseHandle(process);
        if !known
            || String::from_utf16_lossy(&path[..size as usize])
                .replace('/', "\\")
                .to_lowercase()
                != find.executable
        {
            return 1;
        }
        let mut title = [0u16; 512];
        let count = GetWindowTextW(window, title.as_mut_ptr(), 512);
        if count <= 0 {
            return 1;
        }
        let Ok(token) = incarnation(window) else {
            return 1;
        };
        find.windows.push(WindowInfo {
            target: WindowTarget {
                handle: window as u64,
                pid,
                created: (u64::from(start.high) << 32) | u64::from(start.low),
                incarnation: token,
            },
            title: String::from_utf16_lossy(&title[..count as usize]),
        });
    }
    1
}
fn find(executable: &str) -> io::Result<Outcome> {
    let mut find = Find {
        executable: executable.replace('/', "\\").to_lowercase(),
        visited: 0,
        windows: vec![],
    };
    unsafe {
        EnumWindows(find_window, &mut find as *mut Find as isize);
    }
    Ok(Outcome::Windows {
        windows: find.windows,
    })
}
struct Drawing {
    window: Handle,
    dc: Handle,
    memory: Handle,
    bitmap: Handle,
    previous: Handle,
}
impl Drop for Drawing {
    fn drop(&mut self) {
        unsafe {
            if self.previous != 0 && self.memory != 0 {
                SelectObject(self.memory, self.previous);
            }
            if self.bitmap != 0 {
                DeleteObject(self.bitmap);
            }
            if self.memory != 0 {
                DeleteDC(self.memory);
            }
            if self.dc != 0 {
                ReleaseDC(self.window, self.dc);
            }
        }
    }
}
fn capture_inner(target: WindowTarget) -> io::Result<Outcome> {
    let window = checked(&target)?;
    unsafe {
        if IsIconic(window) != 0 {
            return Err(invalid("Restore the guest window before capturing"));
        }
    }
    let mut rect = Rect::default();
    unsafe {
        if GetClientRect(window, &mut rect) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    if w <= 0 || h <= 0 || w > 8192 || h > 8192 || i64::from(w) * i64::from(h) > 16 * 1024 * 1024 {
        return Err(invalid("Window raster exceeds bounds"));
    }
    let mut draw = Drawing {
        window,
        dc: 0,
        memory: 0,
        bitmap: 0,
        previous: 0,
    };
    let mut pixels = vec![0u8; (w as usize) * (h as usize) * 4];
    unsafe {
        draw.dc = GetDC(window);
        if draw.dc == 0 {
            return Err(io::Error::last_os_error());
        }
        draw.memory = CreateCompatibleDC(draw.dc);
        draw.bitmap = CreateCompatibleBitmap(draw.dc, w, h);
        if draw.memory == 0 || draw.bitmap == 0 {
            return Err(io::Error::last_os_error());
        }
        draw.previous = SelectObject(draw.memory, draw.bitmap);
        if draw.previous == 0 || draw.previous == -1 {
            draw.previous = 0;
            return Err(invalid("Cannot select window bitmap"));
        }
        if PrintWindow(window, draw.memory, 1) == 0 {
            return Err(invalid("Application does not provide a PrintWindow frame"));
        }
        SelectObject(draw.memory, draw.previous);
        draw.previous = 0;
        let mut info = BitmapInfo {
            size: 40,
            width: w,
            height: -h,
            planes: 1,
            bits: 32,
            compression: 0,
            image_size: 0,
            x: 0,
            y: 0,
            used: 0,
            important: 0,
            color: 0,
        };
        if GetDIBits(
            draw.dc,
            draw.bitmap,
            0,
            h as u32,
            pixels.as_mut_ptr(),
            &mut info,
            0,
        ) != h
        {
            return Err(invalid("Cannot read complete window bitmap"));
        }
    }
    checked(&target)?;
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }
    let rgba = image::RgbaImage::from_raw(w as u32, h as u32, pixels)
        .ok_or_else(|| invalid("Invalid raster"))?;
    let original = image::DynamicImage::ImageRgba8(rgba);
    for (width, height, quality) in [
        (640, 480, 75),
        (480, 320, 55),
        (320, 240, 40),
        (160, 120, 35),
    ] {
        let rgb = original.thumbnail(width, height).into_rgb8();
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality)
            .encode(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(io::Error::other)?;
        if bytes.len() <= 28000 {
            let hex = bytes.iter().map(|b| format!("{b:02x}")).collect();
            return Ok(Outcome::WindowFrame {
                target,
                width: rgb.width(),
                height: rgb.height(),
                jpeg_hex: hex,
            });
        }
    }
    Err(invalid("Encoded frame exceeds serial protocol bounds"))
}
static CAPTURE_BUSY: AtomicBool = AtomicBool::new(false);
fn capture(target: WindowTarget) -> io::Result<Outcome> {
    if CAPTURE_BUSY.swap(true, Ordering::SeqCst) {
        return Err(invalid("A prior window capture is still pending"));
    }
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                CAPTURE_BUSY.store(false, Ordering::SeqCst);
            }
        }
        let _reset = Reset;
        let _ = sender.send(capture_inner(target));
    });
    receiver
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| invalid("Window capture timed out; guest may be unresponsive"))?
}
fn post(window: Handle, message: u32, wparam: usize, lparam: isize) -> io::Result<()> {
    if unsafe { PostMessageW(window, message, wparam, lparam) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
fn focused(root: Handle) -> Handle {
    unsafe {
        let thread = GetWindowThreadProcessId(root, std::ptr::null_mut());
        let mut info = GuiInfo {
            size: std::mem::size_of::<GuiInfo>() as u32,
            flags: 0,
            active: 0,
            focus: 0,
            capture: 0,
            menu: 0,
            move_size: 0,
            caret: 0,
            caret_rect: Rect::default(),
        };
        if GetGUIThreadInfo(thread, &mut info) != 0
            && info.focus != 0
            && GetAncestor(info.focus, 2) == root
        {
            info.focus
        } else {
            root
        }
    }
}
fn input(action: &Action, target: &WindowTarget) -> io::Result<Outcome> {
    let root = checked(target)?;
    match action {
        Action::WindowClick { x, y, .. } => {
            let mut rect = Rect::default();
            unsafe {
                if GetClientRect(root, &mut rect) == 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            let mut point = Point {
                x: i32::from(*x) * (rect.right - rect.left - 1).max(0) / 32767,
                y: i32::from(*y) * (rect.bottom - rect.top - 1).max(0) / 32767,
            };
            let mut window = root;
            unsafe {
                for _ in 0..32 {
                    let child = ChildWindowFromPointEx(window, point, 7);
                    if child == 0 || child == window {
                        break;
                    }
                    if ClientToScreen(window, &mut point) == 0
                        || ScreenToClient(child, &mut point) == 0
                    {
                        return Err(invalid("Cannot map click"));
                    }
                    window = child;
                }
            }
            let position = ((point.y as u16 as u32) << 16) | (point.x as u16 as u32);
            post(window, 0x201, 1, position as isize)?;
            post(window, 0x202, 0, position as isize)?;
        }
        Action::WindowText { text, .. } => {
            for unit in text.encode_utf16() {
                post(focused(root), 0x102, unit as usize, 1)?;
            }
        }
        Action::WindowKey { key, .. } => {
            let scan = unsafe { MapVirtualKeyW(u32::from(*key), 0) };
            let flags = 1
                | ((scan as isize) << 16)
                | if (33..=40).contains(key) || *key == 46 {
                    1 << 24
                } else {
                    0
                };
            post(focused(root), 0x100, *key as usize, flags)?;
            post(
                focused(root),
                0x101,
                *key as usize,
                flags | (1 << 30) | (1 << 31),
            )?;
        }
        _ => return Err(invalid("Unsupported input action")),
    }
    Ok(Outcome::WindowInputAccepted)
}
pub fn handle(action: &Action) -> io::Result<Outcome> {
    match action {
        Action::WindowPointer { .. }
        | Action::WindowShortcut { .. }
        | Action::WindowResize { .. } => input::handle(action),
        Action::Windows { pid } => list(*pid, false),
        Action::WindowInventory { pid } => list(*pid, true),
        Action::FindAppWindows { executable } => find(executable),
        Action::CaptureWindow { target } => capture(target.clone()),
        Action::WindowClick { target, .. }
        | Action::WindowText { target, .. }
        | Action::WindowKey { target, .. } => input(action, target),
        _ => Err(invalid("Unsupported window action")),
    }
}
