# Experimental Coherence video probes

These SDK-only probes evaluate a potential Windows Graphics Capture → Media
Foundation H.264 → VideoToolbox/native macOS surface backend. They do not enable
video in FreeLane. The shipped Coherence viewer still uses snapshots.

## Run

Portable evidence checks (Python 3, no extra packages):

```sh
python3 -m unittest discover -s tests/video -v
python3 scripts/video/report.py WINDOWS_REPORT_JSON MAC_REPORT_JSON
```

For native execution, use the **Experimental native video probes** GitHub Actions
workflow. It builds/runs separately on Windows x64 and Windows 11 ARM, builds the
Swift probe on macOS ARM, and transfers only a generated Windows ARM fixture to
macOS. Each probe process has a 60-second CI timeout. Outputs are diagnostic
artifacts, not application installers.

Manual native builds require a Visual Studio C++20 developer shell with the
Windows SDK, or Swift and the macOS SDK respectively. The exact compiler/linker
commands live in [the workflow](../../.github/workflows/video-probes.yml).
`probe-windows.exe NEW_DIRECTORY` refuses an existing destination;
`probe-macos GENERATED_FIXTURE_MP4` prints a JSON report.

## What is measured

Windows captures only its own painted test window and verifies its center pixel.
Capture uses a free-threaded frame pool; a CPU staging readback is used for
validation. Separately, generated NV12 pixels (320×180, 30 pictures at 30 FPS)
exercise software H.264 encoding. The capture image is **not** yet connected to
the encoder.

The MP4 fixture verifies cross-platform codec interoperability. A separate direct
IMFTransform probe measures actual packet delivery at a paced 30 FPS, recording
codec readbacks, first packet time, packets before final drain, and peak pending
input frames, observed immediately after submission before draining output.
This is not capture-to-display latency. Hardware encoder enumeration does not
establish hardware encoding; the reference encoder is software.

The Mac probe requires exactly 30 nonempty, single-picture H.264 samples with
increasing presentation timestamps, decodes them with VideoToolbox, and checks
320×180 dimensions and generated pixel variation. The MP4 reader can expose one
leading empty edit and a bounded tail of empty end markers (at most 64); actual
reader completion is still required. End markers must have zero bytes, zero
samples, and can occur only after all 30 pictures. Valid marker timestamps must
fall between the final picture and the one-second fixture endpoint. They
contain no coded picture and are counted separately. Coded data is never skipped to meet the
frame bound. The probe queries actual hardware decoder use and submits the
compressed pictures to an owned `AVSampleBufferDisplayLayer` window.

Native layer `rendering` status is not proof of visible presentation. Pixel
validation performs CPU readback, so it cannot prove zero-copy rendering.

## Acceptance remains separate

The assessor distinguishes a candidate backend from completed feasibility and
product acceptance. Gate A additionally requires bounded direct packet output,
codec readbacks, measured copy path and verified onscreen pixels. End-to-end and
performance acceptance stay false in every probe report.

These tests do not establish 720p30/60 performance, physical Mac CPU/GPU usage,
live resizing, reconnect/keyframe recovery, VM transport, audio synchronization,
or independent application-window video. Those require subsequent integration
and sustained measurements described in the
[approved design](../../docs/superpowers/specs/2026-10-18-coherence-video-design.md).
