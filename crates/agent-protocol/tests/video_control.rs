use freelane_agent_protocol::{
    video::Binding,
    video_control::{Operation, Parameters, Reply, VERSION},
    Action, Request, WindowTarget,
};
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
        width: 1280,
        height: 720,
        fps: 30,
    }
}
fn request(version: u16, operation: Operation) -> Request {
    Request {
        version,
        id: 1,
        action: Action::Video { operation },
    }
}
#[test]
fn v14_is_exclusive_and_legacy_rejects_video() {
    for version in 1..14 {
        assert!(request(
            version,
            Operation::Start {
                parameters: parameters()
            }
        )
        .validate()
        .is_err());
    }
    assert!(request(
        VERSION,
        Operation::Start {
            parameters: parameters()
        }
    )
    .validate()
    .is_ok());
    assert!(Request {
        version: VERSION,
        id: 1,
        action: Action::Ping
    }
    .validate()
    .is_err());
}
#[test]
fn strict_json_roundtrips_without_unknown_fields() {
    let r = request(
        VERSION,
        Operation::Start {
            parameters: parameters(),
        },
    );
    let bytes = serde_json::to_vec(&r).unwrap();
    assert_eq!(serde_json::from_slice::<Request>(&bytes).unwrap(), r);
    let mut value = serde_json::to_value(&r).unwrap();
    value["action"]["operation"]["parameters"]["binding"]["unexpected"] = true.into();
    assert!(serde_json::from_value::<Request>(value).is_err());
}
#[test]
fn target_output_and_session_bounds_are_enforced() {
    for i in 0..12 {
        let mut p = parameters();
        match i {
            0 => p.target.handle = 0,
            1 => p.target.pid = 0,
            2 => p.target.created = 0,
            3 => p.target.incarnation = 0,
            4 => p.width = 0,
            5 => p.height = 721,
            6 => p.width = 1922,
            7 => p.height = 1082,
            8 => p.fps = 60,
            9 => p.binding.nonce = [0; 16],
            10 => p.binding.stream = 0,
            _ => p.binding.generation = 0,
        }
        assert!(
            request(VERSION, Operation::Start { parameters: p })
                .validate()
                .is_err(),
            "case={i}"
        );
    }
    let mut p = parameters();
    p.width = 1920;
    p.height = 1080;
    assert!(p.validate().is_ok());
}
#[test]
fn reply_must_match_the_exact_operation_target_and_generation() {
    let p = parameters();
    let start = Operation::Start {
        parameters: p.clone(),
    };
    assert!(Reply::Started {
        parameters: p.clone()
    }
    .validate_for(&start)
    .is_ok());
    let mut wrong = p.clone();
    wrong.target.incarnation += 1;
    assert!(Reply::Started { parameters: wrong }
        .validate_for(&start)
        .is_err());
    let stop = Operation::Stop { binding: p.binding };
    assert!(Reply::Started {
        parameters: p.clone()
    }
    .validate_for(&stop)
    .is_err());
    assert!(Reply::Stopped { binding: p.binding }
        .validate_for(&stop)
        .is_ok());
    assert!(Reply::KeyframeRequested { binding: p.binding }
        .validate_for(&Operation::Keyframe { binding: p.binding })
        .is_ok());
    let mut binding = p.binding;
    binding.generation += 1;
    assert!(Reply::Stopped { binding }.validate_for(&stop).is_err());
}
