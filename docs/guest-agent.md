# Experimental Windows and Linux guest agent

Linux support uses `/dev/virtio-ports/org.freelane.agent.0` and absolute executable paths. A static ARM64 build, real Linux-host stdio launch tests and actual installed Ubuntu Server ARM64 transport/workloads pass. Ubuntu ran the agent in its normal UID 1000 console session and retained tested bytes after reboot; graphical Linux sessions and native Mac remain unverified. See [Ubuntu setup](ubuntu.md). The Windows instructions and existing driver behavior below remain applicable to Windows.

FreeLane has an opt-in request channel for pinging a Windows guest and launching an executable in the agent's interactive user session. **The ARM64 executable has run through actual virtio serial in both WinPE and the installed Windows Audit session.** Normal PnPUtil staged the signed ARM64 vioserial package in the installed OS; after a clean cold boot, host ping returned Windows/aarch64 and literal CLI launch opened 7-Zip with the intended archive. The native FreeLane GUI also received ping and opened an actual Notepad window. Native Mac and normal user-session setup remain unverified. Portable tests use a fake launcher; they do not replace the Windows runtime experiment.

## Build and enable

A Linux x86_64 machine can also create the ARM64 executable:

```sh
bash scripts/build-windows-agent.sh
```

The script downloads pinned LLVM-MinGW with a published SHA-256 check, or uses the compiler explicitly selected by `FREELANE_LLVM_MINGW_ROOT`. It links unwind support statically. Output: `target/aarch64-pc-windows-gnullvm/release/freelane-guest-agent.exe`. This build has been linked, inspected as ARM64 and run through the `--stdio` path in Windows PE. It does not require a separate libunwind DLL. Preserve LLVM-MinGW runtime license notices when distributing a built executable.

For protocol diagnostics, `freelane-guest-agent.exe --stdio` reads requests from stdin and writes responses to stdout until EOF. This uses the same validated handler and process launcher; it does not create a network listener or replace the normal guest channel. Do not feed requests from untrusted sources into an interactive user's agent.

On a Windows development machine with Rust and the ARM64 MSVC toolchain/SDK:

```sh
rustup target add aarch64-pc-windows-msvc
cargo build --locked -p freelane-guest-agent --release --target aarch64-pc-windows-msvc
```

Copy `target/aarch64-pc-windows-msvc/release/freelane-guest-agent.exe` into the Windows VM. Install a trusted, appropriately signed **ARM64** virtio-serial driver compatible with the guest first. No driver is bundled or signature enforcement bypassed. An x64 driver is not an ARM64 kernel driver.

Set `guest_agent = true` under `[runtime]` in the host's `vm.toml`, then restart that VM. This adds a virtio-serial port named `org.freelane.agent.0`, backed by `agent.sock` in its private runtime directory. The feature defaults to off and exposes no network listener.

Run the agent inside the signed-in Windows user's session:

```powershell
.\freelane-guest-agent.exe '\\.\Global\org.freelane.agent.0'
```

The device path is the expected virtio-serial symbolic name and must be checked against the installed driver's device naming. The agent retries opening the channel once a second when disconnected. Running it as a Session 0 service will not put GUI app windows into the interactive desktop; this implementation does not install a service or elevate privileges.

Installed Audit verification staged the existing signed package with `pnputil /add-driver E:\serial\vioser.inf /install` (exit0, published oem1.inf), copied the compiled agent to `C:\FreeLaneTest\freelane-agent.exe`, then shut down Windows cleanly before enabling `runtime.guest_agent=true`. The current ARM PCI root bus rejected live serial hotplug; cold startup was required. After login, the agent was started manually in the interactive user session with `conhost.exe C:\FreeLaneTest\freelane-agent.exe`, without `--stdio`. A fresh correlated host ping succeeded, followed by an actual 7-Zip archive window and GUI Test reporting no errors after reboot. This does not configure automatic startup; run the agent in the intended interactive session after each boot. These paths describe local tests, not a bundled driver or production installer.

From the host:

```sh
cargo run --locked -- agent-ping /absolute/path/to/vm.toml
cargo run --locked -- launch /absolute/path/to/vm.toml 'C:\Windows\System32\notepad.exe' 'C:\Users\User\Documents\notes.txt'
```

Launch uses an absolute `.exe` path and a literal argument list via Windows process creation. It does not automatically wrap commands in `cmd.exe` or PowerShell. The returned PID means process creation succeeded, not that its UI opened or stayed healthy. Child stdin/stdout/stderr are disconnected from the agent protocol; console output and interactive terminal sessions are not streamed. Apps render in Windows. An optional [separate-window video/snapshot viewer and Dock launcher](app-windows.md) can present ordinary Win32 client windows on the host with an updated agent.

A host response timeout leaves execution outcome unknown. The client does not automatically retry launch requests, which could otherwise open a second process.

## Protocol and validation

On current main, `freelane agent-capabilities /absolute/path/to/vm.toml` and the desktop Details probe query version 3, then fall back to versions 2 and 1. Discovery sends only Ping requests. Version 3 reports the agent package version, OS, architecture, advertised protocol versions and bounded feature metadata. Version 2 reports snapshot support; other unreported legacy features remain unknown. Failed probes clear cached metadata. These changes postdate preview.6.

The current v3 agent has passed real virtio discovery on a controlled Ubuntu ARM64 TCG VM, with a separate test confirming UID1000 execution and v1/v2 replies. See [capability evidence](evidence/agent-capabilities.json). Discovery does not install the agent, grant permissions or establish that a guest desktop is usable. Clipboard and file transfer are not advertised until implemented. Each operation still uses its own supported protocol; version 3 rejects mutations.

Protocol version 1 uses newline-delimited UTF-8 JSON, capped at 64 KiB per frame. Requests have a nonzero ID; responses must echo that ID and the version. Unknown fields, malformed/truncated/oversized frames, NUL characters, excessive argument counts and unsupported versions are rejected. Host reads have a five-second total deadline.

```json
{"version":1,"id":1,"action":{"type":"launch","executable":"C:\\Windows\\System32\\notepad.exe","arguments":[]}}
{"version":1,"id":1,"outcome":{"status":"launched","pid":1234}}
```

The example PID is a protocol illustration, not a test result. Ping responds with guest OS and architecture. Launch errors return a structured failure; the host propagates it as a nonzero CLI exit. Requests carry no host credentials.

Run `cargo test --locked --workspace` to include the shared protocol and guest request-handler suites. The QEMU UEFI test verifies that the configured virtio-serial channel is attached and its socket is cleaned up; it does not contain a Windows driver or execute the Windows agent. `cargo check --locked -p freelane-guest-agent --target aarch64-pc-windows-msvc` checks the Windows ARM64 branch but does not link an executable or run it.

Clipboard, agent-mediated shared-folder access, guest service installation, live Coherence-style video and save/restore are still outstanding. The experimental snapshot window extension is described in [app windows](app-windows.md). A separate [read-only USB host-folder transfer](sharing.md) does not use the agent.

## Real transport and app evidence

The Linux cloud experiment used memory-mapped modern virtio serial (legacy port I/O disabled), the ARM64 w11 vioserial driver from the official UTM v4.4.4 guest-tools ISO, and the cross-linked agent. Catalog issuer metadata alone was not considered proof: WinPE drvload completed and actual host requests subsequently received correlated replies. No manual trust certificate or signature-enforcement bypass was used. This older package is an experimental baseline, not an automatically bundled driver; preserve its provenance and notices and assess release dependencies separately. NetKVM connectivity has not been tested.

Both CLI and native FreeLane GUI launched verified 7-Zip 25.01 ARM64 in WinPE. It created an archive, extracted the file, and both individual SHA-256 hashes matched the original host-side source. GUI Launch app opened an archive named `X:\FreeLane a & b.7z` as one literal argument. A process-start response was accompanied by an actual window and correct file entry. WinPE lacks fc, so that initial comparison failed; the corrected check used 7-Zip SHA-256 rather than hiding the failure. This does not establish x64/x86 translation or a full installed Windows desktop.

Host requests use fresh nonzero OS-random IDs across CLI processes, so delayed responses cannot normally match a new invocation. An uncorrelated response is rejected. IDs correlate responses; they are not an authentication mechanism. The private socket and the agent's own Windows user permissions remain the trust boundary.
