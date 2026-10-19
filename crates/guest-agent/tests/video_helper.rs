use freelane_guest_agent::settings::video_helper;
use sha2::{Digest, Sha256};
use std::fs;
#[test]
fn managed_helper_is_pinned_to_manifest_hash_and_refuses_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("freelane-guest-agent.exe");
    let bytes = b"owned helper fixture";
    let hash = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let helper = dir.path().join(format!("freelane-video-helper-{hash}.exe"));
    fs::write(&helper, bytes).unwrap();
    fs::write(
        dir.path().join("agent-install.json"),
        format!(r#"{{"schema":1,"video_sha256":"{hash}"}}"#),
    )
    .unwrap();
    assert_eq!(video_helper(&exe).unwrap(), helper);
    fs::write(&helper, b"modified").unwrap();
    assert!(video_helper(&exe).is_err());
    fs::write(
        dir.path().join("agent-install.json"),
        r#"{"schema":1,"video_sha256":"../outside.exe"}"#,
    )
    .unwrap();
    assert!(video_helper(&exe).is_err());
}
#[test]
fn legacy_install_uses_fixed_sibling_but_invalid_manifest_never_falls_back() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("freelane-guest-agent.exe");
    assert_eq!(
        video_helper(&exe).unwrap(),
        dir.path().join("freelane-video-helper.exe")
    );
    fs::write(dir.path().join("agent-install.json"), r#"{"schema":1}"#).unwrap();
    assert_eq!(
        video_helper(&exe).unwrap(),
        dir.path().join("freelane-video-helper.exe")
    );
    fs::write(dir.path().join("agent-install.json"), "invalid").unwrap();
    assert!(video_helper(&exe).is_err());
}
#[cfg(unix)]
#[test]
fn helper_and_manifest_symlinks_are_refused() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("agent.exe");
    let hash = Sha256::digest(b"fixture")
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let source = dir.path().join("source");
    fs::write(&source, b"fixture").unwrap();
    symlink(
        &source,
        dir.path().join(format!("freelane-video-helper-{hash}.exe")),
    )
    .unwrap();
    let manifest = dir.path().join("agent-install.json");
    fs::write(
        &manifest,
        format!(r#"{{"schema":1,"video_sha256":"{hash}"}}"#),
    )
    .unwrap();
    assert!(video_helper(&exe).is_err());
    fs::remove_file(&manifest).unwrap();
    symlink(&source, &manifest).unwrap();
    assert!(video_helper(&exe).is_err());
}
