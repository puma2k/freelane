use std::{env, path::PathBuf, process::Command};
fn output(command: &mut Command) -> String {
    let result = command.output().expect("Native Mac tool must be available");
    assert!(result.status.success(), "Native Mac tool failed");
    String::from_utf8(result.stdout)
        .expect("Native tool path must be UTF8")
        .trim()
        .to_owned()
}
fn main() {
    println!("cargo:rerun-if-changed=native/video-surface.swift");
    println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos")
        || env::var_os("CARGO_FEATURE_NATIVE_VIDEO").is_none()
    {
        return;
    }
    assert!(
        env::var("HOST").unwrap().contains("apple-darwin"),
        "Build native-video on a Mac with the Apple SDK"
    );
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        _ => panic!("Unsupported Mac video architecture"),
    };
    let directory = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let swift = output(Command::new("xcrun").args(["--find", "swiftc"]));
    let sdk = output(Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-path"]));
    let status = Command::new(&swift)
        .args([
            "-parse-as-library",
            "-swift-version",
            "5",
            "-O",
            "-emit-library",
            "-static",
            "-module-name",
            "FreeLaneNativeVideo",
            "-target",
        ])
        .arg(format!("{arch}-apple-macos12.0"))
        .args(["-sdk", &sdk, "native/video-surface.swift", "-o"])
        .arg(directory.join("libfreelane_native_video.a"))
        .status()
        .expect("Start Swift compiler");
    assert!(status.success(), "Native video Swift build failed");
    println!("cargo:rustc-link-search=native={}", directory.display());
    println!("cargo:rustc-link-search=native={sdk}/usr/lib/swift");
    let swift_path = PathBuf::from(swift);
    println!(
        "cargo:rustc-link-search=native={}",
        swift_path
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("lib/swift/macosx")
            .display()
    );
    println!("cargo:rustc-link-lib=static=freelane_native_video");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    for framework in [
        "AppKit",
        "AVFoundation",
        "VideoToolbox",
        "CoreVideo",
        "CoreMedia",
        "QuartzCore",
        "Foundation",
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}
