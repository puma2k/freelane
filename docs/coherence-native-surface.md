# Embedded native Mac video surface

The opt-in `native-video` desktop feature embeds an `AVSampleBufferDisplayLayer`
child view into eframe's existing AppKit view. VideoToolbox requests a hardware
H.264 decoder and NV12 IOSurface output. The renderer does not read decoded
pixels back to the CPU or upload decoded frames as egui RGB textures.

The Rust owner stays on the UI thread and validates configuration, binding,
dimensions and increasing accepted sequence/timestamps before calling Swift.
At most two compressed frames are submitted concurrently; a busy return leaves
the caller's record unconsumed. The presentation slot retains the latest decoded
image. Closing detaches immediately, prevents further presentation, and drains
asynchronous decoder callbacks off the UI thread before releasing the single
process decoder budget.

The normal app viewer still uses its existing image path. This feature is a
renderer integration step; session control, guest helper distribution and live
viewer activation remain separate work. In particular, Windows capture must use
the same client-area geometry as pointer input before that activation.

## Validation

Portable Rust tests cover record validation and retry admission semantics.
Native CI builds a Swift SDK harness and a Rust/eframe owned-window fixture:

```sh
cargo run -p freelane-desktop --features native-video \
  --example native_video_surface -- captured-packets.flv
```

Both consume the actual 30-picture Windows capture fixture. The SDK harness
also reads decoded pixels for test-only variation checks and repeatedly closes
with asynchronous decode outstanding. The Rust fixture exercises the C ABI,
Swift archive linkage and eframe's real NSView. CI reports decoded/presented
counts and actual hardware decoder readback; successful compilation alone is
not runtime evidence.

These checks do not measure displayed screen pixels, end-to-end latency,
zero-copy behavior, sustained user-workload frame rate, or physical-Mac
sleep/wake. The normal app does not advertise live video on this evidence.
