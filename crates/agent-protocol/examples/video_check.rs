//! Offline generated-fixture validator, never a source of capture authorization.
use freelane_agent_protocol::video::{read_record, Kind, Receiver};
use std::{
    fs::File,
    io::{self, BufReader},
    path::Path,
};
fn validate(path: &Path) -> io::Result<()> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > 4 * 1024 * 1024 {
        return Err(io::Error::other("Invalid raw fixture bounds"));
    }
    let mut input = BufReader::new(file);
    let first =
        read_record(&mut input)?.ok_or_else(|| io::Error::other("Missing raw configuration"))?;
    if first.kind != Kind::Configuration || (first.width, first.height) != (320, 180) {
        return Err(io::Error::other("Unexpected raw fixture configuration"));
    }
    let mut receiver = Receiver::new(first.binding)?;
    receiver.accept(first)?;
    let mut frames = 0;
    while let Some(record) = read_record(&mut input)? {
        if frames >= 30
            || record.sequence != frames + 1
            || matches!(record.kind, Kind::Configuration)
        {
            return Err(io::Error::other("Unexpected raw picture count/sequence"));
        }
        receiver.accept(record)?;
        if receiver.pop().is_none() || receiver.take_keyframe_request() {
            return Err(io::Error::other("Raw fixture cannot decode continuously"));
        }
        frames += 1;
    }
    if frames != 30 {
        return Err(io::Error::other("Expected30 raw pictures"));
    }
    Ok(())
}
fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 2 {
        return Err(io::Error::other("Usage: video_check GENERATED_PACKETS_FLV"));
    }
    validate(Path::new(&args[1]))?;
    println!("{{\"records\":31,\"pictures\":30,\"protocol_verified\":true}}");
    Ok(())
}
