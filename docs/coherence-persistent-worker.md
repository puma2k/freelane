# Persistent Windows video worker

The Windows SDK worker now runs independently of transient guest RPC connections.
This milestone adds the encoder process and agent adapter; the desktop app still
uses snapshots until the native Mac viewer and packaging are integrated.

The helper receives a strict 80-byte `FLW1` setup over private stdin. It verifies
HWND, PID, process creation time and the agent's window incarnation property,
initializes WGC and the encoder, then reports `R`. The agent verifies the target
again before sending `G`; `S` is returned only after the first full coded picture
has been written. Normal control reconnects retain the same guest Session.

Video uses only `\\.\Global\org.freelane.video.0`. Control status never carries
pixels or user text. One keyframe request can be pending at a time. Stop and Drop
close/terminate only the worker process, preserving the captured application.

Resource and failure limits:

- Two WGC pool buffers, one latest captured frame, reused staging texture and
  one cached NV12 frame. This reference path uses CPU readback/conversion.
- Baseline software H.264, nominal maximum 30 FPS, verified low-latency settings,
  zero B frames and one encoder worker. At most two submitted pictures pending.
- A 500ms overlapped-write deadline and bounded cancellation. Uncompleted IO is
  never freed/reused: the helper terminates if cancellation cannot complete.
- Two pending inputs cannot persist for 500ms. Three seconds without encoded
  output also terminates the worker, including when a native SDK call is stuck.
  An independent watchdog adds one thread waking at most ten times per second.
- Unchanged input is reused for at most one idle heartbeat per second after
  initial codec priming. Actual idle resource savings have not been measured.
- Capture currently accepts source surfaces up to 1920×1080 and fits even coded
  dimensions inside the request without upscaling. Larger sources fail explicitly.
  Resize, minimize/hide, destruction or identity replacement ends this generation.

The agent looks for the regular sibling file `freelane-video-helper.exe`. Without
it, Windows continues omitting `window_video`. Release setup does not install this
new executable yet. Presence is not an end-to-end driver or viewer acceptance test.

Native [run 39190796705](https://github.com/puma2k/freelane/actions/runs/39190796705)
at source `aa18d08` passed on Windows x64 and Windows ARM:

- 90 captured pictures per worker, two IDRs including a forced keyframe, at most
  two pending pictures; owned fixture coded dimensions 306×172.
- Incarnation replacement, resize and destruction rejected after a successful
  mutation during streaming, with the matching reason within two seconds.
  Initialization failures and generic timeouts cannot count as passing cases.
- Actual overlapped named-pipe stall cancellation and process termination after
  an injected encoder-progress stall passed.
- The Rust FLV1 receiver validated all 90 ARM worker pictures. Existing generated
  and captured 30-picture fixtures still passed native Mac hardware decoding.

Evidence: [coherence-persistent-worker.json](evidence/coherence-persistent-worker.json).

Local checks after guest exit-race fixes: 267 Rust tests passed, 9 ignored; strict
clippy/format checks passed; 15 portable video/diagnostic tests and 14 MCP tests
passed. Full repository [CI39191978602](https://github.com/puma2k/freelane/actions/runs/39191978602)
passed at source `c726a4b`, including Windows guest runtime/process tests,
Windows ARM cross-check/build, portable macOS/Linux, and existing ARM VM checks.

Remaining: native Mac app viewer attachment, automatic helper installation and
release packaging, real Windows virtio-driver/agent/worker end-to-end validation,
720p30 sustained motion and physical Mac latency/resource/sleep-wake acceptance.
The 90-picture fixture is correctness evidence, not a sustained performance result.

Deferred test polish: the portable duplicate-configuration assertion currently
uses an already-poisoned writer, so it does not isolate that particular branch.
