# Experimental release packages

FreeLane 0.1.0 packages are development previews. The release profile optimizes compiled Rust code; it does not establish smooth guest performance or everyday reliability. Windows stability and Android software-emulation System UI ANRs remain unresolved. iOS runtime and native Mac guest execution still need verification.

## Mac Apple Silicon

[Latest preview: v0.1.0-preview.24](https://github.com/puma2k/freelane/releases/tag/v0.1.0-preview.24) adds experimental live Windows app video with hardware decoding on Mac and snapshot fallback. The Windows ARM64 package includes the native capture helper and one setup command. [Integration evidence and limits](coherence-live-viewer.md) distinguish native socket-emulated testing from real installed-VM and physical-Mac acceptance. [Published payload verification](evidence/release-preview24.json) records checksums, embedded source revision, Mac/Windows architectures and matching setup scripts.

[Historical preview: v0.1.0-preview.23](https://github.com/puma2k/freelane/releases/tag/v0.1.0-preview.23) skips repeated host JPEG decoding and texture recreation for an identical validated app frame. Changed frames retain bounded decoding; a manual unchanged frame can restore a cleared texture locally without another guest request. Snapshot cadence/quality, guest work, bandwidth and input behavior stay unchanged. [Optimization evidence](evidence/viewer-frame-cache.json) records narrowly scoped Linux debug CPU samples, not Mac/release performance. [Release verification](evidence/release-preview23.json) records successful source/tag/native CI and public payload checksums/architectures/source revision. Native Mac and installed Windows acceptance remain pending.

[Preview.12 verification](evidence/release-preview12.json) records native packaging/required checks and an independently downloaded public DMG checksum. Preview.11 did not publish because of a corrected attribution staging error.

New preview tags include a compressed DMG: open it, drag `FreeLane.app` onto `Applications`, then eject it. The native Mac job verifies the disk image, mounts it read-only and rechecks the app signature, ARM64 architecture and Applications shortcut before upload. SHA256 files accompany DMG and ZIP downloads.

[Historical v0.1.0-preview.2](https://github.com/puma2k/freelane/releases/tag/v0.1.0-preview.2): Mac ARM64 DMG/ZIP, Windows ARM64 agent ZIP and SHA256 files, built from `9c5272e`. [Release workflow](https://github.com/puma2k/freelane/actions/runs/38277664939) builds and verifies the packages before publishing. [DMG verification evidence](evidence/published-dmg-preview.json) records native mounting/signature checks and an independent downloaded-file checksum. Native Mac guest execution and Dock quick-open remain unverified.

Use macOS 14 or later on Apple Silicon. The [Experimental release packages workflow](https://github.com/puma2k/freelane/actions/workflows/release.yml) builds the CLI and desktop with the macOS SDK, bundles `FreeLane.app`, ad-hoc signs it, verifies the signatures/ARM64 architecture and executes an offline release CLI plan check. Download its `FreeLane-experimental-macos-arm64` artifact from a **successful** run, then extract the inner ZIP. A workflow definition or push alone is not evidence that its build succeeded. Artifacts expire after 30 days. Preview tags additionally build the Windows agent and publish a GitHub prerelease after both packaging jobs and checksum verification succeed.

The package contains `FreeLane.app`, CLI, documentation, example configs, source/font notices, dependency metadata, commit identifier and a ZIP SHA-256 file. It does not contain VM disks, Windows, QEMU, swtpm, firmware, Android SDKs, Xcode or guest-agent binaries. Use [the setup flow](setup.md) to download missing VM tools. SDK/firmware/config paths must match your Mac; examples contain placeholders.

The app uses local ad-hoc signing, **not Developer ID signing or Apple notarization**. macOS may block a downloaded experimental app. Inspect its origin and signature; use Apple's supported Open Anyway flow if you choose to run it. Do not disable system-wide security controls.

The wizard fills QEMU/firmware paths automatically and installs missing QEMU, swtpm, 7-Zip and GnuPG. If Homebrew is absent, its pinned official bootstrap opens in Terminal; complete macOS prompts there. Ubuntu Server ISOs download automatically; Windows uses your selected ARM64 ISO. CPU/RAM/disk and technical paths are under Advanced settings. Mobile SDK/custom tools retain their separate setup; a Terminal launch can supply those:

```sh
export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"
./FreeLane.app/Contents/MacOS/freelane-desktop /absolute/path/to/vm.toml --lang vi
# Or use a mobile device profile:
./FreeLane.app/Contents/MacOS/freelane-desktop --mobile /absolute/path/to/mobile.toml
# CLI is bundled beside the desktop:
./FreeLane.app/Contents/MacOS/freelane mobile-plan /absolute/path/to/mobile.toml
```

The desktop locates the sibling CLI automatically. QEMU remains a separate prerequisite; the wizard installs it and bootstraps Homebrew when required. MCP remains a source-checkout development workflow using `target/debug/freelane`; this standalone app does not configure MCP clients or replace the source wrapper.

Build the same package yourself on Mac with Xcode, Python3.11+ and the pinned Rust toolchain:

```sh
bash scripts/build-macos-release.sh
```

Outputs go under `target/packages/`, named with version and commit. Existing package directories are refused; move your previous output before rebuilding the same commit. The recorded Mac build passed; native GUI, HVF guest boot and mobile runtime still need testing.

## Linux cloud

An x86_64 Linux release can be built in the retained cloud:

```sh
cargo build --locked --release --workspace
```

This produces Linux executables under `target/release/`; they cannot run on Mac. The experimental cloud archive includes CLI/desktop/host agent, docs, configs and notices, while QEMU and mobile SDKs remain separate. Desktop execution needs graphical libraries/session. Its Linux runtime uses explicit TCG software emulation, without a Mac/HVF performance claim.
## Publishing a preview

Pushing a new `v*-preview.*` tag runs the release workflow. Both native Mac packaging and Windows ARM64 agent packaging must succeed before the publish job verifies their SHA256 files and creates a GitHub prerelease with ZIPs and checksums. Only that job receives `contents: write`. Main-branch builds continue to upload Actions artifacts without creating releases. Release notes are maintained in `docs/release-notes-preview.md`.

