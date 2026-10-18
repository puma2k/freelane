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

Local validation: 12 portable video/diagnostic tests pass; 261 Rust tests pass with 9 ignored. The accompanying Linux Rust
workspace, clippy and formatting checks pass. The separate native Mac socket-EOF
regression failed while setting `SO_RCVTIMEO` after disconnect, before checking
EOF. The test now uses bounded `poll` with nonblocking read, preserving its
requirement to observe EOF while an unrelated child is running. No broker
shutdown behavior was changed to mask inheritance. The kernel close-on-exec
flag also has a direct assertion.

At source `dcef55e`, [run 39142683488](https://github.com/puma2k/freelane/actions/runs/39142683488)
completed both portable macOS and Linux jobs successfully (including fmt, clippy,
workspace tests and build). This distinguishes the previously failing Mac test
from the independently successful native capture/decoder probes.
