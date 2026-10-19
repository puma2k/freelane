# FreeLane

[English](README.md) · Tiếng Việt

**Trình quản lý máy ảo và thiết bị giả lập mã nguồn mở cho Mac Apple Silicon, viết bằng Rust.**

Quản lý **Windows 11 ARM**, **Ubuntu ARM64**, **Android Emulator** và **Xcode iOS Simulator** qua giao diện Việt/Anh, CLI và MCP để phát triển, chạy và kiểm thử ứng dụng.

FreeLane đang ở giai đoạn thử nghiệm, hướng tới một lựa chọn miễn phí thay thế Parallels Desktop. Bằng chứng chạy thực tế hiện đến từ Linux cloud; chạy trên Mac thật, độ ổn định hằng ngày và lợi thế hiệu năng vẫn cần kiểm chứng.

![Trung tâm điều khiển FreeLane tiếng Việt](docs/assets/desktop-overview-vi.png)

*UI tiếng Việt chạy thật trên Linux Xvfb với cấu hình Ubuntu đã mở. Chưa cập nhật trạng thái chạy; đây không phải ảnh desktop của hệ điều hành khách.*

Có danh sách app đã biết, định danh Dock ổn định, UI sao lưu/khôi phục VM đã tắt và số liệu RAM/CPU thực dùng. [Phạm vi hiện tại](docs/plans/portable-workspace-batch.md).

## Nền tảng và trạng thái

| Nền tảng | Backend và tính năng | Phạm vi đã kiểm thử / giới hạn |
| --- | --- | --- |
| Windows 11 ARM | Máy ảo QEMU ARM, điều khiển trạng thái, chụp màn hình, guest agent tùy chọn | Đã vào desktop Audit, thử một số thao tác 7-Zip x86/x64/ARM64 và Notepad. Lỗi kernel trước đó chưa được giải quyết; cần kiểm tra thiết lập người dùng và nhiều ứng dụng hơn. |
| Ubuntu ARM64 | QEMU ARM, ổ virtio, agent tùy chọn, chia sẻ thư mục 9p và GPU Virtio 2D | Ubuntu Server 24.04.5 từ cloud image; kiểm tra file/archive và dữ liệu sau reboot. Chưa kiểm chứng Ubuntu Desktop hoặc chạy trên Mac thật. |
| Android | SDK Emulator/ADB chính thức; khởi động/tắt/trạng thái, cài APK, mở activity và chụp PNG | AOSP Android 10 x86 trên Linux giả lập CPU phần mềm. Bài thử app qua sau khi phục hồi thủ công lỗi System UI; độ ổn định và tăng tốc trên Mac chưa được xác minh. |
| iOS | Xcode Simulator; điều khiển thiết bị, cài `.app` cho Simulator, mở bundle và chụp PNG | Cần Mac, Xcode và Simulator runtime. Đã kiểm tra biên dịch/cấu trúc lệnh, chưa chạy thực tế trên Mac. Chưa hỗ trợ IPA App Store. |

Windows/Ubuntu dùng QEMU cài riêng. Android/iOS dùng backend SDK chính thức. Trên Apple Silicon, tạo Android AVD với image ARM64 phù hợp; ứng dụng iOS phải được build riêng cho Simulator.

Xem [bằng chứng Windows](docs/test-evidence.md), [Ubuntu](docs/ubuntu-test-evidence.md) và [hướng dẫn/kiểm thử mobile](docs/mobile.md).

## Tính năng

- Một nút setup: chọn ISO Windows ARM64 hoặc phiên bản Ubuntu Server LTS; tự tải công cụ còn thiếu và ảnh Ubuntu. [Hướng dẫn](docs/setup.vi.md).
- Thư viện VM, thẻ tài nguyên, cấu hình và chuyển ngôn ngữ Việt/Anh.
- Chia sẻ trực tiếp Ubuntu9p chỉ đọc hoặc đọc/ghi, có kiểm tra và chọn lại thư mục. [Hướng dẫn](docs/sharing.vi.md).
- Chuyển file hai chiều có SHA-256, chọn file từ thư mục xuất và nhớ thư mục trên Windows; giữ file trùng tên, có tùy chọn mở văn bản đã gửi. [Hướng dẫn](docs/file-transfer.vi.md).
- Gửi/lấy và tự đồng bộ clipboard tiếng Việt có xử lý xung đột khi bật chia sẻ và dùng guest agent mới trong phiên có giao diện; cài/cập nhật/rollback/gỡ agent Windows có xác minh. [Hướng dẫn](docs/clipboard.vi.md).
- Chỉnh CPU, RAM, tăng tốc, mạng, media, hiển thị, guest agent và USB.
- Chuyển thư mục USB chỉ đọc cho Windows/Ubuntu; Ubuntu có chia sẻ đọc/ghi hai chiều qua 9p.
- GPU Virtio **2D cho Ubuntu**, đã kiểm tra scanout từ guest Linux ARM thật.
- Điều khiển thiết bị mobile, cài/mở ứng dụng và chụp khung hình PNG thật.
- CLI và **22 công cụ MCP stdio** cho build/test, điều khiển VM/mobile, chụp ảnh và gửi yêu cầu tới guest.

UI hiện hiển thị ảnh chụp; VM desktop có tùy chọn cập nhật định kỳ. Video VM liên tục và chuyển tiếp bàn phím đầy đủ chưa hoàn tất; dùng cửa sổ backend để tương tác desktop. [Viewer app Windows riêng và launcher Dock](docs/app-windows.vi.md) có video H.264 giải mã phần cứng trên Mac, tự dùng ảnh chụp khi video không khả dụng, cùng thao tác chuột, nhập chữ, phím tắt và đổi kích thước. Video cần bộ agent/helper Windows tương ứng và cổng video của VM. Đã có cấu hình chọn USB vật lý nhưng chưa kiểm thử phần cứng. Chia sẻ thư mục hai chiều cho Windows, tăng tốc 3D/Metal, passthrough GPU Apple và đầy đủ trải nghiệm như Coherence còn đang phát triển. CI Mac đã kiểm tra dữ liệu hình Windows qua viewer; hiệu năng lâu dài và VM cài thực tế vẫn cần thử trên Mac thật.

## Tải bản thử nghiệm

[Tải prerelease cho Mac Apple Silicon](https://github.com/puma2k/freelane/releases/tag/v0.1.0-preview.24): DMG/ZIP Mac, ZIP agent Windows ARM64 và file SHA256. Mở DMG rồi kéo FreeLane vào Applications. Có video app riêng, ảnh chụp dự phòng, setup agent/helper Windows và xuất launcher Dock; đọc ghi chú release trước khi cài.

[Workflow đóng gói Mac Apple Silicon](https://github.com/puma2k/freelane/actions/workflows/release.yml) tạo ZIP `.app` thử nghiệm trên runner Mac. Chỉ tải artifact khi job đã chạy thành công. Cần macOS14+; luồng setup VM tự tải backend còn thiếu, còn runtime mobile có phần thiết lập riêng; bản này ký ad-hoc, chưa notarize. Xem [hướng dẫn release](docs/release.md). Binary Linux cloud là bản riêng, không chạy được trên Mac.

## Build

Dùng toolchain đã ghim **Rust 1.99.0**:

```sh
cargo build --locked --workspace
```

Chuẩn bị thành phần cần cho tính năng muốn dùng:

| Tính năng | Điều kiện |
| --- | --- |
| VM Windows/Ubuntu | QEMU 10.0+, cặp firmware EDK2 AArch64 code/variables phù hợp; Windows thêm swtpm, firmware Secure Boot có khóa Microsoft và bộ cài Windows ARM |
| Android | Android Studio SDK, Emulator, Platform Tools mới và AVD phù hợp kiến trúc máy |
| iOS | macOS, Xcode đầy đủ, Simulator runtime và UUID thiết bị có sẵn |
| MCP | Python 3.11+; server chạy trên máy chứa SDK/VM |

Build UI trên Linux cần các thư viện trong [hướng dẫn phụ thuộc](docs/dependencies.md). DMG Mac đã có trên GitHub Releases; QEMU, firmware và bộ cài hệ điều hành vẫn cần chuẩn bị riêng. Người dùng lấy Windows, SDK và ứng dụng thương mại riêng theo giấy phép tương ứng.

## Windows và Ubuntu

Tạo thư mục VM mới với đường dẫn firmware:

```sh
./target/debug/freelane init /duong/dan/windows-vm /duong/dan/code.fd /duong/dan/vars.fd windows
./target/debug/freelane init /duong/dan/ubuntu-vm /duong/dan/code.fd /duong/dan/vars.fd ubuntu
```

`init` tạo ổ sparse 64 GiB và trạng thái firmware riêng, từ chối ghi đè thư mục có sẵn. Cấu hình media trước khi boot; xem [thiết lập Windows](docs/development.md) hoặc [Ubuntu](docs/ubuntu.md). Tạo profile chưa phải cài OS. Bản Ubuntu đã kiểm thử dùng Server cloud image chính thức với NoCloud provisioning.

```sh
./target/debug/freelane-desktop /duong/dan/windows-vm/vm.toml /duong/dan/ubuntu-vm/vm.toml --lang vi
```

Chọn thẻ trong thư viện hoặc **Thêm máy ảo**. **Cấu hình** nhóm phần cứng, đồ họa, tệp và thiết bị; lưu khi VM đã tắt. Profile mặc định dùng HVF trên Apple Silicon. Kiểm thử Linux phải chọn rõ `accelerator = "tcg"`; mạng mặc định tắt.

Đóng trình quản lý vẫn để VM chạy. Yêu cầu shutdown guest và kiểm tra đã thoát trước khi di chuyển ổ đĩa hoặc lưu snapshot cloud. Xem [điều khiển UI](docs/desktop.md), [guest agent](docs/guest-agent.md), [chia sẻ file](docs/sharing.md) và [đồ họa](docs/graphics.md).

## Android và iOS

Sao chép profile mobile rồi điền thông tin SDK/thiết bị đã chuẩn bị:

- [Mẫu Android](configs/mobile.android.example.toml): SDK root, tên AVD, CPU/RAM và tăng tốc/đồ họa.
- [Mẫu iOS](configs/mobile.ios.example.toml): UUID chính xác của Simulator và thư mục Xcode tùy chọn.

```sh
./target/debug/freelane-desktop --mobile /duong/dan/mobile.toml --lang vi
```

Bấm **Mở** trong mục **Android & iOS**, rồi khởi động, xem trạng thái, cài/mở app hoặc chụp ảnh. Cấu hình và điều khiển mobile hoạt động riêng với VM Windows/Ubuntu.

![Bảng điều khiển Android và iOS bằng tiếng Việt](docs/assets/mobile-panel-vi.png)

*UI tiếng Việt chụp trên Linux Xvfb với cấu hình Android đã mở; không khởi chạy giả lập để chụp ảnh này. Bằng chứng chụp ứng dụng Android trước đó và quá trình phục hồi lỗi System UI nằm trong [hướng dẫn mobile](docs/mobile.md).*

[Hướng dẫn mobile](docs/mobile.md) có lệnh CLI, cách kiểm thử APK, script tạo lại môi trường Linux, cả lỗi chưa phục hồi và lần thử thành công sau phục hồi.

## Vibe code với MCP

Kết nối client stdio tương thích như **Codex, Cursor hoặc Claude** tới server Python của FreeLane. Đăng ký đường dẫn profile VM/mobile có sẵn trong cấu hình local tin cậy, rồi tạo cấu hình client:

```sh
python3 scripts/mcp-client-config.py --client codex --config /duong/dan/mcp.local.toml
# Dùng --client cursor hoặc --client claude để xuất JSON.
```

Script in cấu hình để ghép vào thiết lập đang dùng. Server chạy các tác vụ phát triển cố định, trả exit code/timeout thật và ảnh chụp thực tế. Không tự cấp máy Mac từ xa hay cài cấu hình editor. Đường dẫn cloud chỉ dùng được khi tiến trình MCP chạy trong workspace cloud đó.

Xem [hướng dẫn kết nối MCP](docs/mcp.md) và [mẫu đăng ký thiết bị mobile](configs/mcp.mobile.example.toml).

## Kiểm thử và hướng phát triển

```sh
cargo test --locked --workspace
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
python3 -m unittest discover -s tests/mcp -v
```

Lần chạy Linux được ghi nhận gần nhất đã qua **59 kiểm thử Rust**, **11 kiểm thử Python MCP** và **6 bài QEMU thật chạy riêng**. `cargo test` thông thường bỏ qua 6 bài QEMU; [hướng dẫn phát triển](docs/development.md) nêu cách chuẩn bị và chạy. Kiểm tra biên dịch cho macOS ARM64 đã qua, nhưng chưa chứng minh chạy trên Mac. Kết quả mobile có giới hạn đã nêu ở trên.

Ưu tiên tiếp theo là kiểm thử Apple Silicon thật, cài đặt/shutdown tin cậy, ổn định Android, xác minh iOS Simulator và tăng tốc đồ họa. Chưa có bằng chứng chạy mọi app hoặc nhanh hơn Parallels. Xem [trạng thái triển khai](docs/implementation.md) và [tương thích ứng dụng](docs/compatibility.md).

## Giấy phép

Mã nguồn và tài liệu FreeLane dùng [MIT](LICENSE). Font giữ [giấy phép DejaVu](crates/desktop/assets/DejaVu-LICENSE.txt); QEMU, firmware, phần mềm TPM và SDK giữ giấy phép riêng.

FreeLane độc lập với Apple, Microsoft, Google, Canonical và Parallels.

[VM creation and persistent library](docs/vm-creation.md) · [Roadmap progress](docs/plans/progress.md) · [Mac acceptance](docs/mac-acceptance.md)

[Offline checkpoint/recovery-copy foundation](docs/checkpoints.md)

[Tạo Ubuntu từ ảnh cloud ARM64](docs/cloud-images.vi.md) · [Nhập/xuất cấu hình](docs/settings-bundles.md) · [Tìm lại VM đã chuyển thư mục](docs/library-recovery.md)

[Thiết lập tạo VM và nơi lưu](docs/setup.vi.md).
