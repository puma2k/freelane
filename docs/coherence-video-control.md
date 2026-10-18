# Negotiated video control

The control foundation supports strict v14 start, stop and keyframe requests.
This milestone does **not** enable live video in the application: production
launchers still omit `window_video` until a real native encoder is integrated.

Start requires a complete window identity, fresh random binding, even dimensions
up to 1920×1080 and nominal 30 FPS. Host requests negotiate capability and verify
the existing video socket before mutation. Replies must match the exact operation,
target, session and parameters. There is no automatic retry or VM restart.

The guest owns one encoder independently of transient RPC connections. Competing
starts and wrong-session commands cannot affect it. Target identity and worker
liveness are checked before acknowledgement, including after startup. Failed
cleanup quarantines that owner until explicit stop succeeds. Cleanup never kills
the captured application.

Retirement remembers all used bindings across intervening streams. Old nonces
and generation regressions are rejected. History never evicts: after 65,536 retired
sessions (2 MiB of binding storage), fresh starts fail closed and require an
explicit guest-agent restart. Stop, keyframe and current-session idempotent start
continue to work. Native integration must preserve this controller for the agent
lifetime and invalidate window incarnations when the agent restarts.

Validation on Linux, 2026-10-18:

- `cargo test --locked --workspace`: 260 passed, 0 failed, 9 ignored.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all --check`: passed.
- Existing MCP suite: 14 passed; portable video probe suite: 10 passed.
- Alternating-stream replay regression failed before the fix and passes after it.

Owned child processes exercise lifecycle and cleanup; owned Unix sockets exercise
serialized negotiation and acknowledgement. These checks do not measure native
capture throughput, Mac rendering performance or physical Mac behavior.

Next: persistent Windows capture/encoder worker and native Mac viewer attachment,
followed by end-to-end lifecycle and performance checks. Existing native packet
interoperability evidence is in [coherence-direct-packets.md](coherence-direct-packets.md).
