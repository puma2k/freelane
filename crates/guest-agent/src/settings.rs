//! Explicit per-user installer selections. No inferred or automatically created folders.
use serde::Deserialize;
use std::{
    fs::OpenOptions,
    io::{self, Read},
    path::{Path, PathBuf},
};
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub inbox: Option<PathBuf>,
    pub outbox: Option<PathBuf>,
}
#[derive(Deserialize)]
struct Manifest {
    schema: u16,
    #[serde(default)]
    transfer: Settings,
    #[serde(default)]
    video_sha256: Option<String>,
}
impl Settings {
    pub fn load(path: &Path) -> io::Result<Self> {
        let Some(manifest) = read_manifest(path)? else {
            return Ok(Self::default());
        };
        for path in [&manifest.transfer.inbox, &manifest.transfer.outbox]
            .into_iter()
            .flatten()
        {
            let text = path.to_str().ok_or(io::ErrorKind::InvalidData)?;
            if !path.is_absolute() || text.len() > 2048 || text.chars().any(char::is_control) {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        Ok(manifest.transfer)
    }
}

fn read_manifest(path: &Path) -> io::Result<Option<Manifest>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let metadata = file.metadata()?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
    }
    if !metadata.is_file() || metadata.len() > 4096 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|_| io::ErrorKind::InvalidData)?;
    if manifest.schema != 1 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(Some(manifest))
}

/// Installer pins helpers to immutable content-named siblings, retaining the old
/// helper for verified rollback. An unmanaged legacy sibling remains supported.
pub fn video_helper(executable: &Path) -> io::Result<PathBuf> {
    use sha2::{Digest, Sha256};
    let parent = executable.parent().ok_or(io::ErrorKind::InvalidInput)?;
    let manifest = read_manifest(&parent.join("agent-install.json"))?;
    let Some(hash) = manifest.and_then(|m| m.video_sha256) else {
        return Ok(parent.join("freelane-video-helper.exe"));
    };
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let path = parent.join(format!("freelane-video-helper-{hash}.exe"));
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000);
    }
    let file = options.open(&path)?;
    let metadata = file.metadata()?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(io::ErrorKind::InvalidData.into());
        }
    }
    if !metadata.is_file() || metadata.len() > 32 * 1024 * 1024 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut digest = Sha256::new();
    let mut limited = file.take(32 * 1024 * 1024 + 1);
    let mut count = 0u64;
    let mut buffer = [0; 8192];
    loop {
        let n = limited.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        digest.update(&buffer[..n]);
    }
    if count > 32 * 1024 * 1024 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let actual = digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if actual != hash {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(path)
}
