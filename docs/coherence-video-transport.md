# Coherence video transport foundations

This is the Gate B infrastructure following the
[native backend evaluation](coherence-video-probe-evidence.md).
It does not enable video in the application viewer or advertise an agent video
capability. Native live capture/encoding, v14 control negotiation and native viewer
integration remain separate work.

New VM starts with the guest-agent channel enabled now provision two named ports
on one virtio-serial controller:

| Purpose | Guest port | Private host socket |
| --- | --- | --- |
| Existing RPC/input/clipboard | `org.freelane.agent.0` | `agent.sock` |
| Binary video | `org.freelane.video.0` | `video.sock` |

A running VM is not restarted to add a port. Old running VMs keep their existing
snapshot behavior. Neither channel opens a TCP listener.

## Reader and session behavior

The VM video reader leases `video-client.lock`, independently of the existing
agent-client lock. It requires a private owner runtime, a held VM supervisor lock,
a complete window identity and a caller-authorized session binding. It does not
start capture or authorize a target from incoming data. A second reader receives
a busy error; it cannot steal the active connection.

Idle reads sleep in a cancellable `poll`. Once the first byte of a record arrives,
one absolute two-second deadline covers its header and body; trickling bytes
does not renew it. Shutdown wakes the reader immediately, joins its worker and
releases only its own descriptors/lease. Children do not inherit its source
connection. EOF, invalid data and deadline expiry clear queued frames/configuration.
The reader never stops a VM, closes an app or replays an input operation.

The experimental `FLV1` framing is documented byte-for-byte in the
[implementation plan](superpowers/plans/2026-10-18-coherence-video-transport.md).
Lengths are rejected before payload allocation. Configuration is at most64KiB,
an access unit at most2MiB, dimensions at most1920×1080. Payloads use H.264
baseline avcC configuration and four-byte-length-prefixed NALs. Structural NAL
validation verifies that an IDR flag actually accompanies IDR slices; full
bitstream syntax and decoded dimensions still require native decoder validation.

A receiver binds one nonce, stream and generation. It requires configuration
before frames, monotonic sequence/time and matching dimensions. Reconfiguration
requires a fresh authorized generation. At most two access units wait for a
consumer. Overflow or sequence gaps discard the dependency chain and coalesce a
keyframe request; dependent pictures are ignored until an IDR arrives. The native
controller must route this request over the independent control channel once
that extension is implemented. No automatic command is sent by this foundation.

## Verification

- Protocol tests: short reads/writes, clean EOF vs truncation, bounds before payload
  reads, unsupported header fields, malformed NALs/configuration, false IDR flags,
  identity mismatch, replay, timestamp regression and dependency recovery.
- Owned Unix endpoints: exclusive readers, preserved target, EOF/error clearing,
  idle/partial-read cancellation, absolute trickle timeout, child descriptor
  isolation, and a real RPC round trip during continuous video input.
- QEMU argument tests and real ARM QEMU/UEFI smoke: both chardevs exist, a video
  reader connects, QMP remains usable after closing it, owned cleanup removes
  sockets, and the same VM state can restart.

These establish portable transport behavior and QEMU provisioning. They do not
establish the complete broker/frontend handshake, cross-process viewer IPC,
Windows-to-Mac live pictures, sustained720p30/60, zero-copy rendering or measured
input-to-visible latency.
