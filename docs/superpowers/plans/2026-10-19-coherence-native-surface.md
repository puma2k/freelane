# Native Mac Video Surface Implementation Plan

> **For agentic workers:** Continue superpowers:executing-plans inline with one fresh final review. Owner already authorized continued video implementation and push. Existing isolated cloud workspace; native tests run on owned CI windows.

**Goal:** Provide a hardware-decoded NV12 native surface embedded in an existing Mac NSView, ready for app-viewer integration.
**Architecture:** Swift C ABI wraps one VideoToolbox decoder and an AVSampleBufferDisplayLayer child view. Rust owns the opaque context on the UI thread. Two compressed inputs may be in flight, one latest decoded image waits for presentation. No CPU decoded-pixel readback in production.
**Tech Stack:** Existing Rust/eframe, Swift, AppKit, VideoToolbox, AVFoundation. No new codec dependency.
**Spec:** `docs/superpowers/specs/2026-10-18-coherence-video-design.md`.

## Constraints / Review focus

- Raw NSView borrowed only on UI thread; native child is transparent to hit testing. Layout uses host-view points and respects flipped coordinates.
- Strict avcC/baseline, complete parameter bounds and expected dimensions; sample size<=2MiB, coded dimensions<=1920×1080, monotonic PTS representable by CMTime.
- Hardware decoder required/read back; NV12 IOSurface requested, never decode to RGB/upload egui textures in this path. Do not claim measured zero-copy.
- Two in-flight decode requests; latest decoded frame replacement is safe, encoded reference frames are never silently dropped. Submission returns busy before exceeding the limit.
- Retain callback context until asynchronous frames are drained and decoder invalidated. Detach immediately on close; defer draining off UI thread. One process decoder budget remains reserved until retirement completes.
- Old callbacks cannot present after close; decoded timestamp regression cannot replace a newer image; bound main-queue presentation work to one pending callback.
- Native harness must exercise actual surface attachment, hardware decode, close during asynchronous decode and repeated recreation. No app video/performance claim until host session/UI wiring and real guest transport are exercised.

### Task 1: Swift native surface and owned SDK harness

Files: `crates/desktop/native/video-surface.swift`, `scripts/video/surface-probe.swift`, existing `.github/workflows/video-probes.yml`.
C ABI `freelane_video_create(NSView*)->context*`, `configure(context,avcC,len,width,height)->i32`, `submit(context,AVCC,len,pts,idr)->i32` (1 accepted,0 busy,-1 failed), `layout(context,x,y,w,h,visible)`, `stats(context,u64[8])`, `destroy(context)`.
Stats: submitted, decoded, presented, superseded, in-flight, hardware-readback flag, error-code, reserved.

- [ ] Add native CI build/runtime checks for actual NV12 native surface, real Windows 30-picture captured fixture, hardware readback and callback-safe close/recreate; observe missing-source build RED.
- [ ] Implement surface with strict input parsing, finite decode/presentation slots and asynchronous retirement. Harness validates decoded pixels through internal state only; production performs no pixel readback.
- [ ] Run portable suite; native Mac compile/runtime required; commit.

### Task 2: Rust UI-thread wrapper and native build linkage

Files: desktop `build.rs`, `src/native_video.rs`, module declaration/dependency configuration and `examples/native_video_surface.rs`.
`Surface::attach(rawNSView)->Result`, `configure(&Record)`, `submit(&Record)->Result<AcceptedOrBusy>`, `place(rect,visible)`, `stats()->Stats`; `Surface` is neither Send nor Sync. Drop destroys context on UI thread. Non-Mac build returns unsupported without Swift dependency.

- [ ] Write Rust tests for configuration/AU/session/dimension/PTS validation before FFI, including invalid length and wrong record kind; observe RED.
- [ ] Build Swift static archive only for native Mac target; link platform frameworks/runtime; preserve Linux checks and root Mac cross-check.
- [ ] Native Rust/eframe example embeds the layer in its actual view, feeds the owned captured fixture and asserts decoded/presented/hardware counts. Existing input remains with egui; no separate native window is spawned by the surface.
- [ ] Full checks + one fresh review; fix Important/Critical once, document evidence/remaining host-session/app-UI/installer integration, push. Do not enable a fake Video badge or claim full Coherence.
