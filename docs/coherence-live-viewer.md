# Experimental live Windows app viewer

The Mac release enables the native-video desktop feature. A viewer negotiates
one exact Windows window and fresh stream binding, connects the dedicated binary
port before Start, and renders H.264 into its existing native view. Unsupported
agents, missing ports/helpers, decoder errors and silent streams fall back to
snapshots. Another viewer cannot take an active VM stream.

Initial output is bounded to 1280×720 at a requested maximum of 30 FPS; the CPU
reference capture path currently accepts source windows up to 1920×1080. Larger
or unsupported windows retain snapshots. These are implementation bounds, not
measured frame-rate acceptance.

Windows capture is cropped to the physical client rectangle using PMv2 DPI
coordinates and DWM frame bounds, aligning it with normalized client-area input.
Geometry or DPI changes terminate the old generation. Selection changes, resize,
minimization and explicit video toggling invalidate outstanding Start revisions.
A new session uses a random stream identity, satisfying the guest's retired
stream policy across independent viewer processes.

The native decoder retains two submissions and one latest decoded image. Busy
submission keeps its compressed access unit; receiver overflow abandons dependent
frames and requests IDR. Native layout follows egui zoom. Input remains in egui
and existing targeted guest RPCs; no uncertain input or launch is replayed.

Session control runs outside the UI thread. Drop releases the UI broker reference
before waking the final owner to Stop and join its reader. Keyframe requests are
coalesced and rate-limited. A healthy static window sends a one-second encoded
heartbeat; four seconds without a completed valid record marks the session ended,
including when QEMU keeps its socket open after the helper exits.

The Windows release ZIP includes the static-CRT ARM64 guest agent and native
capture helper, checksums, notices and setup-windows-agent.ps1. Setup pins the
helper to an immutable hash-named sibling. Update, rollback and uninstall verify
managed bytes and preserve unrelated files. Older manual fixed-sibling helpers
remain supported; missing helpers leave snapshot capability intact.

## Verification and limits

Native Windows ARM64/x64 tests verify 90 captured client-area frames, IDR recovery,
finite pending inputs, target lifecycle rejection and output/progress deadlines.
Native package CI extracts the actual ARM64 ZIP, installs both components, runs
owned helper capture and uninstalls; paired lifecycle tests cover update, rollback,
corruption refusal, locked files and preservation.

Native Mac CI feeds actual Windows-captured packets through an owned local
serial/control socket emulator, production LiveSession/VideoBroker/ViewerVideo,
and the hardware decoder in eframe's actual NSView. This is transport and renderer
integration evidence, not a running Windows VM test. The SDK harness additionally
checks NV12 pixels and asynchronous close/recreate. Portable regressions cover
production guest restart policy, silence versus heartbeats, stale attempts,
coalescing, blocked-control retirement and snapshot/input preservation.

Physical-Mac installed-VM input alignment, DPI/multiple displays, sleep/wake,
onscreen pixels, native-view/egui popup overlap and sustained CPU/latency/frame-rate
measurements remain pending. The owner explicitly deferred physical Mac testing.
No zero-copy, all-app compatibility, 30 FPS acceptance or Parallels performance
comparison is claimed. Existing full-VM desktop display remains snapshot-based.

Verified source `54d27aa`: [native Windows/Mac run](https://github.com/puma2k/freelane/actions/runs/39216051037)
and [full source checks](https://github.com/puma2k/freelane/actions/runs/39216051154)
passed. The production live-viewer fixture decoded 30 pictures and presented 29,
with hardware readback and exact Stop acknowledgement. Portable checks passed
284 Rust tests (nine ignored), 16 video tests and 14 MCP tests. See the
[structured evidence](evidence/coherence-live-viewer.json) for scope and reports.
