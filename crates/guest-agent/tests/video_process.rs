use freelane_agent_protocol::{video::Binding, video_control::Parameters, WindowTarget};
use freelane_guest_agent::video_process::setup_bytes;
fn parameters() -> Parameters {
    Parameters {
        target: WindowTarget {
            handle: 1,
            pid: 2,
            created: 3,
            incarnation: 4,
        },
        binding: Binding {
            nonce: [5; 16],
            stream: 6,
            generation: 7,
        },
        width: 320,
        height: 180,
        fps: 30,
    }
}
#[test]
fn setup_has_fixed_big_endian_layout_and_validates_before_encoding() {
    let p = parameters();
    let bytes = setup_bytes(&p).unwrap();
    assert_eq!(bytes.len(), 80);
    assert_eq!(&bytes[..8], b"FLW1\0P\0\0");
    assert_eq!(&bytes[8..16], &1u64.to_be_bytes());
    assert_eq!(&bytes[16..20], &2u32.to_be_bytes());
    assert_eq!(&bytes[24..32], &3u64.to_be_bytes());
    assert_eq!(&bytes[32..40], &4u64.to_be_bytes());
    assert_eq!(&bytes[40..56], &[5; 16]);
    assert_eq!(&bytes[56..64], &6u64.to_be_bytes());
    assert_eq!(&bytes[64..72], &7u64.to_be_bytes());
    assert_eq!(&bytes[72..], &[1, 64, 0, 180, 30, 0, 0, 0]);
    let mut bad = p;
    bad.target.incarnation = 0;
    assert!(setup_bytes(&bad).is_err());
}
