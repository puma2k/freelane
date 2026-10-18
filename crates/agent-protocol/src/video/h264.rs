//! Structural bounds only: the native decoder still validates H.264 syntax/dimensions.
use super::invalid;
use std::io;
pub(super) fn configuration(bytes: &[u8]) -> io::Result<()> {
    if bytes.len() < 7 || bytes[..2] != [1, 66] || bytes[4] != 255 || bytes[5] != 225 {
        return Err(invalid("Unsupported H264 configuration"));
    }
    let mut at = 6;
    for (kind, min_len) in [(7, 4), (8, 1)] {
        if kind == 8 {
            if bytes.get(at) != Some(&1) {
                return Err(invalid("Expected one H264 PPS"));
            }
            at += 1;
        }
        let length_bytes = bytes
            .get(at..at + 2)
            .ok_or_else(|| invalid("Truncated H264 parameter length"))?;
        let length = u16::from_be_bytes(length_bytes.try_into().unwrap()) as usize;
        at += 2;
        let nal = bytes
            .get(at..at + length)
            .ok_or_else(|| invalid("Truncated H264 parameter"))?;
        if length < min_len
            || nal[0] & 0x80 != 0
            || nal[0] & 31 != kind
            || (kind == 7 && nal[1] != 66)
        {
            return Err(invalid("Invalid H264 parameter"));
        }
        at += length;
    }
    if at != bytes.len() {
        return Err(invalid("Trailing H264 configuration data"));
    }
    Ok(())
}
pub(super) fn access_unit(bytes: &[u8], idr: bool) -> io::Result<()> {
    let mut at = 0;
    let mut count = 0;
    let mut key = false;
    let mut dependent = false;
    while at < bytes.len() {
        count += 1;
        if count > 256 {
            return Err(invalid("Too many H264 NAL units"));
        }
        let length_bytes = bytes
            .get(at..at + 4)
            .ok_or_else(|| invalid("Truncated H264 NAL length"))?;
        let length = u32::from_be_bytes(length_bytes.try_into().unwrap()) as usize;
        at += 4;
        // checked_add also protects 32-bit builds with hostile length prefixes.
        let end = at
            .checked_add(length)
            .ok_or_else(|| invalid("Invalid H264 NAL length"))?;
        let nal = bytes
            .get(at..end)
            .filter(|n| !n.is_empty())
            .ok_or_else(|| invalid("Truncated/empty H264 NAL"))?;
        if nal[0] & 0x80 != 0 {
            return Err(invalid("Invalid H264 NAL header"));
        }
        match nal[0] & 31 {
            1 => dependent = true,
            5 => key = true,
            6 | 9 | 12 => {}
            _ => return Err(invalid("Unsupported H264 NAL type")),
        };
        at = end;
    }
    if (!key && !dependent) || (key && dependent) || key != idr {
        return Err(invalid("H264 IDR flag does not match slices"));
    }
    Ok(())
}
