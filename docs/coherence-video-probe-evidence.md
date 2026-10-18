# Coherence video probe evidence — 2026-10-18

Status: experimental backend evaluation. FreeLane's application viewer still uses
snapshots; these probes do not enable continuous video in the product.

## Scope

Native Windows probes run on Windows x64 and Windows 11 ARM. They capture only a
window they created. Encoding uses generated 320×180 NV12 pictures, not captured
user content. The Mac probe consumes the generated ARM MP4, decodes 30 pictures,
checks generated pixels, queries hardware decoding, and submits compressed
samples to an owned native display layer. Capture → encode → live transport →
presentation is not yet integrated.

## Encoder configuration finding

Run [39104787019](https://github.com/puma2k/freelane/actions/runs/39104787019)
showed that low-latency and zero-B-frame property readbacks alone were
insufficient. At paced 30 FPS, the default software encoder retained four inputs
at cadence boundaries, produced 26 packets before final drain, and delivered the
first ARM packet after approximately 149 ms.

Requesting and reading back one encoder worker reduced that backlog. The
reviewed implementation records peak pending inputs immediately after submission,
before output is consumed. Earlier cadence-boundary reports understated that
peak by one in the observed single-worker run. Neither measurement is a
capture-to-display latency measurement or a proof of 720p performance.

The runner enumerated no hardware H.264 encoder. These results concern the
software-reference path; virtual graphics availability does not establish guest
hardware encoding.

## Native result

Source `dc71c2a` passed [native run 39105862266](https://github.com/puma2k/freelane/actions/runs/39105862266).
The [machine-readable evidence](evidence/coherence-video-native.json) contains the
actual reports and assessor output.

| Probe | Windows ARM | Windows x64 |
| --- | ---: | ---: |
| Owned capture pixel verified | yes | yes |
| Generated MP4 encoded pictures | 30 | 30 |
| Direct packets | 30 | 30 |
| Peak submitted-but-not-output pictures | 2 | 2 |
| Packets before final drain | 29 | 29 |
| First packet at paced 30 FPS | 37.396 ms | 43.425 ms |
| Worker count readback | 1 | 1 |
| Low latency / zero B frames readback | yes / yes | yes / yes |

These are individual CI observations, not a controlled sustained benchmark.

The Apple Silicon probe decoded all 30 ARM-generated H.264 pictures, verified
pixel variation/dimensions, confirmed hardware decoder use and obtained native
layer rendering status. The reader exposed one leading empty container buffer
and three terminal non-picture buffers. They were accounted for separately;
all 30 coded pictures remained required, followed by actual reader completion.
The injected error after asynchronous submission exited as expected without a
crash. This establishes a candidate SDK backend for the next experimental
integration step, **not** completed Gate A or product/performance acceptance.

## Validation and remaining gates

Eight portable tests passed. Existing Rust checks passed 222 tests with nine
ignored; 14 MCP tests and strict clippy passed. The portable checks exercise report validation, architecture separation, codec
readbacks, strict frame counts, queue bounds and Windows macro compatibility.
The native suite also checks refusal to overwrite an existing output directory
and Mac cleanup after an injected error following asynchronous submission.

A native surface reporting `rendering` is not verified onscreen output. Validation
uses CPU readback; zero-copy/copy-path acceptance remains open. Product acceptance
and performance acceptance remain false, including after a green probe workflow.

Next work includes raw access-unit/configuration interoperability, independent
bounded video transport and broker integration, native capture-to-encoder wiring,
Mac viewer integration, and sustained 720p30 measurements on the user's physical
Apple Silicon / Windows ARM setup. Physical Mac testing remains deferred; it is
not the only work remaining.
