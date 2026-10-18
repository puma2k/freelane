# Direct H.264 Packet Interoperability Plan

> **For agentic workers:** Use superpowers:executing-plans inline and one final whole-batch review. Continue existing user-approved video architecture and execution method.

**Goal:** Windows ARM direct encoder packets, rather than MP4 reader output, must decode on Mac through the new FLV1 framing.

**Architecture:** Experimental generated fixture only. Convert the Media Foundation Annex B output to bounded avcC configuration + AVCC access units, serialize FLV1, check it with the real Rust protocol reader, and construct native Mac compressed samples from those same records. Preserve the MP4 test as an independent reference.

**Tech Stack:** Existing Windows C++ SDK probe, portable C++ packet converter/tests, Rust protocol example, native Swift CoreMedia/VideoToolbox; no new dependency or product capability.

**Spec:** `docs/superpowers/specs/2026-10-18-coherence-video-design.md`; wire layout from `2026-10-18-coherence-video-transport.md`.

## Constraints / Review Focus

- Owned generated320×180/30pictures only; no user window data. Refuse existing destination as before.
- Raw fixture at most4MiB, config64KiB, AU2MiB, at most256NALs; FLV1 binding fixed only for this owned test, never production authorization.
- Annex B start codes/emulation prevention, empty/truncated NALs, forbidden bits, missing/changed SPS/PPS, claimed IDR without IDR, and repeated configuration must not silently corrupt references.
- Rust must validate the actual Windows file before native Mac decoding, proving cross-language header/NAL compatibility.
- Mac must decode exactly30real pictures, verify hardware decode/dimensions/pixel variation and exercise error cleanup; file replay is not live transport/performance acceptance.
- Keep previous broker/product tests valid. Native copy-path/onscreen acceptance stays false.

### Task1: Packet conversion and Windows fixture

Files: `scripts/video/annexb.hpp`, portable C++ regression in `tests/video/test_annexb.py`, `direct-encoder.hpp`, `probe-windows.cpp`.
Interface: portable `PacketConverter::convert(vector<uint8_t>) -> Converted { configuration, access_unit, idr }` (configuration emitted once; same parameter sets allowed again, changed sets fail); direct output writer serializes the approved64byteFLV1header and config/AUs, counts30pictures and verifies file<=4MiB.

- [x] Write portable tests for3/4byte start codes, SPS/PPS→avcC, stripping parameter sets from AVCC picture, IDR/dependent slices, missing/changed params, malformed/oversize/empty NALs; observe missing-header RED.
- [x] Implement converter/writer, wire actual IMFTransform output bytes and actual PTS to fixture. Report raw packet/config/IDR counts without claiming live delivery.
- [x] Native Windows ARM/x64 jobs must emit exactly30rawpictures/config1 and fixture; capture actual packet conversion diagnostics if unsupported.

### Task2: Rust/native Mac direct packet decode

Files: `crates/agent-protocol/examples/video_check.rs`, `scripts/video/packet-input.swift`, `probe-macos.swift`, workflow.
Interfaces: Rust example validates exact finite fixture via read_record+Receiver and pops each picture immediately, outputs31records/30frames. Swift raw loader accepts the same file, bounded4MiB/31records, creates CMVideoFormatDescription from SPS/PPS and CMSampleBuffers from AVCC bytes at declared timestamps; no MP4 demux.

- [x] Add native harness requiring Rust fixture validation and Mac raw decode report before implementations (expected missing example/source RED).
- [x] Implement Rust validator and native sample construction, keep MP4 path independent; compile/run native SDK, require30pictures+pixel checks and separate source label.
- [x] Run portable probes/protocol/workspace checks as affected, one fresh whole-batch review; Important/Critical regression fix once, no re-review. Record actual native results, commit/push. No DMG release or live-video product claim.

## Outcome

Native39112796846/source6eb5c63 GREEN: actual Windows ARM/x64 raw30/config1/IDR1;
Rust validated actual ARM file; Mac raw30picture decode/actualHWreadback/rendering
true. MP4 reference also passes. Portable10tests, workspace237passed/9ignored,
strict clippy/fmt green. Fresh review Important directhardware gating fixed
RED→GREEN; two minor test-harness improvements deferred, recorded in
[the evidence](../../coherence-direct-packets.md). No product live-video or
performance acceptance is claimed.
