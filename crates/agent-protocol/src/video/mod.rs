//! Independently versioned baseline H.264 transport; does not authorize capture.
mod h264;
mod receiver;
pub use receiver::Receiver;
use std::io::{self, Read, Write};
pub const HEADER_SIZE: usize = 64;
pub const MAX_ACCESS_UNIT: usize = 2 * 1024 * 1024;
pub const MAX_CONFIGURATION: usize = 64 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub nonce: [u8; 16],
    pub stream: u64,
    pub generation: u64,
}
impl Binding {
    pub fn validate(&self) -> io::Result<()> {
        if self.nonce == [0; 16] || self.stream == 0 || self.generation == 0 {
            return Err(invalid("Invalid video binding"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Configuration,
    AccessUnit { idr: bool },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub binding: Binding,
    pub kind: Kind,
    pub width: u16,
    pub height: u16,
    pub sequence: u64,
    pub pts_us: u64,
    pub payload: Vec<u8>,
}
pub(super) fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
impl Record {
    fn validate_header(&self, length: usize) -> io::Result<()> {
        self.binding.validate()?;
        if self.width == 0
            || self.height == 0
            || self.width > 1920
            || self.height > 1080
            || self.width % 2 != 0
            || self.height % 2 != 0
        {
            return Err(invalid("Invalid video dimensions"));
        }
        let limit = match self.kind {
            Kind::Configuration => {
                if self.sequence != 0 || self.pts_us != 0 {
                    return Err(invalid("Invalid video configuration sequence"));
                }
                MAX_CONFIGURATION
            }
            Kind::AccessUnit { .. } => {
                if self.sequence == 0 {
                    return Err(invalid("Invalid video frame sequence"));
                }
                MAX_ACCESS_UNIT
            }
        };
        if length == 0 || length > limit {
            return Err(invalid("Video payload exceeds bounds"));
        }
        Ok(())
    }
    pub fn validate(&self) -> io::Result<()> {
        self.validate_header(self.payload.len())?;
        match self.kind {
            Kind::Configuration => h264::configuration(&self.payload),
            Kind::AccessUnit { idr } => h264::access_unit(&self.payload, idr),
        }
    }
}
pub fn read_record(reader: &mut impl Read) -> io::Result<Option<Record>> {
    let mut h = [0u8; HEADER_SIZE];
    loop {
        match reader.read(&mut h[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    reader.read_exact(&mut h[1..])?;
    if &h[..4] != b"FLV1" || h[6] != 66 || h[7] != 0 {
        return Err(invalid("Unsupported video format"));
    }
    let kind = match (h[4], h[5]) {
        (1, 0) => Kind::Configuration,
        (2, 0) => Kind::AccessUnit { idr: false },
        (2, 1) => Kind::AccessUnit { idr: true },
        _ => return Err(invalid("Invalid video record kind/flags")),
    };
    let u64_at = |start| u64::from_be_bytes(h[start..start + 8].try_into().unwrap());
    let length = u32::from_be_bytes(h[8..12].try_into().unwrap()) as usize;
    let mut r = Record {
        binding: Binding {
            nonce: h[16..32].try_into().unwrap(),
            stream: u64_at(32),
            generation: u64_at(40),
        },
        kind,
        width: u16::from_be_bytes(h[12..14].try_into().unwrap()),
        height: u16::from_be_bytes(h[14..16].try_into().unwrap()),
        sequence: u64_at(48),
        pts_us: u64_at(56),
        payload: vec![],
    };
    r.validate_header(length)?; // Reject before allocation or reading payload.
    r.payload = vec![0; length];
    reader.read_exact(&mut r.payload)?;
    r.validate()?;
    Ok(Some(r))
}
pub fn write_record(writer: &mut impl Write, r: &Record) -> io::Result<()> {
    r.validate()?;
    let mut h = [0u8; HEADER_SIZE];
    h[..4].copy_from_slice(b"FLV1");
    (h[4], h[5]) = match r.kind {
        Kind::Configuration => (1, 0),
        Kind::AccessUnit { idr } => (2, u8::from(idr)),
    };
    h[6] = 66;
    h[8..12].copy_from_slice(&(r.payload.len() as u32).to_be_bytes());
    h[12..14].copy_from_slice(&r.width.to_be_bytes());
    h[14..16].copy_from_slice(&r.height.to_be_bytes());
    h[16..32].copy_from_slice(&r.binding.nonce);
    for (start, value) in [
        (32, r.binding.stream),
        (40, r.binding.generation),
        (48, r.sequence),
        (56, r.pts_us),
    ] {
        h[start..start + 8].copy_from_slice(&value.to_be_bytes());
    }
    writer.write_all(&h)?;
    writer.write_all(&r.payload)
}
