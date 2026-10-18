use freelane_agent_protocol::video::{read_record, write_record, Binding, Kind, Receiver, Record};
use std::io::{self, Cursor, Read, Write};
fn binding() -> Binding {
    Binding {
        nonce: [7; 16],
        stream: 1,
        generation: 1,
    }
}
fn config() -> Record {
    Record {
        binding: binding(),
        kind: Kind::Configuration,
        width: 320,
        height: 180,
        sequence: 0,
        pts_us: 0,
        payload: vec![
            1, 66, 0, 30, 255, 225, 0, 4, 0x67, 66, 0, 30, 1, 0, 2, 0x68, 0,
        ],
    }
}
fn frame(sequence: u64, idr: bool) -> Record {
    Record {
        kind: Kind::AccessUnit { idr },
        sequence,
        pts_us: sequence * 33333,
        payload: vec![0, 0, 0, 2, if idr { 0x65 } else { 0x41 }, 0x80],
        ..config()
    }
}
fn wire(record: &Record) -> Vec<u8> {
    let mut out = vec![];
    write_record(&mut out, record).unwrap();
    out
}
#[test]
fn short_io_roundtrips_and_distinguishes_clean_eof_from_truncation() {
    struct Short<T>(T);
    impl<T: Read> Read for Short<T> {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            self.0.read(&mut b[..1])
        }
    }
    impl<T: Write> Write for Short<T> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.write(&b[..b.len().min(3)])
        }
        fn flush(&mut self) -> io::Result<()> {
            self.0.flush()
        }
    }
    let r = frame(1, true);
    let mut out = Short(vec![]);
    write_record(&mut out, &r).unwrap();
    let mut input = Short(Cursor::new(out.0.clone()));
    assert_eq!(read_record(&mut input).unwrap(), Some(r));
    assert!(read_record(&mut input).unwrap().is_none());
    for length in 1..out.0.len() {
        assert!(
            read_record(&mut Cursor::new(&out.0[..length])).is_err(),
            "length={length}"
        );
    }
}
#[test]
fn invalid_headers_reject_before_payload_read() {
    for (offset, bytes) in [
        (0, vec![0]),
        (4, vec![9]),
        (5, vec![128]),
        (6, vec![100]),
        (7, vec![1]),
        (8, (2_097_153u32).to_be_bytes().to_vec()),
        (12, 1922u16.to_be_bytes().to_vec()),
        (14, 181u16.to_be_bytes().to_vec()),
        (16, vec![0; 16]),
        (32, vec![0; 8]),
        (40, vec![0; 8]),
    ] {
        let mut encoded = wire(&frame(1, true));
        encoded[offset..offset + bytes.len()].copy_from_slice(&bytes);
        let mut input = Cursor::new(encoded);
        assert!(read_record(&mut input).is_err(), "offset={offset}");
        assert_eq!(
            input.position(),
            64,
            "read payload before checking offset={offset}"
        );
    }
    let mut encoded = wire(&config());
    encoded[8..12].copy_from_slice(&65537u32.to_be_bytes());
    let mut input = Cursor::new(encoded);
    assert!(read_record(&mut input).is_err());
    assert_eq!(input.position(), 64);
}
#[test]
fn config_and_nal_validation_reject_fabricated_random_access() {
    let mut r = frame(1, true);
    r.payload[4] = 0x41;
    assert!(write_record(&mut vec![], &r).is_err());
    for payload in [
        vec![],
        vec![0, 0, 0, 0],
        vec![0, 0, 0, 20, 0x65],
        vec![0, 0, 0, 1, 0x67],
        vec![0, 0, 0, 1, 0xe5],
    ] {
        r.payload = payload;
        assert!(write_record(&mut vec![], &r).is_err());
    }
    for index in [0, 1, 4, 5, 8, 9, 12, 15] {
        let mut r = config();
        r.payload[index] = 0;
        assert!(write_record(&mut vec![], &r).is_err(), "index={index}");
    }
    let mut r = config();
    r.payload.push(0);
    assert!(write_record(&mut vec![], &r).is_err());
}
#[test]
fn overflow_drops_dependency_chain_and_coalesces_keyframe_requests() {
    let mut r = Receiver::new(binding()).unwrap();
    r.accept(config()).unwrap();
    r.accept(frame(1, true)).unwrap();
    r.accept(frame(2, false)).unwrap();
    r.accept(frame(3, false)).unwrap();
    assert!(r.pop().is_none());
    assert!(r.take_keyframe_request());
    assert!(!r.take_keyframe_request());
    r.accept(frame(4, false)).unwrap();
    assert!(r.pop().is_none());
    r.accept(frame(5, true)).unwrap();
    r.accept(frame(6, false)).unwrap();
    assert_eq!(r.pop().unwrap().sequence, 5);
    assert_eq!(r.pop().unwrap().sequence, 6);
    assert!(r.pop().is_none());
}
#[test]
fn sequence_gap_requires_idr_but_replay_and_time_regression_fail() {
    let mut r = Receiver::new(binding()).unwrap();
    r.accept(config()).unwrap();
    r.accept(frame(1, true)).unwrap();
    r.accept(frame(3, false)).unwrap();
    assert!(r.pop().is_none());
    assert!(r.take_keyframe_request());
    r.accept(frame(4, true)).unwrap();
    assert_eq!(r.pop().unwrap().sequence, 4);
    assert!(r.accept(frame(4, true)).is_err());
    let mut backwards = frame(5, true);
    backwards.pts_us = 1;
    assert!(r.accept(backwards).is_err());
}
#[test]
fn identity_dimensions_config_and_disconnect_cannot_reroute_frames() {
    let mut r = Receiver::new(binding()).unwrap();
    assert!(r.accept(frame(1, true)).is_err());
    r.accept(config()).unwrap();
    let mut wrong = frame(1, true);
    wrong.binding.generation = 2;
    assert!(r.accept(wrong).is_err());
    let mut wrong = frame(1, true);
    wrong.binding.nonce = [8; 16];
    assert!(r.accept(wrong).is_err());
    let mut wrong = frame(1, true);
    wrong.width = 640;
    assert!(r.accept(wrong).is_err());
    assert!(r.accept(config()).is_err());
    r.accept(frame(1, true)).unwrap();
    r.clear();
    assert!(r.pop().is_none());
    assert!(r.configuration().is_none());
    assert!(r.accept(frame(2, true)).is_err());
    assert!(r.accept(config()).is_err());
}
