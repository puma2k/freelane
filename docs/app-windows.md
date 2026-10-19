# Windows app windows and Dock launchers

FreeLane opens ordinary Win32 applications in separate Mac viewer windows and can export named `.app` launchers for the Dock. Current Mac builds use hardware-decoded H.264 window video when the updated guest backend is available, with JPEG snapshot fallback. Closing a viewer stops only its video session; the Windows app and VM keep running.

The live path captures Windows client pixels, uses the dedicated binary virtio-serial port, and embeds a native NV12 VideoToolbox/AVSampleBufferDisplayLayer surface in the existing viewer. It supports one active video stream per VM. Other viewers retain snapshots; they do not take over the active encoder. Enable **Live video** in the viewer, or turn it off to use snapshots.

This remains experimental. Native CI proves actual Windows-captured payloads through production host session/transport/viewer code and hardware decode; it does not establish physical-Mac screen pixels, all-app compatibility, sustained performance or Parallels Coherence parity. See [live integration evidence and limits](coherence-live-viewer.md).

## Matching builds

Use the Mac and Windows ARM64 packages from the same release. Preview.24 enables this integration; preview.23 and earlier retain their snapshot viewer. The Windows package now includes both the guest agent and native capture helper.

## Prepare Windows

Use the matching Windows ARM64 guest-agent build from this revision. Set `guest_agent=true` in the VM's `[runtime]` configuration and install the compatible signed ARM64 virtio-serial driver as described in [guest-agent setup](guest-agent.md). Run the agent in the signed-in user's interactive session. It must be available after login for Dock quick-open; the launcher does not log in or obtain credentials.

After verifying and extracting the release ZIP, run this in ordinary-user PowerShell:

```powershell
powershell.exe -NoProfile -File .\setup-windows-agent.ps1
```

Setup installs the agent and matching checksum-pinned ARM64 capture helper together and creates the per-user Startup shortcut. Run the same command after stopping the installed agent to update; explicit rollback restores the old pair. It does not elevate, install drivers or enable autologin. Sign in again to start the agent. See [install/update/rollback/uninstall](agent-lifecycle.md).

A VM started before the video port was provisioned must be restarted when convenient. FreeLane leaves it running and uses snapshots until then. No Windows ISO reinstall is needed for the host video feature.

Protocol v1 ping/launch remain compatible with the old agent. The viewer first probes the optional **v2 window extension** with a read-only ping; an old agent returns a version error and **no app launch is sent**. Install the updated agent before using app windows. Linux agents return `window_snapshots=false` and retain normal v1 operation.

## Save an application

In the manager's **Applications / Ứng dụng** tab for a Windows VM, enter the `.exe` and literal arguments, then fill in **App name** and a **new app profile path**. Choose **Save app profile** or **Save & open separate window**. Existing profile files are refused. On Mac, enter a new `.app` path and choose **Export Dock launcher** using the saved profile.

Alternatively copy [the Notepad example](../configs/app.notepad.example.toml):

```toml
name = "Windows Notepad"
vm = "/absolute/path/to/windows/vm.toml"
executable = 'C:\Windows\System32\notepad.exe'
arguments = []
boot_timeout_seconds = 600
```

Relative VM paths resolve against the profile. Arguments are literal strings, not shell text. Startup waits 10–900 seconds as configured (default 600); no launch is retried after an uncertain response. If readiness times out, the VM is left running for inspection. Paused/stopping VMs must be resumed explicitly. A VM startup lock avoids duplicate starts from simultaneous app viewers; ordinary QEMU locking remains active.

```sh
./target/debug/freelane-desktop --app /absolute/path/to/notepad.app.toml
# macOS only, creates and ad-hoc signs a new native launcher:
./target/debug/freelane app-export /absolute/path/to/notepad.app.toml '/Users/you/Applications/Windows Notepad.app'
```

The launcher contains copied CLI/desktop executables and a private saved profile with the canonical VM path. It has its own bundle identifier/name so it can be pinned independently in the Dock. Its icon is currently macOS's generic application icon; Windows icon extraction is pending. Moving the VM breaks that saved path; update/re-export explicitly. Exported launchers retain their copied binary version, so re-export to a new path after upgrading FreeLane. Use macOS14+; signing is ad-hoc, not Developer ID/notarization. The same [release security and backend prerequisites](release.md) apply.

When launching a stopped VM from Finder/Dock, the supervisor retains the inherited PATH and adds existing `/opt/homebrew/bin` and `/usr/local/bin` tool locations. Explicit backend paths in the VM config remain preferable. The launcher does not install QEMU, firmware, Windows, drivers or mobile SDKs.

## Interact with the viewer

- Select the desired visible window for the launched PID; refresh the list for dialogs in the same process.
- Click inside the captured image to send a mapped primary click and focus the host image.
- Type after focusing the image, or use **Gửi chữ** to send up to 1024 UTF-8 bytes to the guest window's focused control. Enter/Tab/navigation/Delete/Backspace/Escape are supported.
- With video unavailable or disabled, use **Refresh frame** or periodic refresh. An acknowledged guest capture timeout gets one read-only retry after a one-second pause. Other errors, or a second timeout, stop automatic refresh; pending unsent input is canceled and its count is reported. Inspect before sending another mutation.

Every guest window target includes HWND, PID and process creation time. The agent revalidates these before acting, so a recycled handle/PID is refused. Frames are bound to the selected identity, preventing late captures from a previously selected window from being displayed as another app. Input messages are delivered to that window/child control rather than injected globally. Delivery acceptance does not prove that an application used the input.

**Closing the Mac viewer leaves the Windows app and VM running.** Use the full VM display for unsupported menus/save/exit; unsaved-document dialogs remain the guest's responsibility. Reopening a closed viewer requests a new launch, which may create another app instance. Single-instance apps or launchers that delegate to another PID may not expose windows under the returned PID; inspect the VM rather than automatically launching again.

## Limits and validation

Capture uses Win32 `PrintWindow` in a bounded background worker, with at most one outstanding capture. A hung application may keep that worker pending; further captures fail until it completes. The agent still returns errors rather than leaving the serial request unbounded. Raw rasters are limited to 16 million pixels; JPEG previews adapt down from at most640×480 to fit 28 KiB, hex-encoded inside the existing64 KiB frame limit. Text can be less crisp than a live high-resolution stream.

Minimized windows must be restored inside Windows. UAC/secure desktops, protected video, GPU-heavy apps, games, cross-process windows and programs ignoring posted Win32 messages may fail or produce a blank frame. Older agents support only primary click/basic keys. Updated v10 agents add targeted right-click, double-click, wheel, a bounded drag gesture, guarded foreground shortcuts and explicit client-size requests. Continuous video, seamless resizing, host clipboard integration in the viewer, audio and native app icons remain unfinished. This mode does not create guest graphics acceleration or claim frame-rate performance.

Portable tests cover profile/bundle preservation, literal arguments, v1/v2 negotiation and wrong-window response rejection. Windows ARM64 builds/checks include the GDI/User32 branch. Native Mac release packaging additionally exports an offline Dock fixture, lints its plist and verifies its signature; it does **not** boot that fixture or claim Windows/Dock interaction. Recorded real guest tests are reported separately when available.

## Recorded Windows cloud observation

The updated ARM64 agent actually answered v1 ping and v2 window capabilities in the installed Windows Audit session through virtio serial. One Notepad launch returned PID6456; its visible window was identified by HWND131418 and process creation time. A separate640×366 JPEG of the real client area was returned (15,375 bytes). Targeted click/text requests were accepted, but the cloud session interrupted its processes before the after-input capture, so text rendering and the live host viewer were **not** validated in that session. This is not a normal-user, native Mac/Dock or stability result.

![Actual Notepad client-area JPEG from the Windows agent](assets/windows-app-client.jpg)

[Recorded partial acceptance](evidence/windows-app-window.json) preserves the successful capture and the interrupted input verification separately.

## Host viewer replay check

After the live guest session interrupted, the genuine captured Notepad JPEG was decoded and displayed by the host viewer over a read-only local RPC fixture. This verifies the host JPEG/render path and UI layout, not Windows input behavior or a live guest connection.

![Host viewer rendering the recorded Windows client frame through a labeled replay fixture](assets/windows-app-viewer-replay.png)

An explicit `freelane-desktop --app profile.toml --pid PID` can attach to the specified guest process without another launch; use a fresh verified PID, not a PID retained across reboot.

## Resumed live Windows acceptance

The previously interrupted input check was completed in a new Windows Audit session under Linux TCG. The new agent returned a real Notepad client frame, accepted a targeted click/text request and returned another frame visibly containing `FreeLane live window test 2026`. The Linux host viewer then attached to that verified PID through the real virtio channel. A host UI text submission visibly inserted `ui verified` into Notepad. Closing the viewer left both the guest window and VM running.

Periodic refresh encountered a guest capture timeout. The viewer now retries that acknowledged read-only snapshot once after one second; persistent timeout and transport failures stop refresh, with no input replay. A read-only fixture verified exactly two captures before stopping on persistent timeouts. The updated viewer still refreshed the genuine guest frame after a 20-second observation. These checks establish this narrow input/display path; they do not establish smooth video or sustained stability.

![Real Windows client after targeted input](assets/windows-app-input.jpg)

![Host viewer connected to the real Windows agent after host UI text entry](assets/windows-app-viewer-live.png)

[Live acceptance evidence](evidence/windows-app-live-acceptance.json) records the scope. This remains an Audit-session Linux test, without normal-user Startup installer or native Mac/Dock runtime validation. The snapshot retry change is on `main`; the earlier `preview.2` DMG predates it.

## Window identity follow-up (newer than preview.16)

Updated agents negotiate v9 window incarnations. Profiles without arguments reattach to a visible matching process image before sending a new launch. Profiles with arguments retain one literal-argument launch; different-image delegates remain unsupported. [Implementation and validation scope](plans/p3-window-identity.md).

## Extended input (preview.17)

The viewer negotiates `window_input` v10. Click/right-click/double-click and wheel map to client coordinates. A drag is delivered as one bounded gesture on release, with16 interpolated moves; it is not a continuous native pointer stream. `Guest window size` requests a client width/height of64–8192 pixels; Windows/app constraints may limit the result. Selecting another window discards queued input.

Mac Command maps to guest Control. Shortcuts require the exact selected guest root to already be foreground and no guest modifier/requested key held. FreeLane refuses otherwise, without stealing focus or bypassing Windows restrictions. `Copy in guest`/`Paste in guest` use the guest clipboard; they do not transfer the host clipboard. Host sharing still uses the manager's separate opt-in controls. Closing the viewer preserves the guest app. Input errors stop refresh and discard unsent actions; inspect the guest before explicitly retrying.

![English viewer with extended controls](assets/app-input-en.png)

These controls were exercised against an owned Linux RPC fixture displaying a recorded real Notepad frame, not against a live guest. Protocol bounds, legacy refusal, key mappings and native Windows integration coverage have distinct evidence; see [input evidence](evidence/p3-window-input.json). Native Mac app acceptance remains pending.

A native integration limitation remains: egui may consume Cmd/Ctrl+V without delivering a viewer event when the host clipboard has no text. Use **Paste in guest** in that case; no host clipboard text is forwarded by these shortcuts.

## Snapshot scheduling

The updated viewer keeps one request in flight and at most32 queued inputs. When live refresh is on and a frame is due, it gives capture a turn after at most8 completed input actions even if more input is queued. A slow request still blocks the worker until it completes or times out; this bounds scheduling fairness, not wall-clock latency or FPS. Live-off still allows input and explicit Refresh frame.

Switching selected windows clears unsent input, drag and old texture; late frames must match the current target. A failed request stops live refresh and discards unsent input without replaying uncertain operations. The previous conditional read-only capture-timeout retry is unchanged. [Scheduling evidence](evidence/p3-viewer-scheduling.json) separates deterministic policy tests and owned native Linux/synthetic-peer acceptance from pending installed Windows/Mac acceptance.

## Identical frames

The optimized host viewer skips JPEG decoding and texture recreation for an exact match to the previous validated frame/window identity. Changed frames still decode normally; selection changes/errors invalidate the key. A manual unchanged reply can restore a cleared texture locally, without extra guest I/O. [Implementation and measured scope](viewer-optimization.md). Guest capture/encoding, bandwidth and snapshot cadence remain unchanged.
