#[cfg(all(target_os = "macos", feature = "native-video"))]
#[allow(dead_code)]
#[path = "../src/native_video.rs"]
mod native_video;
#[allow(dead_code)]
#[path = "../src/viewer_video.rs"]
mod viewer_video;
use freelane_agent_protocol::WindowTarget;
use viewer_video::ViewerVideo;
fn target(handle: u64) -> WindowTarget {
    WindowTarget {
        handle,
        pid: 2,
        created: 3,
        incarnation: 4,
    }
}
#[test]
fn fallback_does_not_retry_until_selection_or_explicit_retry_changes() {
    let mut video = ViewerVideo::default();
    video.requested = true;
    video.maintain(Some(target(1)), false);
    let attempt = video.begin().unwrap();
    assert_eq!(attempt.target, target(1));
    assert!(video.begin().is_none());
    video.failed(&attempt, "No helper".into());
    assert!(video.begin().is_none());
    video.maintain(Some(target(2)), false);
    assert_eq!(video.begin().map(|attempt| attempt.target), Some(target(2)));
    video.invalidate();
    assert_eq!(video.begin().map(|attempt| attempt.target), Some(target(2)));
}
#[test]
fn minimization_and_retirement_block_new_video_start() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let mut video = ViewerVideo::default();
    video.requested = true;
    video.maintain(Some(target(1)), true);
    assert!(video.begin().is_none());
    video.maintain(Some(target(1)), false);
    let pending = Arc::new(AtomicBool::new(false));
    video.wait_for_retirement(pending.clone());
    assert!(video.begin().is_none());
    pending.store(true, Ordering::Release);
    video.maintain(Some(target(1)), false);
    assert_eq!(video.begin().map(|attempt| attempt.target), Some(target(1)));
    video.requested = false;
    video.invalidate();
    assert!(video.begin().is_none());
}
#[test]
fn stale_failure_cannot_replace_selected_status() {
    let mut video = ViewerVideo::default();
    video.requested = true;
    video.maintain(Some(target(1)), false);
    let old = video.begin().unwrap();
    video.maintain(Some(target(2)), false);
    let current = video.begin().unwrap();
    video.failed(&current, "Busy".into());
    video.failed(&old, "Stale failure".into());
    assert_eq!(video.status(), "Busy");
}
#[test]
fn invalidating_and_restarting_same_target_rejects_previous_attempt() {
    let mut video = ViewerVideo::default();
    video.requested = true;
    video.maintain(Some(target(1)), false);
    let old = video.begin().unwrap();
    video.invalidate();
    let current = video.begin().unwrap();
    video.failed(&current, "Current".into());
    video.failed(&old, "Obsolete".into());
    assert_eq!(video.status(), "Current");
    assert_ne!(old, current);
}
