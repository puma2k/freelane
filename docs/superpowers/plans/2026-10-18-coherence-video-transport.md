# Coherence Video Transport Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans inline, with one final whole-batch review. Existing user authorization is to continue the approved roadmap, commit and push; do not request it again.

**Goal:** Implement testable binary video framing, bounded session recovery and an exclusive cancellable VM video reader, plus automatic QEMU port provisioning.

**Architecture:** The chosen experimental candidate is Windows Graphics Capture / single-worker software Media Foundation baseline H.264, with native Mac hardware decoding. Native run39105862266 supports that choice, while copy-path and full performance gates remain open. Video receives its own QEMU port; no snapshot/input RPC or application launch is transported through video.

**Tech Stack:** Existing Rust workspace and std/libc/fs2; no new runtime dependency.

**Spec:** `docs/superpowers/specs/2026-10-18-coherence-video-design.md` (approved); native evidence `docs/coherence-video-probe-evidence.md`.

## Global Constraints

- This implements Gate B foundations, not native live capture/encoder/viewer integration, v14 start/stop negotiation or the cross-process viewer IPC frontend. Do not advertise a video capability yet.
- One reader lease per VM, separate from the agent-client lease. Broker shutdown never closes a guest app or stops/restarts a VM.
- 1920×1080 maximum; even positive dimensions. H.264 baseline only. Codec config64KiB; access unit2MiB. Header lengths checked before allocation.
- Session nonce16 bytes, not all zero, nonzero stream/generation, strict sequence/timestamp checks. Stream data cannot change the authorized target/binding.
- At most2 queued access units. Overflow or sequence gap clears dependencies, requests a keyframe and accepts only a verified IDR before resuming. Decode/presentation integration remains future work.
- Idle reader sleeps in poll; partial record gets one absolute2second budget. Shutdown wakes poll without waiting for source traffic.
- Port provisioning affects new VM starts only; no running VM is force-restarted. Existing control socket is unchanged.

## Exact Experimental Wire Format

64-byte header, unsigned integers big endian:
`0..4 FLV1`, `4 kind (1=config,2=AU)`, `5 flags (bit0=IDR, otherwise0)`,
`6 profile=66`, `7 reserved=0`, `8..12 payload bytes`, `12..14 width`,
`14..16 height`, `16..32 nonce`, `32..40 stream`, `40..48 generation`,
`48..56 sequence`, `56..64 presentation microseconds`.
Configuration has flags/sequence/time0, and AVCDecoderConfigurationRecord (avcC),
4-byte NAL lengths, one SPS and one PPS, profile66 matching SPS. Access units use
4-byte NAL lengths, at most256 NALs; allowed types1/5/6/9/12. At least one VCL NAL;
IDR flag must agree with type5 and no mixed type1/5. This validates framing, not
full H.264 syntax or decoded dimensions; the native decoder must validate those.

## Review Focus

- A claimed keyframe containing only dependent slices must not resume recovery.
- Wrong binding/config changes must not deliver stale frames to another target.
- Slow trickle must not reset a record deadline; idle shutdown must wake promptly.
- Another broker, symlinked lease, or stopped VM must not connect/steal video.
- Adding the video port must preserve one virtio controller and existing control operations.

### Task1: Binary framing and dependency-aware receiver

Files: `crates/agent-protocol/src/video/{mod.rs,h264.rs,receiver.rs}`, integration tests in `crates/agent-protocol/tests/video.rs`; export module in lib.rs.
Interfaces: `Binding { nonce:[u8;16], stream:u64, generation:u64 }`, `Record { binding, kind, width, height, sequence, pts_us, payload }`, `Kind::{Configuration,AccessUnit{idr:bool}}`; `read_record(&mut impl Read)->io::Result<Option<Record>>`, `write_record(&mut impl Write,&Record)`.
`Receiver::new(Binding)`, `accept(Record)`, `pop()->Option<Record>`, `configuration()->Option<&Record>`, `take_keyframe_request()->bool`, `clear()`; no target adoption from bytes.

- [x] Write roundtrip/short-read/short-write tests; malformed/truncated/unsupported/oversize headers fail, lengths rejected before payload reads. Run missing-module/API RED.
- [x] Implement framing and bounded avcC/AVCC validation; test fabricated IDR flag and malformed NAL/config lengths.
- [x] Write recovery tests then implement receiver: first config+IDR, queue overflow clears2dependents, ignores P frames until IDR; sequence gap similarly recovers, timestamp regression/replayed sequence/wrong nonce/generation/config replacement fail without delivering a frame; clear prevents old session reuse.
- [x] Run protocol suite, commit focused implementation.

### Task2: Owned exclusive VM reader

Files: `src/video/{mod.rs,reader.rs}`, root lib export, `tests/video_broker.rs`.
Interface: `VideoBroker::connect(config:&Path,target:WindowTarget,binding:Binding)->Result<Self>` requires a previously negotiated/authorized target; validates complete identity. It verifies private owner runtime and a held supervisor lock, leases `video-client.lock`, connects only `video.sock`, starts one read worker. `pop`, `configuration`, `take_keyframe_request`, `ended`, `target`. Drop cancels and joins, no reconnect/replay or source/socket unlink.

- [x] Write owned Unix endpoints tests for exclusive reader, retained target, valid output, malformed/disconnected source clears pending frames, stopped VM and unsafe/symlinked locks rejected; observe RED.
- [x] Implement private regular-file lease checks and VM-running lock check; separate lock from control. Worker uses cancellable poll and absolute2second partial-record deadline; idle no periodic wakeup.
- [x] Test cancellation when idle and midway through header/body, slow trickle deadline, and a saturated video source alongside a real owned agent request; control must complete independently. No request/keyframe command is automatically sent before negotiated v14 integration exists.
- [x] Run focused suite and commit.

### Task3: QEMU port lifecycle and evidence

Files: `src/qemu.rs`, `tests/qemu_plan.rs`, `tests/qemu_smoke.rs`, transport docs.

- [x] Test agent-enabled new starts provision `video.sock` / `org.freelane.video.0` separately, retain existing agent port, one virtio-serial controller and no TCP listener; agent-disabled starts omit both. Observe RED then implement.
- [x] Add video socket removal only inside existing owned supervisor cleanup lock. Real QEMU smoke queries separate chardevs and verifies cleanup; run with installed QEMU/firmware.
- [x] Run workspace fmt, strict clippy, workspace tests, MCP and portable video probes. One fresh whole-batch review, fix Important/Critical once with regressions; no re-review.
- [x] Record actual gates and limits, commit/push using existing authorization. No product release or native-video UI claim from these foundations.

## Outcome

Completed these Gate B foundations at `3520e42`; [evidence](../../evidence/coherence-video-transport.json). Workspace237passed/9ignored, native ARM QEMU/UEFI1passed underumask0022, MCP14 and portablevideo8passed; strict clippy/fmt green. One fresh review found legacy0644supervisor-lock incompatibility; regression RED→GREEN and new private lock creation fixed it. No deferred findings. Full Gate B and continuous-video product/performance acceptance remain open as scoped above.
