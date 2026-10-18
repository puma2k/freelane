//! Versioned, bounded guest RPC. Launch requests contain an executable and literal arguments, never shell text.
pub mod clipboard;
pub mod clipboard_sync;
pub mod export;
pub mod export_catalog;
pub mod transfer_folders;
pub mod video;
pub mod window_input;
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Read, Write};
pub const VERSION: u16 = 1;
/// Optional window extension; v1 ping/launch remain compatible.
pub const WINDOW_VERSION: u16 = 2;
/// Read-only extensible capability discovery; existing mutation protocols stay unchanged.
pub const CAPABILITY_VERSION: u16 = 3;
pub const CLIPBOARD_VERSION: u16 = 4;
pub const TRANSFER_VERSION: u16 = 5;
pub const CLIPBOARD_SYNC_VERSION: u16 = 6;
pub const SHARE_STATUS_VERSION: u16 = 7;
pub const CATALOG_VERSION: u16 = 8;
pub const WINDOW_IDENTITY_VERSION: u16 = 9;
pub const MAX_CLIPBOARD_TEXT: usize = 8192;
pub const MAX_FRAME: usize = 64 * 1024;
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u16,
    pub id: u64,
    pub action: Action,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    TransferFolders {
        reconnect: bool,
    },
    ExportCatalog,
    Export {
        operation: export::Operation,
    },
    WindowPointer {
        target: WindowTarget,
        gesture: window_input::Gesture,
    },
    WindowShortcut {
        target: WindowTarget,
        key: u16,
        modifiers: u8,
    },
    WindowResize {
        target: WindowTarget,
        width: u16,
        height: u16,
    },
    WindowInventory {
        pid: u32,
    },
    FindAppWindows {
        executable: String,
    },
    AppCatalog {
        cursor: u16,
    },
    ShareStatus,
    ClipboardSyncGet,
    ClipboardSyncSet {
        expected: ClipboardSnapshot,
        update: ClipboardSnapshot,
    },
    Transfer {
        operation: TransferAction,
    },
    ClipboardGet,
    ClipboardSet {
        source: String,
        sequence: u64,
        text: String,
    },
    Ping,
    Windows {
        pid: u32,
    },
    CaptureWindow {
        target: WindowTarget,
    },
    WindowClick {
        target: WindowTarget,
        x: u16,
        y: u16,
    },
    WindowText {
        target: WindowTarget,
        text: String,
    },
    WindowKey {
        target: WindowTarget,
        key: u16,
    },
    Launch {
        executable: String,
        arguments: Vec<String>,
    },
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum TransferAction {
    Open {
        token: String,
    },
    Begin {
        name: String,
        size: u64,
        sha256: String,
    },
    Chunk {
        token: String,
        offset: u64,
        data_hex: String,
    },
    Finish {
        token: String,
    },
    Cancel {
        token: String,
    },
}
impl TransferAction {
    pub fn validate(&self) -> io::Result<()> {
        let hex = |s: &str, len: usize| s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit());
        match self {
            Self::Begin { name, size, sha256 }
                if name.is_empty()
                    || name.len() > 240
                    || name.chars().any(char::is_control)
                    || *size > 64 * 1024 * 1024 * 1024
                    || !hex(sha256, 64) =>
            {
                return Err(invalid("Invalid transfer metadata"))
            }
            Self::Chunk {
                token, data_hex, ..
            } if !hex(token, 32)
                || data_hex.is_empty()
                || data_hex.len() > 32768
                || data_hex.len() % 2 != 0
                || !data_hex.bytes().all(|b| b.is_ascii_hexdigit()) =>
            {
                return Err(invalid("Invalid transfer chunk"))
            }
            Self::Finish { token } | Self::Cancel { token } | Self::Open { token }
                if !hex(token, 32) =>
            {
                return Err(invalid("Invalid transfer identity"))
            }
            _ => {}
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AppEntry {
    pub name: String,
    pub executable: String,
}
impl AppEntry {
    pub fn validate(&self) -> io::Result<()> {
        let p = self.executable.as_bytes();
        let absolute = p.first() == Some(&b'/')
            || (p.len() > 3
                && p[0].is_ascii_alphabetic()
                && p[1] == b':'
                && b"/\\".contains(&p[2]));
        if self.name.is_empty()
            || self.name.len() > 128
            || self.name.chars().any(char::is_control)
            || !absolute
            || self.executable.len() > 2048
            || self.executable.chars().any(char::is_control)
        {
            return Err(invalid("Invalid application metadata"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AppPage {
    pub apps: Vec<AppEntry>,
    pub next: Option<u16>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SharedMount {
    pub path: String,
    pub readonly: bool,
}
impl SharedMount {
    pub fn validate(&self) -> io::Result<()> {
        if !self.path.starts_with('/')
            || self.path.len() > 1024
            || self.path.chars().any(char::is_control)
        {
            return Err(invalid("Invalid guest mount path"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WindowTarget {
    pub handle: u64,
    pub pid: u32,
    pub created: u64,
    #[serde(default, skip_serializing_if = "zero_incarnation")]
    pub incarnation: u64,
}
fn zero_incarnation(value: &u64) -> bool {
    *value == 0
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WindowInfo {
    pub target: WindowTarget,
    pub title: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Feature {
    pub name: String,
    pub protocol: u16,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentInfo {
    pub agent_version: String,
    pub os: String,
    pub arch: String,
    pub protocols: Vec<u16>,
    pub features: Vec<Feature>,
    pub max_frame: u32,
}
impl AgentInfo {
    pub fn validate(&self) -> io::Result<()> {
        let text = |value: &str| {
            !value.is_empty()
                && value.len() <= 64
                && value.is_ascii()
                && !value.chars().any(char::is_control)
        };
        if !text(&self.agent_version)
            || !text(&self.os)
            || !text(&self.arch)
            || self.max_frame != MAX_FRAME as u32
            || self.protocols.is_empty()
            || self.protocols.len() > 16
            || !self.protocols.contains(&CAPABILITY_VERSION)
            || self.protocols.contains(&0)
            || self.features.len() > 32
        {
            return Err(invalid("Invalid guest capability metadata"));
        }
        let mut protocols = std::collections::HashSet::new();
        if self
            .protocols
            .iter()
            .any(|version| !protocols.insert(*version))
        {
            return Err(invalid("Duplicate guest protocol versions"));
        }
        let mut names = std::collections::HashSet::new();
        for feature in &self.features {
            if feature.name.is_empty()
                || feature.name.len() > 48
                || !feature
                    .name
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
                || !names.insert(&feature.name)
                || !self.protocols.contains(&feature.protocol)
            {
                return Err(invalid("Invalid guest capability feature"));
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClipboardSnapshot {
    pub source: String,
    pub sequence: u64,
    pub text: String,
}
impl ClipboardSnapshot {
    pub fn validate(&self) -> io::Result<()> {
        if self.source.len() != 32
            || !self.source.bytes().all(|b| b.is_ascii_hexdigit())
            || self.sequence == 0
            || self.text.len() > MAX_CLIPBOARD_TEXT
            || self.text.contains('\0')
        {
            return Err(invalid("Invalid clipboard metadata or text limit"));
        }
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub version: u16,
    pub id: u64,
    pub outcome: Outcome,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    TransferFolders {
        report: transfer_folders::Report,
    },
    ExportCatalog {
        page: export_catalog::Page,
    },
    ExportReady {
        token: String,
        size: u64,
    },
    ExportFinished {
        sha256: String,
    },
    ExportChunk {
        offset: u64,
        data_hex: String,
    },
    ExportClosed,
    AppCatalog {
        page: AppPage,
    },
    ShareMounts {
        mounts: Vec<SharedMount>,
    },
    ClipboardSync {
        instance: String,
        snapshot: ClipboardSnapshot,
    },
    ClipboardConflict,
    TransferReady {
        token: String,
    },
    TransferProgress {
        received: u64,
    },
    TransferComplete,
    DocumentOpened,
    TransferCanceled,
    Clipboard {
        snapshot: ClipboardSnapshot,
    },
    ClipboardAccepted {
        source: String,
        sequence: u64,
    },
    AgentInfo {
        info: AgentInfo,
    },
    Pong {
        os: String,
        arch: String,
    },
    Capabilities {
        window_snapshots: bool,
    },
    Windows {
        windows: Vec<WindowInfo>,
    },
    WindowFrame {
        target: WindowTarget,
        width: u32,
        height: u32,
        jpeg_hex: String,
    },
    WindowInputAccepted,
    Launched {
        pid: u32,
    },
    Failed {
        message: String,
    },
}
impl Request {
    pub fn validate(&self) -> io::Result<()> {
        if ![
            VERSION,
            WINDOW_VERSION,
            CAPABILITY_VERSION,
            CLIPBOARD_VERSION,
            TRANSFER_VERSION,
            CLIPBOARD_SYNC_VERSION,
            SHARE_STATUS_VERSION,
            CATALOG_VERSION,
            WINDOW_IDENTITY_VERSION,
            window_input::VERSION,
            export::VERSION,
            export_catalog::VERSION,
            transfer_folders::VERSION,
        ]
        .contains(&self.version)
        {
            return Err(invalid("Unsupported guest protocol version"));
        }
        if self.id == 0 {
            return Err(invalid("Request ID must not be zero"));
        }
        if self.version == transfer_folders::VERSION {
            return if matches!(self.action, Action::TransferFolders { .. }) {
                Ok(())
            } else {
                Err(invalid(
                    "Protocol v13 only supports file folder state/reconnect",
                ))
            };
        }
        if matches!(self.action, Action::TransferFolders { .. }) {
            return Err(invalid("File folder state requires protocol v13"));
        }
        if self.version == export_catalog::VERSION {
            return if matches!(self.action, Action::ExportCatalog) {
                Ok(())
            } else {
                Err(invalid("Protocol v12 only supports outbox listing"))
            };
        }
        if matches!(self.action, Action::ExportCatalog) {
            return Err(invalid("Outbox listing requires protocol v12"));
        }
        if self.version == export::VERSION {
            return match &self.action {
                Action::Export { operation } => operation.validate(),
                _ => Err(invalid("Protocol v11 only supports file export")),
            };
        }
        if matches!(self.action, Action::Export { .. }) {
            return Err(invalid("File export requires protocol v11"));
        }
        if self.version == window_input::VERSION {
            return window_input::validate(&self.action);
        }
        if window_input::target(&self.action).is_some() {
            return Err(invalid("Extended window input requires protocol v10"));
        }
        if self.version == WINDOW_IDENTITY_VERSION {
            return match &self.action {
                Action::WindowInventory { pid } if *pid > 0 => Ok(()),
                Action::FindAppWindows { executable } => {
                    let p = executable.as_bytes();
                    if p.len() >= 7
                        && p.len() <= 2048
                        && p[0].is_ascii_alphabetic()
                        && p[1] == b':'
                        && b"/\\".contains(&p[2])
                        && executable.to_ascii_lowercase().ends_with(".exe")
                        && !executable.chars().any(char::is_control)
                        && !executable.contains('"')
                    {
                        Ok(())
                    } else {
                        Err(invalid("Expected absolute local Windows executable"))
                    }
                }
                Action::CaptureWindow { target }
                | Action::WindowClick { target, .. }
                | Action::WindowText { target, .. }
                | Action::WindowKey { target, .. }
                    if target.incarnation > 0 =>
                {
                    Request {
                        version: WINDOW_VERSION,
                        id: self.id,
                        action: match &self.action {
                            Action::CaptureWindow { target } => Action::CaptureWindow {
                                target: target.clone(),
                            },
                            Action::WindowClick { target, x, y } => Action::WindowClick {
                                target: target.clone(),
                                x: *x,
                                y: *y,
                            },
                            Action::WindowText { target, text } => Action::WindowText {
                                target: target.clone(),
                                text: text.clone(),
                            },
                            Action::WindowKey { target, key } => Action::WindowKey {
                                target: target.clone(),
                                key: *key,
                            },
                            _ => unreachable!(),
                        },
                    }
                    .validate()
                }
                _ => Err(invalid("Protocol v9 requires observed window incarnation")),
            };
        }
        if matches!(
            self.action,
            Action::WindowInventory { .. } | Action::FindAppWindows { .. }
        ) {
            return Err(invalid("Window identity discovery requires protocol v9"));
        }
        if self.version == CAPABILITY_VERSION && !matches!(self.action, Action::Ping) {
            return Err(invalid("Protocol v3 is read-only capability discovery"));
        }
        if let Action::AppCatalog { cursor } = self.action {
            return if self.version == CATALOG_VERSION && cursor <= 512 {
                Ok(())
            } else {
                Err(invalid("Invalid read-only catalog request"))
            };
        }
        if self.version == CATALOG_VERSION {
            return Err(invalid("Protocol v8 is read-only application catalog"));
        }
        if matches!(self.action, Action::ShareStatus) {
            return if self.version == SHARE_STATUS_VERSION {
                Ok(())
            } else {
                Err(invalid("Share status requires protocol v7"))
            };
        }
        if self.version == SHARE_STATUS_VERSION {
            return Err(invalid("Protocol v7 is read-only mount status"));
        }
        if matches!(
            self.action,
            Action::ClipboardSyncGet | Action::ClipboardSyncSet { .. }
        ) {
            if self.version != CLIPBOARD_SYNC_VERSION {
                return Err(invalid("Automatic clipboard requires protocol v6"));
            }
            if let Action::ClipboardSyncSet { expected, update } = &self.action {
                expected.validate()?;
                update.validate()?;
            }
            return Ok(());
        }
        if self.version == CLIPBOARD_SYNC_VERSION {
            return Err(invalid(
                "Protocol v6 only supports conditional text clipboard",
            ));
        }
        if let Action::Transfer { operation } = &self.action {
            if self.version != TRANSFER_VERSION {
                return Err(invalid("Transfers require protocol v5"));
            }
            return operation.validate();
        }
        if self.version == TRANSFER_VERSION {
            return Err(invalid("Protocol v5 only supports file transfer"));
        }
        let clipboard = matches!(
            self.action,
            Action::ClipboardGet | Action::ClipboardSet { .. }
        );
        if clipboard && self.version != CLIPBOARD_VERSION {
            return Err(invalid("Clipboard actions require protocol v4"));
        }
        if self.version == CLIPBOARD_VERSION && !clipboard {
            return Err(invalid("Protocol v4 only supports text clipboard"));
        }
        let window_action = !matches!(
            self.action,
            Action::Ping
                | Action::Launch { .. }
                | Action::ClipboardGet
                | Action::ClipboardSet { .. }
        );
        if window_action && self.version != WINDOW_VERSION {
            return Err(invalid("Window actions require protocol v2"));
        }
        if let Action::ClipboardSet {
            source,
            sequence,
            text,
        } = &self.action
        {
            ClipboardSnapshot {
                source: source.clone(),
                sequence: *sequence,
                text: text.clone(),
            }
            .validate()?;
        }
        match &self.action {
            Action::Windows { pid } if *pid == 0 => return Err(invalid("Invalid process ID")),
            Action::CaptureWindow { target }
            | Action::WindowClick { target, .. }
            | Action::WindowText { target, .. }
            | Action::WindowKey { target, .. }
                if target.pid == 0 || target.handle == 0 || target.created == 0 =>
            {
                return Err(invalid("Invalid window identity"))
            }
            Action::WindowClick { x, y, .. } if *x > 32767 || *y > 32767 => {
                return Err(invalid("Invalid click coordinate"))
            }
            Action::WindowText { text, .. } if text.len() > 1024 || text.contains('\0') => {
                return Err(invalid("Window text exceeds 1024 bytes"))
            }
            Action::WindowKey { key, .. }
                if ![8, 9, 13, 27, 33, 34, 35, 36, 37, 38, 39, 40, 46].contains(key) =>
            {
                return Err(invalid("Unsupported window key"))
            }
            _ => {}
        }
        if let Action::Launch {
            executable,
            arguments,
        } = &self.action
        {
            if executable.is_empty() || executable.len() > 4096 || executable.contains('\0') {
                return Err(invalid("Invalid executable path"));
            }
            if arguments.len() > 128 || arguments.iter().any(|v| v.len() > 4096 || v.contains('\0'))
            {
                return Err(invalid("Invalid argument list"));
            }
        }
        Ok(())
    }
}
pub fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
pub fn read_frame<T: serde::de::DeserializeOwned>(reader: &mut impl BufRead) -> io::Result<T> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take((MAX_FRAME + 1) as u64)
        .read_until(b'\n', &mut bytes)?;
    if bytes.is_empty() {
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
    }
    if bytes.len() > MAX_FRAME {
        return Err(invalid("Guest message exceeds 64 KiB"));
    }
    if bytes.last() != Some(&b'\n') {
        return Err(invalid("Truncated guest message"));
    }
    serde_json::from_slice(&bytes).map_err(|_| invalid("Invalid guest message JSON"))
}
pub fn write_frame(writer: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    let mut bytes =
        serde_json::to_vec(value).map_err(|_| invalid("Cannot encode guest message"))?;
    bytes.push(b'\n');
    if bytes.len() > MAX_FRAME {
        return Err(invalid("Guest message exceeds 64 KiB"));
    }
    writer.write_all(&bytes)?;
    writer.flush()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_arguments_round_trip_without_shell_interpretation() {
        let request = Request {
            version: VERSION,
            id: 7,
            action: Action::Launch {
                executable: "C:\\Windows\\System32\\notepad.exe".into(),
                arguments: vec!["file with spaces.txt".into(), "$(echo secret) & x".into()],
            },
        };
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &request).unwrap();
        let decoded: Request = read_frame(&mut bytes.as_slice()).unwrap();
        assert_eq!(request, decoded);
        decoded.validate().unwrap();
    }
    #[test]
    fn incompatible_or_malformed_requests_are_rejected() {
        let mut request = Request {
            version: 99,
            id: 1,
            action: Action::Ping,
        };
        assert!(request.validate().is_err());
        request.version = VERSION;
        request.id = 0;
        assert!(request.validate().is_err());
        assert!(read_frame::<Request>(&mut b"{}\n".as_slice()).is_err());
        assert!(read_frame::<Request>(
            &mut b"{\"version\":1,\"id\":1,\"action\":{\"type\":\"ping\"},\"unexpected\":1}\n"
                .as_slice()
        )
        .is_err());
    }
    #[test]
    fn oversize_and_unterminated_frames_are_rejected() {
        assert!(read_frame::<Request>(&mut vec![b'x'; MAX_FRAME + 1].as_slice()).is_err());
        assert!(read_frame::<Request>(&mut b"{}".as_slice()).is_err());
        let request = Request {
            version: VERSION,
            id: 1,
            action: Action::Launch {
                executable: "x".repeat(MAX_FRAME),
                arguments: vec![],
            },
        };
        let mut bytes = Vec::new();
        assert!(write_frame(&mut bytes, &request).is_err());
        assert!(bytes.is_empty());
    }
}

#[cfg(test)]
mod capability_tests {
    use super::*;
    fn info() -> AgentInfo {
        AgentInfo {
            agent_version: "0.1.0".into(),
            os: "linux".into(),
            arch: "aarch64".into(),
            protocols: vec![1, 2, 3],
            features: vec![Feature {
                name: "launch".into(),
                protocol: 1,
            }],
            max_frame: MAX_FRAME as u32,
        }
    }
    #[test]
    fn discovery_metadata_bounds_reject_controls_duplicates_and_unadvertised_protocols() {
        let original = info();
        assert!(original.validate().is_ok());
        let mut value = original.clone();
        value.agent_version = "x".repeat(65);
        assert!(value.validate().is_err());
        let mut value = original.clone();
        value.os = "linux\nprivate".into();
        assert!(value.validate().is_err());
        let mut value = original.clone();
        value.protocols.push(3);
        assert!(value.validate().is_err());
        let mut value = original.clone();
        value.features.push(value.features[0].clone());
        assert!(value.validate().is_err());
        let mut value = original.clone();
        value.features[0].protocol = 4;
        assert!(value.validate().is_err());
        let mut value = original;
        value.max_frame += 1;
        assert!(value.validate().is_err());
    }
    #[test]
    fn future_feature_protocols_can_be_discovered_without_enabling_new_request_actions() {
        let mut value = info();
        value.protocols.push(4);
        value.features.push(Feature {
            name: "future_feature".into(),
            protocol: 4,
        });
        assert!(value.validate().is_ok());
        assert!(Request {
            version: 4,
            id: 1,
            action: Action::Ping
        }
        .validate()
        .is_err());
    }
}

#[cfg(test)]
mod clipboard_tests {
    use super::*;
    fn wire(version: u16, text: &str) -> Vec<u8> {
        let value = serde_json::json!({"version":version,"id":9,"action":{"type":"clipboard_set","source":"0123456789abcdef0123456789abcdef","sequence":1,"text":text}});
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        bytes
    }
    #[test]
    fn unicode_clipboard_has_its_own_bounded_protocol() {
        let request: Request =
            read_frame(&mut wire(4, "Xin chào anh 👋\nTiếng Việt").as_slice()).unwrap();
        request.validate().unwrap();
        let mut bytes = vec![];
        write_frame(&mut bytes, &request).unwrap();
        let decoded: Request = read_frame(&mut bytes.as_slice()).unwrap();
        assert_eq!(decoded, request);
        let legacy: Request = read_frame(&mut wire(1, "hello").as_slice()).unwrap();
        assert!(legacy.validate().is_err());
        let large: Request = read_frame(&mut wire(4, &"x".repeat(8193)).as_slice()).unwrap();
        assert!(large.validate().is_err());
        let controls: Request = read_frame(&mut wire(4, &"\u{1}".repeat(8192)).as_slice()).unwrap();
        controls.validate().unwrap();
        let mut bytes = vec![];
        write_frame(&mut bytes, &controls).unwrap();
        assert!(bytes.len() <= MAX_FRAME);
    }
}

#[cfg(test)]
mod export_contract_tests {
    use super::*;
    #[test]
    fn export_requires_v11_and_a_single_portable_filename() {
        let valid = r#"{"version":11,"id":1,"action":{"type":"export","operation":{"operation":"begin","name":"Tài liệu.txt"}}}"#;
        let r: Request = serde_json::from_str(valid).expect("export wire action");
        r.validate().unwrap();
        for version in [1, 5, 10] {
            let mut r: Request = serde_json::from_str(valid).unwrap();
            r.version = version;
            assert!(r.validate().is_err());
        }
        for name in [
            "../secret",
            "a/b",
            "a\\b",
            "CON.txt",
            "a:ads",
            ".hidden",
            "a.",
        ] {
            let mut v: serde_json::Value = serde_json::from_str(valid).unwrap();
            v["action"]["operation"]["name"] = name.into();
            let r: Request = serde_json::from_value(v).unwrap();
            assert!(r.validate().is_err(), "{name}");
        }
    }
}
