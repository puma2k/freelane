# Direct packet interoperability evidence

Native run [39112796846](https://github.com/puma2k/freelane/actions/runs/39112796846),
source `6eb5c63`, passed the direct Windows ARM encoder → Rust protocol validator →
Mac hardware decoder/surface test. See the
[actual reports](evidence/coherence-direct-packets.json).

The software Media Foundation encoder emitted 30 generated320×180 pictures as
Annex B. The bounded adapter extracted one SPS/PPS configuration, converted slices
to four-byte-length-prefixed AVCC and wrote31FLV1records (one configuration and
30pictures). Native x64 and ARM runs each emitted one IDR and30pictures.

The real Rust reader validated the actual ARM file, header limits, generation,
configuration, NAL framing, IDR flags and continuous sequence. The Mac loader
constructed CoreMedia format/sample buffers from those same bytes, without MP4
demuxing. VideoToolbox decoded30pictures with correct dimensions and changing
generated pixels. Actual hardware decoding/readback and native surface rendering
were true. A separate direct-path assessor enforces these properties independently
of the retained MP4 reference test.

Ten portable video tests passed. The unchanged workspace checks passed237tests
with9ignored; strict clippy/fmt passed. One fresh review found direct hardware
acceptance was not enforced; a new assessor regression went RED→GREEN and the
native workflow then passed its independent direct hardware gate.

Two minor test-harness improvements remain deferred: preserving the converter's
bounded static error message instead of only E_FAIL, and injecting an error with
raw samples separately from the existing shared-decoder MP4 failure test.

This establishes direct codec-byte/framing interoperability. The fixture is still
a finite file with generated pixels. It is not live capture-to-VM-to-viewer video,
720p30/60 acceptance, zero-copy proof or measured input-to-visible latency. The
next source integration needs negotiated start/stop/keyframe control, a persistent
native capture/encoder worker, and the native Mac viewer frontend over the owned
video transport. Full performance and physical Mac acceptance remain open.
