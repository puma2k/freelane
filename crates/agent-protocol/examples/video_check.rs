//! Offline generated-fixture validator, never a source of capture authorization.
use freelane_agent_protocol::video::{read_record, Kind, Receiver};
use std::{
    fs::File,
    io::{self, BufReader},
    path::Path,
};
fn validate(path: &Path, worker: bool) -> io::Result<()> {
    let expected = if worker { 90 } else { 30 };
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > 4 * 1024 * 1024 {
        return Err(io::Error::other("Invalid raw fixture bounds"));
    }
    let mut input = BufReader::new(file);
    let first =
        read_record(&mut input)?.ok_or_else(|| io::Error::other("Missing raw configuration"))?;
    if first.kind != Kind::Configuration
        || (!worker && (first.width, first.height) != (320, 180))
        || (worker && (first.width > 320 || first.height > 180))
    {
        return Err(io::Error::other("Unexpected raw fixture configuration"));
    }
    let mut receiver = Receiver::new(first.binding)?;
    receiver.accept(first)?;
    let mut frames = 0;
    while let Some(record) = read_record(&mut input)? {
        if frames >= expected
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
    if frames != expected {
        return Err(io::Error::other("Unexpected raw picture count"));
    }
    Ok(())
}
fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    let worker = args.len() == 3 && args[2] == "--worker";
    if args.len() != 2 && !worker {
        return Err(io::Error::other(
            "Usage: video_check GENERATED_PACKETS_FLV [--worker]",
        ));
    }
    validate(Path::new(&args[1]), worker)?;
    let frames = if worker { 90 } else { 30 };
    println!(
        "{{\"records\":{},\"pictures\":{},\"protocol_verified\":true}}",
        frames + 1,
        frames
    );
    Ok(())
}
