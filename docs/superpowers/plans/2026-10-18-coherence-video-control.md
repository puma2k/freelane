# Coherence Video Control Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans inline, one final whole-batch review. User explicitly requests continued implementation; preserve existing architecture, execution method and commit/push authorization.

**Goal:** Provide strict negotiated v14 start/stop/keyframe control, guest session ownership and host correlation before connecting the persistent native encoder/viewer.

**Architecture:** Control stays on existing RPC; encoded bytes stay on video. A validated complete WindowTarget and fresh Binding authorize one active stream. A real backend must opt in; existing Windows/Linux launchers remain unsupported until native worker installation/integration. No fake video capability is advertised.

**Tech Stack:** Existing Rust protocol/guest/host crates, serde, getrandom; no new dependency.
**Spec:** `docs/superpowers/specs/2026-10-18-coherence-video-design.md`; existing FLV1 Binding and WindowTarget identity contracts.

## Constraints / Review Focus

- Reserve v14 only for `Action::Video { operation }`; legacy protocols must reject it and v14 rejects other actions.
- Parameters: complete nonzero target handle/pid/created/incarnation; nonzero nonce/stream/generation; even positive dimensions<=1920×1080; nominal fps30 only in this milestone.
- `Start` carries full Parameters; `Stop`/`Keyframe` carry Binding. Replies mirror exact Parameters/Binding and operation; no unrelated success or stale-generation acknowledgement is accepted.
- Only one active Parameters per guest session controller. Same active Start is idempotent; different Start returns busy. Stop wrong binding and keyframe wrong binding never affect the current encoder.
- Backend start failure leaves no active session; stop failure does not falsely report stopped; successful repeated stop of the last stopped binding is idempotent. Reuse of any stopped nonce or a regressed generation for any previously used stream is rejected, including across intervening streams. Retirement history never evicts entries; at 65,536 retired bindings new starts fail closed until the guest agent is restarted (2 MiB binding storage). Caller must generate fresh nonce for every new session.
- Start authorization is rechecked even for idempotent Start; target replacement must not silently reuse an encoder. If recheck fails, stop only the owned encoder and fail closed.
- Control socket requests are transient; normal RPC reconnect/disconnect does not stop an independently active stream. Explicit stop, backend lifecycle and owning controller Drop handle cleanup. Never launch/kill the guest application.
- Keep existing file transfer, clipboard, catalog and snapshot semantics; no capability unless backend registers support.

### Task1: Strict v14 wire contract

Files: protocol `video_control.rs`, `video/mod.rs` serde Binding, `lib.rs` variants/version validation; tests `video_control.rs`.
Types: `Parameters{target,binding,width,height,fps}`, tagged `Operation::{Start{parameters},Stop{binding},Keyframe{binding}}`, tagged `Reply::{Started{parameters},Stopped{binding},KeyframeRequested{binding}}`; `Operation::validate`, `Reply::validate_for(&Operation)`.

- [x] Test JSON roundtrip, unknown fields, v1-v13 rejection, nonvideo-v14 rejection, target/session/dimension/fps bounds, exact reply correlation; observe RED.
- [x] Implement contract, preserve all legacy protocol suites, commit.

### Task2: Guest control dispatch/session ownership

Files: guest `video.rs`, trait/dispatch and TransferLauncher forwarding in `lib.rs`, tests.
Interface `Encoder` trait (`available`, `authorize`, `start`, `stop`, `keyframe`); `Session<E>::action(&Operation)->io::Result<Reply>` and Drop. Trait Launcher video support/action defaults unsupported. Native backend supplies real authorization/start later.

- [x] Test start success/failure, idempotency without second encoder, busy competing target, wrong-session stop/keyframe, stale authorization cleanup, stop-failure consistency, explicit stop preserving unrelated owned worker and Drop cleanup. Observe RED before implementation.
- [x] Add dispatch/capability list conditionally, forward through TransferLauncher. No production backend opts in yet. Test real serialized serve request routing and old-launcher unsupported response; ordinary control reconnection preserves session state.
- [x] Run guest tests, commit.

### Task3: Host requests and lifecycle handshake

Files: root `src/agent.rs`, `src/video/control.rs`, root video exports, owned Unix endpoint tests.
Interfaces: `agent::video_request` routes v14 through normal lease/correlation; `agent::request_socket_version` validates exact video replies. `video::fresh_binding(stream,generation)` uses getrandom and validate. `video::require_capability` verifies explicit v14feature and added video socket without restarting a VM. No automatic retry or command replay.

- [x] Test exact correlated replies, malformed/mismatched success rejection, negotiated-unsupported refusal before mutation, fresh nonce generation and runtime port missing. Observe RED.
- [x] Implement bounded control integration; leave native worker/viewer attachment to next plan and capabilities disabled until real registration.
- [x] Full workspace, fmt/clippy, MCP/probes and one fresh review; fix Important/Critical once with regressions, record evidence and push. No live-video/performance/release claim.
