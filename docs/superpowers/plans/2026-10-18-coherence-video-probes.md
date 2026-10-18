# Coherence Video Feasibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement inline, with one final whole-batch review. Steps use checkbox syntax for tracking.

**Goal:** Produce actual native Windows capture/H.264 and Apple Silicon decode/surface evidence before selecting the permanent Coherence video backend.

**Architecture:** Standalone experimental SDK probes, an owned generated MP4 fixture and strict portable evidence assessment. No production protocol, viewer codec dependency or automatic capture is added.

**Tech Stack:** Windows SDK C++20/C++/WinRT, Media Foundation, Swift with AVFoundation/VideoToolbox/AppKit, Python standard library, native GitHub CI.

**Spec:** `docs/superpowers/specs/2026-10-18-coherence-video-design.md`, approved by user “làm đi e”. This plan implements GateA diagnostics; a file encode or surface submission alone cannot accept latency/zero-copy/full video.

## Global Constraints

- Capture only a window created by the probe; use only generated pixels/video. No user desktop/window capture.
- New output directory only; refuse existing destinations. Bound each probe process to60seconds in CI.
- Windows fixture:320×180 NV12,30frames at30FPS, H.264 baseline/1Mbps. At most4MiB output fixture. Request low latency and0B pictures; record actual readback.
- Runtime architecture must be recorded separately from cross-compilation. x64 execution never establishes Windows ARM support.
- Mac decodes the Windows-produced H.264 MP4, checks dimensions/pixel variation, queries actual hardware decoder use and creates an owned native display layer/window. Surface submission is not onscreen pixel/zero-copy acceptance.
- No new runtime dependency/codec is shipped with the app. No permanent video protocol before backend selection.

## Review Focus

- An x64 Windows CI success must not pass the ARM runtime gate — Task1 architecture test.
- Enumerated codecs without encoded/decoded frames must not pass — Task1 counts test and Task2/3 real fixture.
- Requested hardware/low-latency settings must not masquerade as actual support — Task1 readback test and Task2/3 fields.
- Probe output must not overwrite user files — Task2 directory-refusal test.
- Native layer creation must not claim presentation pixels/zero-copy/performance — Task1 assessment test and Task3 report labels.

### Task1: Strict portable evidence contract

Files: `scripts/video/report.py`, `tests/video/test_report.py`.
Interface: `assess(windows: dict, mac: dict) -> dict`; `load(path: Path) -> dict` reads at most64KiB UTF-8 JSON and rejects wrong schema/platform/type/counts. Output contains individual boolean gates, a candidate-backend flag and explicitly false end-to-end/performance acceptance.

- [x] Write tests for valid ARM candidate, x64 exclusion, no frames, false readbacks, unsupported hardware decode, malformed/oversized reports and surface-only limitation.
- [x] Run `python3 -m unittest discover -s tests/video -v`; observe missing-module/API RED.
- [x] Implement exact validation/assessment; require all tests pass. Do not infer runtime success from a build.

### Task2: Windows capture and encode probe

Files: `scripts/video/probe-windows.cpp`, `.github/workflows/video-probes.yml` Windows job.
Interface: `probe-windows.exe NEW_DIRECTORY` writes `report.json` and generated `fixture.mp4`; refuses an existing path. Report schema1 includes platform/windows, compile architecture, WGC support/item/frame/owned-pixel proof, D3D device kind, software/hardware H.264 transform counts, encoded-frame count, codec-property readbacks, elapsed milliseconds and bounded HRESULT diagnostics.

- [x] Add native harness/workflow invoking probe and existing-destination refusal. Compile missing source first for native RED, then implement the probe.
- [x] Initialize COM/WinRT/Media Foundation with paired cleanup. Create and paint one owned window; event-driven free-threaded capture with pool2 and bounded frame wait. Verify center pixels via a staging readback, labeling this validation copy explicitly.
- [x] Enumerate encoder categories, then use a software-reference Media Foundation SinkWriter for the finite H.264 fixture. Request baseline/low-latency/0Bframes and record readbacks separately from enumeration. Do not claim raw packet latency or hardware encoding from file output.
- [x] Native x64 and Windows11ARM execution must each produce30encoded frames and a nonempty bounded fixture. Build ARM64 executable using the same SDK source; keep the ARM runtime gate open until the native runner actually completes it. Retain generated artifacts, never raw screen content.

### Task3: Apple Silicon decode/surface probe and evidence

Files: `scripts/video/probe-macos.swift`, workflow Mac job, probe docs/evidence.
Interface: `probe-macos FIXTURE_MP4` prints schema1/macOS/arm64 JSON. AVAssetReader supplies compressed samples; VideoToolbox decodes, validates dimensions/variation and reports actual hardware-decoder property. AVSampleBufferDisplayLayer in an owned NSWindow records rendering status. Diagnostic readbacks are explicit; zero-copy/onscreen pixel acceptance stays false.

- [x] Run on macos-15 after downloading only the generated Windows artifact. Decode exactly30samples, enforce bounded sample/frame sizes and runtime timeout, close the owned window/session.
- [x] Use strict assessor to combine native reports. Cross-build and runtime results stay separate; gate output cannot claim continuous video or end-to-end latency.
- [x] Run portable tests, full relevant existing checks, native workflow; one fresh review, fix Important/Critical with regressions and no re-review. Commit/push focused changes using existing authorization.
- [x] Record real run IDs/reports and go/no-go. If native ARM execution or direct packet/copy-path proof is absent, retain that concrete incomplete gate and do not ship a production video capability. Do not publish a new product release solely for experimental probes.

Execution method preserved: inline. This does not authorize calling GateB–D complete or advertising30/60FPS before actual end-to-end measurement.

## Recorded outcome

Native run39105862266, source dc71c2a: actual Windows ARM/x64 capture + H.264,
Mac ARM hardware decoding of30generated pictures and native rendering status
passed. Portable8tests, Rust222passed/9ignored, MCP14passed, strict clippy passed.
Fresh whole-batch review found two Important issues; one fix pass addressed peak
occupancy accounting and callback-context lifetime with regressions. Subsequent
native SDK/container fixes preserved strict coded-frame limits. See
[the evidence](../../coherence-video-probe-evidence.md).

**Decision:** candidate SDK backend established for further experiments. Gate A
is still incomplete because copy-path/onscreen proof is absent. No production
video capability, permanent transport protocol, 720p30 performance acceptance or
new app release is claimed by completion of this diagnostic plan.
