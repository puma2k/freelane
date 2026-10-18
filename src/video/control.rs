//! Capability/port negotiation never restarts a VM or retries uncertain mutations.
use crate::{Error, Result};
use freelane_agent_protocol::{video::Binding, video_control::VERSION};
use std::{
    fs,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::Path,
};
pub fn fresh_binding(stream: u64, generation: u64) -> Result<Binding> {
    let mut binding = Binding {
        nonce: [1; 16],
        stream,
        generation,
    };
    binding.validate().map_err(|e| Error(e.to_string()))?;
    for _ in 0..4 {
        getrandom::fill(&mut binding.nonce)
            .map_err(|_| Error("Video session entropy unavailable".into()))?;
        if binding.nonce != [0; 16] {
            return Ok(binding);
        }
    }
    Err(Error("Video session entropy unavailable".into()))
}
pub fn require_capability(config: &Path) -> Result<()> {
    let info = crate::agent_capabilities::inspect(config)?;
    if !info.capability_list_complete || info.supports("window_video", VERSION) != Some(true) {
        return Err(Error(
            "Window video unavailable; keep snapshots or install a compatible video guest backend"
                .into(),
        ));
    }
    let directory = crate::qemu::runtime_dir(config)?;
    let socket=fs::symlink_metadata(directory.join("video.sock")).map_err(|_|Error("Video port unavailable; restart the VM when convenient to add it. The current VM was kept running".into()))?;
    let owner = fs::metadata(config)
        .map_err(|e| Error(e.to_string()))?
        .uid();
    if !socket.file_type().is_socket() || socket.uid() != owner {
        return Err(Error(
            "Video requires the VM owner's private video socket".into(),
        ));
    }
    Ok(())
}
