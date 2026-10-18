# Owned captured-window packet validation

Native run [39141763259](https://github.com/puma2k/freelane/actions/runs/39141763259)
passed for source `9ff4a63` on 2026-10-18.

Windows x64 and ARM each captured 30 changing frames from the probe's own
repainted window using WGC, read the D3D11 texture and converted BGRA to NV12.
These actual captured pixels went into the direct synchronous H.264 MFT and
produced 30 coded pictures plus one configuration in `captured-packets.flv`.
Each captured input's center shade was verified before encoding; capture failure
has no generated-pixel fallback.

The Rust FLV1 reader/receiver validated the ARM artifact. Native Mac ARM then
decoded all 30 pictures, verified dimensions and pixel variation, read back
hardware decoder use, and submitted the compressed samples to its owned native
surface. The original generated fixtures remain independently tested. Injected
failure after asynchronous submission now runs for MP4 and both raw fixtures.

Evidence: [coherence-captured-packets.json](evidence/coherence-captured-packets.json).

Limits: this is a finite 320×180 CPU-readback correctness fixture. Mac checks
variation, not exact four-shade ordering of every decoded frame. It does not
establish zero-copy, live transport, 720p30 performance, onscreen pixel identity
or physical Mac sleep/wake behavior. Production launchers still do not advertise
video; persistent worker installation and app viewer attachment remain pending.

Local validation: 11 portable video tests pass. The accompanying Linux Rust
workspace, clippy and formatting checks pass. A separate native Mac socket-EOF
regression is under investigation; native packet probe success does not imply
the complete repository's Mac test suite passed.
