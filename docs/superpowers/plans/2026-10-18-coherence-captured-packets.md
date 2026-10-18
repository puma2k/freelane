# Captured Window Packet Integration Plan

> **For agentic workers:** Continue approved video design using superpowers:executing-plans inline; one fresh final review. Existing user authorization covers continued implementation and push. This is a native validation milestone, not production feature activation.

**Goal:** Prove owned WGC window pixels survive the actual direct H.264 packet path and native Mac decoder.
**Architecture:** Feed changing pixels from an owned Windows test window through the existing direct MFT encoder. Preserve generated-fixture checks and assess captured output separately. Production launchers stay unsupported until persistent worker and viewer integration.
**Tech Stack:** Existing WinRT WGC, D3D11 staging readback, synchronous software MFT, portable C++ converter tests, native ARM Windows/Mac CI.
**Spec:** `docs/superpowers/specs/2026-10-18-coherence-video-design.md`.

## Constraints and review focus

- Capture only the helper's own window; no application lookup or arbitrary HWND input.
- Keep one latest WGC frame, two frame-pool buffers, finite three-second frame wait; close replaced frames and drain callbacks safely on cleanup.
- Validate BGRA dimensions, stride and conversion bounds before reading; copy pixels while texture is mapped and unmap on error.
- Exact center shade matching must reject stale/repeated frames after repaint; no generated NV12 substitution on capture failure.
- Separate captured and generated fixture reports. CI must assert captured 30 frames/one config and native decode variation; never infer product continuous-video performance from this small CPU-readback fixture.

### Task 1: Portable BGRA-to-NV12 conversion

Files: `scripts/video/bgra-nv12.hpp`, `tests/video/test_bgra_nv12.py`.
Interface: `Bytes bgra_nv12(const uint8_t*, size_t bytes, size_t stride, unsigned width, unsigned height)` produces scaled 320×180 limited-range BT.601 NV12; validates input up to 1920×1080. Use nearest-neighbor scaling for this correctness fixture.

- [x] Add compiled portable test: gray Y/neutral UV, padded stride, chroma order, invalid buffer/stride/dimensions; observe missing-header RED.
- [x] Implement converter; run full portable suite; commit.

### Task 2: Owned capture feeds actual encoder

Files: `scripts/video/captured-frames.hpp`, `direct-encoder.hpp`, `probe-windows.cpp`, `.github/workflows/video-probes.yml`.
Interface: `CapturedFrames::next(unsigned)->Bytes`; optional source callback and filename on `direct_encode`. Report separate captured frame count, packet count, config count, HRESULT.

- [x] Add native CI assertions for captured fixture (initially missing report values fail).
- [x] Capture repainted owned window through latest-slot WGC, validate center shade, map staging texture and invoke Task 1 conversion. Feed callback buffers to existing direct MFT without replacing generated fixture.
- [x] On ARM Windows, assert 30 captured frames, 30 packets and one configuration. Rust validates captured FLV1; Mac decodes captured fixture and verifies 30 changing frames and hardware decode. Exercise injected asynchronous failure for raw input too.
- [x] Run portable tests, full Rust checks as appropriate; one fresh review; fix Important/Critical once, record actual native results, push. No product/release/performance claim.

Evidence: native run39141763259 passed at9ff4a63. All three raw/MP4 input failure paths exercised; portablevideo11 passed, LinuxRust261 passed/9ignored, clippy/fmt passed. Fresh reviewer found no Important/Critical findings. Deferred minor: decoder verifies variation rather than exact shade ordering. Separate existing Mac broker EOF test was diagnosed at timeout-option setup after disconnect (native panic line257). Replaced SO_RCVTIMEO with bounded poll plus nonblocking read; Mac/Linux portable jobs passed atdcef55e inrun39142683488. Direct kernel close-on-exec assertion also passes. Portable suite is now12 including privacy-scoped CI diagnostics; LinuxRust261 passed/9ignored.
