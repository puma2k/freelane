#[path = "../src/native_video.rs"]
mod native_video;
use freelane_agent_protocol::video::{Binding, Kind, Record};
use native_video::StreamCheck;
fn config() -> Record {
    Record {
        binding: Binding {
            nonce: [7; 16],
            stream: 1,
            generation: 1,
        },
        width: 320,
        height: 180,
        sequence: 0,
        pts_us: 0,
        kind: Kind::Configuration,
        payload: vec![
            1, 66, 0, 30, 255, 225, 0, 4, 0x67, 66, 0, 30, 1, 0, 2, 0x68, 0x80,
        ],
    }
}
fn picture() -> Record {
    let mut p = config();
    p.sequence = 1;
    p.kind = Kind::AccessUnit { idr: true };
    p.payload = vec![0, 0, 0, 2, 0x65, 0x80];
    p
}
#[test]
fn requires_configuration_exact_binding_and_monotonic_accepted_frames() {
    let mut check = StreamCheck::default();
    let config = config();
    let frame = picture();
    assert!(check.frame(&frame).is_err());
    check.configuration(&config).unwrap();
    check.frame(&frame).unwrap();
    check.frame(&frame).unwrap(); // busy never commits PTS.
    let mut wrong = frame.clone();
    wrong.binding.generation += 1;
    assert!(check.frame(&wrong).is_err());
    wrong = frame.clone();
    wrong.width += 2;
    assert!(check.frame(&wrong).is_err());
    check.accepted(&frame);
    assert!(check.frame(&frame).is_err());
    assert!(check.configuration(&config).is_err());
}
#[test]
fn rejects_wrong_kind_oversized_payload_bad_dimensions_and_unrepresentable_time() {
    let mut check = StreamCheck::default();
    assert!(check.configuration(&picture()).is_err());
    let mut config = config();
    config.width = 0;
    assert!(check.configuration(&config).is_err());
    config.width = 320;
    check.configuration(&config).unwrap();
    assert!(check.frame(&config).is_err());
    let mut frame = picture();
    frame.payload.resize(2 * 1024 * 1024 + 1, 0);
    assert!(check.frame(&frame).is_err());
    frame = picture();
    frame.pts_us = u64::MAX;
    assert!(check.frame(&frame).is_err());
    frame = picture();
    frame.kind = Kind::AccessUnit { idr: false };
    frame.payload[4] = 0x41;
    assert!(check.frame(&frame).is_err());
}
