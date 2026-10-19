//! Private native-helper setup and owned process lifecycle.
use freelane_agent_protocol::video_control::Parameters;
use std::io;
pub fn setup_bytes(p: &Parameters) -> io::Result<[u8; 80]> {
    p.validate()?;
    let mut b = [0; 80];
    b[..8].copy_from_slice(b"FLW1\0P\0\0");
    b[8..16].copy_from_slice(&p.target.handle.to_be_bytes());
    b[16..20].copy_from_slice(&p.target.pid.to_be_bytes());
    b[24..32].copy_from_slice(&p.target.created.to_be_bytes());
    b[32..40].copy_from_slice(&p.target.incarnation.to_be_bytes());
    b[40..56].copy_from_slice(&p.binding.nonce);
    b[56..64].copy_from_slice(&p.binding.stream.to_be_bytes());
    b[64..72].copy_from_slice(&p.binding.generation.to_be_bytes());
    b[72..74].copy_from_slice(&p.width.to_be_bytes());
    b[74..76].copy_from_slice(&p.height.to_be_bytes());
    b[76] = p.fps;
    Ok(b)
}
