# Live Coherence viewer integration — next implementation batch

Continue after the embedded native surface passes native SDK and Rust/eframe CI.
Owner authorized continued implementation and push; physical-Mac acceptance stays
deferred. Execute inline, one fresh whole-batch review after implementation.

Goal: normal app viewer can receive an authorized Windows window video stream,
render it in its existing Mac window and fall back cleanly to snapshots.

## 1. Align client-area capture with input

Current WGC worker captures window chrome; guest pointer and snapshot operations
use client coordinates. Correct this before live activation.

- Add checked portable crop bounds and BGRA conversion tests: padded stride,
  nonzero client offset, odd client size, out-of-range and truncated surfaces.
- Query physical client origin via GetClientRect/ClientToScreen and WGC frame
  origin via DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS). Require frame
  dimensions to agree, crop entirely inside the captured texture, and fit the
  client aspect ratio into negotiated even coded dimensions.
- Establish the helper's DPI awareness before capture. Validate movement,
  client-size/style/DPI changes; reject changed geometry as a generation change
  instead of encoding mismatched coordinates. Preserve exact target identity.
- Extend owned native Windows tests to assert client dimensions and landmark
  pixels. Keep strict lifecycle proofs and encoder/output deadlines intact.

## 2. Distribute the actual helper

- Build the static-CRT ARM64 MSVC helper on native Windows CI and package it as
  freelane-video-helper.exe beside the guest agent. The current Linux GNU agent
  release build cannot compile Windows SDK C++ capture code.
- Extend installer verification/copy/rollback/uninstall for the sibling helper.
  Reject missing/corrupted helper consistently; absence retains snapshot support.
  Preserve existing agent installation and unrelated files on setup failure.
- Include notices/checksums and test artifact installation on owned native CI.

## 3. Session owner and viewer

- Connect the broker before Start; use fresh exact binding and correlated target.
  Add event-driven broker wakeup for records/errors. Start/Stop/keyframe RPCs run
  outside the UI thread; input stays serialized with existing guest RPC behavior.
- Use one owner/control task with coalesced keyframe and stop requests. A dropped
  viewer signals stop immediately; owner keeps the lease until bounded cleanup.
  Stale start replies retire their own binding without touching a newer session.
- Attach Surface to eframe's actual NSView on the UI thread, map egui coordinates
  through zoom/native scale, and preserve egui hit testing. Busy submission keeps
  one pending compressed record; queue overflow abandons dependent frames and
  requests IDR through the existing receiver policy.
- Inventory/selection/resize/minimize retires the old stream and surface; restoring
  negotiates a fresh generation. Visible unfocused windows keep video running.
- Advertise video only after hardware decode/presentation evidence. Missing port,
  helper, hardware support or failed stream retains snapshot controls with a short
  status. Do not retry mutating input or relaunch the guest app automatically.

## Acceptance

Portable session race/backpressure/cancellation tests, existing Rust/MCP suites,
native ARM64 Windows helper tests and Mac actual-view fixture are required.
Record end-to-end transport/presentation evidence separately from file fixtures.
Physical Mac real-app video, input alignment at multiple DPI/zoom levels,
sleep/wake and sustained CPU/latency measurements remain explicit acceptance
checks; a successful file decoder cannot mark them complete.
