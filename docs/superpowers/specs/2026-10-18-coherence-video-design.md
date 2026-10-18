# Coherence: continuous video design proposal

Status: proposed architecture for review, 2026-10-18. Baseline: preview.23 / `37d7cd6`. No continuous-video implementation or performance acceptance is claimed by this document.

## Product intent

Prioritize the Windows application workspace on Apple Silicon Macs: continuous responsive video, readable text and low resource use. Keep separate host windows, targeted input, guest-app preservation when a viewer closes, and English/Vietnamese UI. Other roadmap feature development is deferred while this video milestone is the priority.

Smoothness means delivered motion and input-to-visible latency, not merely a capture timer. Low resource use includes host and guest CPU, memory, decoder copies and idle work. The current viewer remains the compatibility fallback while a separate pipeline is proven.

## Findings from the repository

- `crates/guest-agent/src/windows.rs::capture_inner` uses PrintWindow and a full-window CPU bitmap, then resizes and encodes JPEG. Output is reduced until it fits28000 bytes, then hex-encoded.
- `src/qemu.rs` provides one virtio-serial agent port, `org.freelane.agent.0`. Control requests and snapshots currently share it.
- `crates/desktop/src/app_ui.rs` schedules snapshots and input through one worker. Preview.22 bounded input starvation; preview.23 avoids repeated host JPEG decode/texture creation for identical responses.
- Those optimizations do not remove guest capture/encoding work or provide continuous video. The current host renderer uses eframe/glow; a native video surface will require a new integration boundary.

## Options and recommendation

| Option | Benefit | Main limitation / decision |
| --- | --- | --- |
| Dedicated per-window H.264 stream | Preserves the existing app/window identity model; independent input; native Mac decode/display opportunity | Requires Windows ARM capture/encoder and native Mac surface probes. Recommended, conditional on those probes |
| SPICE display transport | Existing QEMU ecosystem and full-desktop transport | Per-app window integration and Windows ARM graphics/driver support need separate proof; keep as an alternative |
| RDP/RemoteApp | Mature remote application/desktop ecosystem | Windows edition, session behavior, client integration and licensing need evaluation; cannot assume available on every Windows installation |

Use the dedicated stream as the first candidate. If a native capture/encoder probe fails, record the failure and evaluate the alternatives before adding a software dependency or promising a frame rate. No codec library is selected for bundling yet.

## Pipeline

1. Capture the selected, verified Windows window using an event-driven native capture API. Windows Graphics Capture / free-threaded frame pool is the first candidate. Probe support and failure behavior; preserve protected-content, secure-desktop and unavailable/minimized-window restrictions.
2. Reuse a bounded pool of capture/conversion buffers. Prefer D3D-backed conversion to an encoder-supported format when available; measure software conversion separately.
3. Probe Media Foundation H.264 encoder transforms on the actual Windows ARM guest. Prefer a working low-latency hardware path if one exists; otherwise evaluate software encoding within an explicit budget. QEMU/HVF virtual graphics does not establish guest hardware encoding availability.
4. Use a separate binary virtio-serial port, provisionally `org.freelane.video.0`, for video. Keep launch/input/clipboard/file operations on the existing control channel.
5. On Mac, first evaluate a native compressed-video display surface using AVSampleBufferDisplayLayer / VideoToolbox. Determine actual hardware decoding and buffer-copy behavior through a native probe. Preserve the existing egui toolbar/input mapping around a video-surface abstraction. Do not assume that decoding to RGB and uploading through the current image path is a zero-copy solution.

Linux retains a disposable protocol/transport test peer. It cannot establish Apple video rendering or Windows ARM codec performance.

## Separation and ownership

A VM-owned video broker is the sole host connection to the video chardev. Native viewer processes subscribe through private local IPC; no new public TCP listener or firewall exposure is required. The broker must be tied to the existing VM supervisor/runtime lock, survive viewer churn and clean up only its owned resources.

The first implementation supports one active video stream per VM. A second viewer requesting a different target gets an explicit busy/fallback choice; it must not steal the first stream or start another encoder silently. Multiple streams and shared decoding need a measured aggregate budget before being enabled.

A video session binds an unpredictable nonce, stream ID, fresh target identity and generation. Start/stop/configuration are separately capability-negotiated on the control channel. The next unused control extension is v14 at this baseline; confirm it is unused before implementing. Old agents retain their existing behavior.

Validate the selected target before capture and on lifecycle changes. A replacement process/window, resize/reconfiguration, reconnect or new codec configuration changes generation. Host frames/results may only reach the matching viewer target/generation. Stream data cannot authorize a new capture target or launch an app.

## Framing and backpressure

Use an independently versioned, length-prefixed binary protocol. The first implementation plan must fix its exact byte layout before code. The design bounds are:

- Maximum coded size1920×1080; initial motion trial1280×720 at30FPS. Request60FPS only after the pipeline meets resource/latency budgets on the declared machine.
- Maximum encoded access unit2MiB; maximum codec configuration64KiB. Reject lengths before allocation. Reject unsupported codec/profile, invalid dimensions, timestamp/sequence regression and wrong session/generation.
- H.264 low-latency configuration: no B frames and no intentional frame reordering/lookahead; validate that the selected transform honors the requested settings. Carry codec configuration and random-access/keyframe information explicitly.
- At most two encoded access units waiting for processing and one latest decoded frame waiting for presentation. Decoder/capture pools must have explicit finite limits, and counts of retained/in-flight buffers must be measured.
- Do not drop arbitrary H.264 reference frames and continue decoding dependent frames. On encoded overflow, discard the affected dependency chain, request a fresh keyframe and resume from a valid random-access point. Drop superseded decoded presentation frames freely.
- A slow viewer must not block input, another control operation or the VM supervisor. Bound stream write waits, cancel stalled work and release resources on disconnect.
- Recovery may restart a video session with new identity/configuration. It must never replay uncertain input or launch operations.

Changing video settings is not a guest-app relaunch. Closing a viewer stops its stream without closing the guest app or VM.

## Idle, adaptation and UI

Start with a30FPS motion target, not a busy polling loop. Use capture notifications and observed frame changes where supported. Reduce encoding/presentation work for unchanged or hidden/inactive viewers and resume promptly on actual changes/input. A capture notification alone is not proof that pixels changed; measure the selected API's behavior before claiming idle savings.

Adapt cadence/bitrate/resolution from measured queue pressure and encoder/decoder throughput. Never build an unbounded latency queue to preserve nominal FPS. Keep text readability in the acceptance suite, including Vietnamese diacritics, menus and scaling.

The viewer shows a compact Video / Snapshot state with actual delivery statistics available on demand. Avoid explanatory paragraphs in the normal UI. Unsupported video, a busy stream or required VM restart has an actionable explicit choice; never present snapshots as accepted continuous video.

Running VMs without the added port are not force-stopped. Explain that video becomes available after a user-controlled restart; preserve the existing app-viewer path meanwhile. New QEMU command generation and agent installation should supply the new port/runtime support without manual path entry.

## Delivery gates

### A — Native feasibility probes

Build disposable probes for Windows ARM capture, conversion and Media Foundation encoder discovery/configuration; record actual supported formats and low-latency behavior. Build a Mac native decode/display probe using known generated video and record hardware-decoder selection, copies and surface integration. Build/cross-check success alone does not pass runtime acceptance.

Capture/encoder failures must return bounded diagnostics without pixels, clipboard or user document contents. The probe artifacts are explicitly experimental, not enabled product features.

Gate: choose the capture/encoder/display combination from results. If unavailable, issue a documented go/no-go and evaluate the alternatives. No new bundled codec/driver or permanent product protocol is implemented before this choice.

### B — Bounded transport and broker

Implement the selected binary framing, session handshake, independent video port, broker lifecycle and limits. Test malformed/truncated/oversized frames, stalled peers, generation changes, partial writes, disconnects and input while video is saturated using owned endpoints and actual QEMU port fixtures.

Gate: bounded queues/memory, no frame routing to another target, no duplicate broker/encoder, input remains serviceable and owned cleanup preserves VMs/apps/files.

### C — Single-window end-to-end video

Connect the selected native capture/encode and Mac decode/presentation paths. Test resize, target destruction/replacement, hidden/minimized/protected windows, frame/configuration changes and viewer disconnects. Exercise keyboard, pointer and dialogs alongside video; preserve existing foreground/identity restrictions.

Gate: first sustained continuous-video result at1280×720/30FPS on a declared Apple Silicon + installed Windows ARM setup. Target p95 input-to-visible latency below100ms for ordinary desktop interaction; treat this as an engineering target until measured. Record actual delivered FPS, frame age, drops/keyframe recovery, bitrate and both-side CPU/memory. Do not label a failed target as accepted.

### D — Smoothness and efficiency acceptance

Run moving-content and text/scroll/resize workloads for at least10minutes, then static/idle and reconnect tests. Establish a repeatable frame/input marker and clocks; report capture/encode/transport/decode/presentation timing separately. Report both sustained frame rate and stalls.

Compare profiles on the same hardware/guest/workload:30FPS versus60FPS where supported, and hardware/software codec paths. Select defaults from results. Document resolution/scaling, Mac model/macOS/RAM, guest build/vCPUs/RAM and backend/driver versions. A CPU percentage from a debug Linux fixture does not satisfy this gate.

Gate: accepted video/latency targets without rising queues, runaway CPU/memory or lifecycle/input regressions. Full Coherence also requires subsequent dialog/delegation, native window/Dock and multi-viewer acceptance; one video stream is not the entire feature.

## External evidence still required

Authoritative API references to verify during the probes:

- Microsoft: `IGraphicsCaptureItemInterop::CreateForWindow`, Windows Graphics Capture frame pool and capture-session lifetime.
- Microsoft: H.264 Media Foundation encoder, transform enumeration and low-latency codec properties.
- Apple: AVSampleBufferDisplayLayer, VideoToolbox decompression and native surface/buffer integration.

Attempts to read Microsoft/Apple documentation from this cloud returned tunnel403. The subsequent escalated read was aborted before execution. API availability, exact version floors and zero-copy claims are therefore not recorded as verified findings here. Existing source inspection is the evidence for the repository findings above.

Physical-Mac testing remains deferred by the owner. Portable transport work can continue independently after design approval, while native runtime/performance gates stay open. Required source tests/review, native packaging and verified public release payloads remain separate completion conditions.
