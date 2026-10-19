# FreeLane

English · [Tiếng Việt](README.vi.md)

**An open-source VM and mobile simulator manager for Mac Apple Silicon, built with Rust.**

Manage **Windows 11 ARM**, **Ubuntu ARM64**, **Android Emulator** and **Xcode iOS Simulator** through a Vietnamese/English desktop interface, CLI and MCP tools for development and testing.

FreeLane is experimental. Its long-term goal is a free alternative to Parallels Desktop. Current runtime evidence comes from Linux cloud testing; native Mac execution, everyday reliability and a performance advantage still need validation.

![FreeLane desktop control center](docs/assets/desktop-overview-en.png)

*English UI running on Linux Xvfb with an Ubuntu configuration loaded. Runtime state has not been refreshed; this is not a guest desktop.*

## Platforms and current status

| Platform | Backend and capabilities | Verified scope / limits |
| --- | --- | --- |
| Windows 11 ARM | QEMU ARM VM, lifecycle controls, display capture, optional guest agent | Installed Audit desktop; selected 7-Zip x86/x64/ARM64 and Notepad tests. An earlier kernel crash remains unresolved; normal user setup and wider app compatibility need testing. |
| Ubuntu ARM64 | QEMU ARM VM, virtio storage, optional agent, live 9p folder sharing, Virtio GPU 2D | Ubuntu Server 24.04.5 cloud image; file/archive checks and persistence after reboot. Ubuntu Desktop and native Mac runtime remain unverified. |
| Android | Official SDK Emulator/ADB; start/stop/status, APK install, activity launch and PNG capture | AOSP Android 10 x86 on Linux software emulation. App checks passed after manual recovery from a System UI ANR; sustained stability and Mac acceleration remain unverified. |
| iOS | Apple Xcode Simulator; device controls, Simulator `.app` install, bundle launch and PNG capture | Requires Mac, Xcode and an installed Simulator runtime. Adapter compilation and argument tests passed; native operation remains unverified. App Store IPA files are unsupported. |

Windows/Ubuntu use separately installed QEMU. Android/iOS use their official SDK backends. On Apple Silicon, prepare a matching ARM64 Android AVD. iOS applications must be built for Simulator.

See [Windows evidence](docs/test-evidence.md), [Ubuntu evidence](docs/ubuntu-test-evidence.md) and [mobile setup and evidence](docs/mobile.md).

## Features

- One setup action: select a Windows ARM64 ISO or Ubuntu Server LTS version; download missing VM tools and Ubuntu media automatically. [Setup guide](docs/setup.md).
- Searchable VM library, visible CPU/RAM allocation presets, settings and Vietnamese/English language selection.
- Known guest app discovery/search, stable exported Dock identities, stopped-VM backup/restore UI and observed host RAM/CPU counters. [Current scope](docs/plans/portable-workspace-batch.md).
- Live Ubuntu9p sharing in read-only or read/write mode, with folder availability/reselection controls. [Sharing](docs/sharing.md).
- Verified file copies in both directions, an outbox file picker and remembered Windows folder choices. Existing files are preserved; sent text documents can optionally open. [File transfer](docs/file-transfer.md).
- Opt-in Unicode text clipboard send/copy and conflict-aware automatic synchronization with an updated graphical-session guest agent; verified Windows agent install/update/rollback/uninstall. [Clipboard](docs/clipboard.md).
- CPU, RAM, acceleration, network, media, display, guest-agent and USB configuration.
- Read-only USB folder transfer for Windows/Ubuntu; live read/write 9p sharing for Ubuntu.
- Ubuntu Virtio GPU **2D** display, with a real ARM Linux scanout test.
- Mobile device controls, app installation/launch and actual PNG snapshots.
- CLI and **22 stdio MCP tools** for fixed build/test jobs, registered VM/mobile controls, captures and guest requests.

The desktop displays snapshots, with optional refresh for desktop VMs. Continuous VM video and full keyboard forwarding are pending; use backend native windows for desktop input. The experimental [Windows app viewer](docs/app-windows.md) supports separate Mac windows with hardware-decoded H.264 video and snapshot fallback, targeted pointer gestures/text, guarded shortcuts, window sizing and Dock launchers. Live video requires the matching Windows agent/helper package and the VM video port. Physical USB selectors are implemented but need hardware testing. Windows two-way folder sharing, accelerated 3D/Metal, Apple GPU passthrough and full Coherence parity remain pending. Native CI exercises captured Windows packets through the Mac viewer; sustained performance and installed-VM behavior still need physical-Mac testing.

## Download an experimental build

[Download the Apple Silicon prerelease](https://github.com/puma2k/freelane/releases/tag/v0.1.0-preview.23): Mac DMG/ZIP, Windows ARM64 guest-agent ZIP and SHA256 files. Open the DMG and drag FreeLane into Applications. Includes experimental app windows and Dock export; read the release notes before installation.

The [Mac Apple Silicon packaging workflow](https://github.com/puma2k/freelane/actions/workflows/release.yml) produces an experimental `.app` ZIP on a Mac runner. Download the artifact only from a successful run. It needs macOS14+; the VM setup flow downloads missing backends automatically, while mobile runtimes use separate setup; it is ad-hoc signed, not notarized. See [release instructions](docs/release.md). Linux cloud binaries are a separate build and cannot run on Mac.

## Build

Use the pinned **Rust 1.99.0** toolchain:

```sh
cargo build --locked --workspace
```

Install the dependencies needed for your workflow:

| Workflow | Prerequisites |
| --- | --- |
| Windows/Ubuntu VM | QEMU 10.0+, matching AArch64 EDK2 code/variable firmware; Windows also needs swtpm, Secure Boot firmware with Microsoft keys and a Windows ARM installer |
| Android | Android Studio SDK, Emulator, recent Platform Tools and a prepared AVD matching your host |
| iOS | macOS, full Xcode, Simulator runtime and an available device UUID |
| MCP | Python 3.11+; client runs the server on the machine owning the SDKs/VMs |

Linux desktop builds require the libraries in [dependencies](docs/dependencies.md). There is no packaged one-click Mac installer yet. Windows, SDKs and proprietary applications are obtained separately under their respective licenses.

## Windows and Ubuntu

Create a new VM directory with matching firmware paths:

```sh
./target/debug/freelane init /absolute/path/to/windows-vm /absolute/path/to/code.fd /absolute/path/to/vars.fd windows
./target/debug/freelane init /absolute/path/to/ubuntu-vm /absolute/path/to/code.fd /absolute/path/to/vars.fd ubuntu
```

`init` creates a sparse 64 GiB disk and private firmware state; it refuses an existing directory. Configure installation media before booting. Follow [Windows setup](docs/development.md) or [Ubuntu setup](docs/ubuntu.md); creating a profile does not install the OS. The recorded Ubuntu deployment uses an official preinstalled Server cloud image with NoCloud provisioning.

```sh
./target/debug/freelane-desktop /absolute/path/to/windows-vm/vm.toml /absolute/path/to/ubuntu-vm/vm.toml --lang en
```

Select a library card or **Add virtual machine**. **Settings** groups hardware, graphics, files and devices; save while stopped. Profiles default to HVF on Apple Silicon. Linux testing requires explicitly selecting `accelerator = "tcg"`; networking defaults to off.

Closing the manager leaves the VM running. Request guest shutdown and verify exit before moving disks or saving a cloud snapshot. See [desktop controls](docs/desktop.md), [guest agent](docs/guest-agent.md), [sharing](docs/sharing.md) and [graphics](docs/graphics.md).

## Android and iOS

Copy a mobile profile and fill in existing SDK/device details:

- [Android example](configs/mobile.android.example.toml): SDK root, AVD name, CPU/RAM and acceleration/rendering choices.
- [iOS example](configs/mobile.ios.example.toml): exact available Simulator UUID and optional Xcode developer directory.

```sh
./target/debug/freelane-desktop --mobile /absolute/path/to/mobile.toml --lang en
```

Click **Open / Mở** in **Android & iOS**, then start, inspect, install, launch or capture. Mobile configuration and controls operate separately from Windows/Ubuntu VMs.

![FreeLane Android and iOS controls in English](docs/assets/mobile-panel-en.png)

*English UI captured on Linux Xvfb with an Android profile loaded; no emulator was started for this screenshot. Earlier Android app-capture evidence and the System UI ANR recovery are documented in the [mobile guide](docs/mobile.md).*

[Mobile guide](docs/mobile.md) includes CLI commands, APK testing, the reproducible Linux fixture and the unrecovered failure alongside the successful observation after recovery.

## Vibe coding with MCP

Connect compatible stdio clients such as **Codex, Cursor or Claude** to FreeLane's standard-library Python server. Register existing VM and mobile profile paths in trusted local configuration, then generate a client configuration:

```sh
python3 scripts/mcp-client-config.py --client codex --config /absolute/path/to/mcp.local.toml
# Use --client cursor or --client claude for JSON configuration.
```

The generator prints configuration for you to merge into existing settings. The server executes fixed development jobs, reports actual exit codes/timeouts and returns genuine captures. It does not provision remote Macs or automatically configure editors. Cloud paths only work when the MCP process runs in that cloud workspace.

See [MCP connection guide](docs/mcp.md) and [mobile registry example](configs/mcp.mobile.example.toml).

## Validation and next steps

```sh
cargo test --locked --workspace
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
python3 -m unittest discover -s tests/mcp -v
```

The latest recorded Linux run passed **59 portable Rust tests**, **11 Python MCP tests** and **six separately invoked real QEMU integration tests**. Ordinary `cargo test` ignores the six QEMU tests; [development instructions](docs/development.md) describe their prerequisites. macOS ARM64 cross-compilation checks passed; they do not establish native execution. Mobile runtime evidence has the limits described above.

Priorities are native Apple Silicon testing, reliable setup/shutdown, Android stability, actual iOS Simulator validation and accelerated graphics. Universal app support and faster-than-Parallels performance have not been demonstrated. [Implementation status](docs/implementation.md) and [compatibility notes](docs/compatibility.md) track the remaining work.

## License

FreeLane source and documentation use the [MIT license](LICENSE). The bundled font retains its [DejaVu license](crates/desktop/assets/DejaVu-LICENSE.txt). QEMU, firmware, TPM software and SDKs retain their own licenses.

FreeLane is independent and is not affiliated with Apple, Microsoft, Google, Canonical or Parallels.

[VM creation and persistent library](docs/vm-creation.md) · [Roadmap progress](docs/plans/progress.md) · [Mac acceptance](docs/mac-acceptance.md)

[Offline checkpoint/recovery-copy foundation](docs/checkpoints.md)

[Create Ubuntu from an ARM64 cloud image](docs/cloud-images.md) · [Import/export settings](docs/settings-bundles.md) · [Reconnect a moved VM](docs/library-recovery.md)

[Creation and storage setup](docs/setup.md).
