# Persistent Windows Video Worker Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans inline; one fresh whole-batch review. Continue the owner's approved video architecture and existing commit/push authorization, without another handoff pause.

**Goal:** Replace the 30-frame fixture-only encoder with an owned persistent native worker and guest process adapter.
**Architecture:** A sibling Windows SDK executable receives an 80-byte private setup packet over stdin, reports readiness on stdout and writes FLV1 only to the fixed video virtio port. Rust retains the worker across RPC reconnects and kills only that owned child on failure/stop. Native owned-window tests exercise the same capture/codec loop without requiring a virtio driver.
**Tech Stack:** Rust std/process, WinRT WGC, D3D11 CPU staging, synchronous low-latency software MFT, existing FLV1 converter. No new codec dependency.
**Spec:** `docs/superpowers/specs/2026-10-18-coherence-video-design.md`.

## Constraints and review focus

- One worker per Session; exact HWND/PID/process creation/property incarnation validation before capture and while streaming. Never launch or terminate the captured app.
- Initial output is bounded by requested dimensions (even, <=1920×1080), 30 FPS maximum, baseline H.264, one encoder worker, verified low latency and zero B frames.
- Latest captured frame only, capture pool2, encoder pending<=2; no arbitrary encoded-frame dropping. Encoder pressure or IO stall terminates this stream, requiring fresh binding.
- Writes use overlapped IO and a 500ms deadline. Cancellation itself has a finite deadline; terminate the helper if a driver will not complete cancellation rather than free pending OVERLAPPED memory.
- READY (`R`) precedes host go (`G`); STARTED (`S`) only after first encoded picture has been written. Keyframe request (`K`) is coalesced until native acknowledgement (`K`); EOF/stop (`X`) releases worker, not app.
- Rust startup timeouts bounded, startup status reads bounded; never hold an unbounded status queue or fill stdin with repeated keyframe requests. Unexpected stdout or early exit fails closed. Retain ownership for cleanup on all partial-start errors.
- Helper absence keeps capability off. Installation/release packaging and native Mac app viewer attachment remain separate milestones; do not label this app-level video acceptance.

### Task 1: Reusable streaming primitives and private setup contract

Files: `scripts/video/stream-writer.hpp`, `bgra-nv12.hpp`, `worker-setup.hpp`; `crates/guest-agent/src/video_process.rs`; portable tests.
Setup layout (big endian, 80 bytes): `FLW1` [0..4], size80 [4..6], reserved0 [6..8], HWND [8..16], PID [16..20], reserved0 [20..24], created [24..32], incarnation [32..40], nonce [40..56], stream [56..64], generation [64..72], width [72..74], height [74..76], fps30 [76], reserved0 [77..80].
`StreamWriter(write_callback, binding, width, height)` emits one config plus unlimited pictures using strict monotonic sequence/PTS and IDR-first. Callback receives one bounded complete record; never impose fixture's 30-frame/4MiB lifetime cap. Keep existing fixture writer behavior unchanged.
`bgra_nv12` gains explicit even output dimensions with existing 320×180 defaults; worker fits actual window aspect ratio inside request bounds.

- [x] Write failing portable tests for >30 records, custom binding/dimensions, invalid setup/reserved fields/bounds, no duplicate config and stalled output errors. Write missing Rust setup encoding test.
- [x] Implement primitives and setup encoding; verify portable suite and guest suite; commit.

### Task 2: Persistent SDK worker and native owned-window validation

Files: `scripts/video/worker-windows.cpp`, `worker-capture.hpp`, `worker-codec.hpp`, `.github/workflows/video-probes.yml`, protocol offline validator.
Production worker takes no arguments and only opens `\\.\Global\org.freelane.video.0`. Self-test mode creates only its own window and writes a new destination; same capture, conversion, codec and stream writer. Capture closes on identity change/resize/minimize. Cache latest NV12 for initial encoder priming and at-most1FPS idle heartbeat; do not claim measured idle savings.

- [x] Add native build/run assertions requiring 90 pictures (beyond old fixture cap), valid configuration, varying captured pixels and at least two IDRs after forced keyframe; malformed target never starts.
- [x] Implement event-driven latest-frame capture, reused staging texture and bounded encoder loop. Validate target before setup and every encode/heartbeat. No generated input on capture failure.
- [x] Run owned pipe-stall test using the same overlapped writer, expect bounded helper failure while parent survives. Validate output with Rust receiver. Native CI is required for SDK compilation/runtime; record unrun platforms honestly.
- [x] Commit native worker; preserve existing generated/captured native probe evidence independently.

### Task 3: Guest process ownership and Windows dispatch

Files: guest `video_process.rs`, `windows.rs`, `lib.rs`, tests `video_process.rs`.
`ProcessEncoder` implements `video::Encoder`, configured with helper Command and authorization callback; startup writes setup, receives R, reauthorizes target, sends G, receives S. Reader handles only R/S/K and EOF with bounded state. Stop closes stdin and kills/waits only its child, joins status reader. Keyframes coalesce until K acknowledgement; no command replay.
Windows stores a single `Session<ProcessEncoder>` for agent lifetime and looks for sibling `freelane-video-helper.exe`; absent/nonregular helper remains unsupported. Normal control reconnect does not drop Session.

- [x] Owned subprocess tests: R/G/S ordering, startup timeout and wrong status, stale identity after R, early exit, repeated keyframes, stop/drop preserving unrelated child. Observe RED before implementation.
- [x] Implement adapter and conditional Windows forwarding; run guest/whole workspace, fmt/clippy, MCP/portable video suite.
- [x] One fresh review; fix Important/Critical once with failing regressions, record limits/evidence, commit and push. No release/video-performance claim.

Evidence: native worker aa18d08 run39190796705 Success (ARM/x64, strict lifecycle + deadlines); repository c726a4b run39191978602 Success (native Windows guest adapter + all source CI). Local Rust267/0/9ignored, portable15, MCP14, clippy/fmt pass. Final review two Important fixed by RED→GREEN health/lifecycle regressions + native proof; no re-review. One deferred minor: duplicate-config assertion uses an already-poisoned writer.
