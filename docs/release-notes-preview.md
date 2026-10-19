FreeLane v0.1.0-preview.24 for Apple Silicon Macs (macOS14+).

Adds experimental live video to separate Windows app windows, with snapshot fallback.

- Captures client-area pixels on Windows, streams bounded H.264 over the dedicated virtio-serial port, and embeds an NV12 hardware decoder/native display layer in the existing Mac viewer. One video stream per VM; other viewers keep snapshots.
- Keeps targeted mouse/text/shortcut controls and Dock launchers. Closing a viewer stops its own stream and leaves the guest app/VM running. Stale Start replies are rejected; fresh stream identities permit restart; silent transports return to snapshots.
- Bundles the static-CRT ARM64 agent and native capture helper in the Windows ZIP. After verifying/extracting the ZIP, run `powershell.exe -NoProfile -File .\setup-windows-agent.ps1` in the signed-in Windows user's session. Setup/update/rollback verifies the agent/helper pair. The signed virtio-serial driver remains a separate prerequisite; sign in again after setup.
- Enable **Live video / Video trực tiếp** in an app viewer. Older VM instances may need a convenient restart to provision the video port; FreeLane does not stop them automatically. Re-export Dock launchers after upgrading because they retain copied binaries.

Validation:284Rust passed/9ignored,16video tests,14MCP, strict lint. Windows ARM64/x64 owned capture, paired installer lifecycle and extracted ARM64 package setup/capture/uninstall passed native CI. Mac ARM CI sends actual Windows-captured payloads through production session/broker/viewer state and hardware decoding in eframe's NSView; it uses an owned socket peer, not a running guest VM. Regression tests reproduced and fixed restart retirement, silent-open transport and invalidated Start races.

Physical Mac installed-VM input/DPI, sleep/wake, onscreen pixels, native-view/egui popup overlap and sustained CPU/latency/FPS remain unverified. No all-app, zero-copy, 30FPS acceptance or Parallels performance claim. Full-VM desktop display remains snapshot-based. Experimental, ad-hoc signed; no Developer ID/notarization.

Tiếng Việt: Thêm video trực tiếp cho cửa sổ app Windows riêng trên Mac, có ảnh chụp dự phòng. Gói Windows đi kèm agent và helper; chạy một lệnh setup sau khi kiểm tra checksum. Đã qua CI Windows ARM/Mac ARM nhưng trải nghiệm trên VM cài thực tế và hiệu năng dài hạn vẫn cần thử trên Mac của anh.
