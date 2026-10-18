# Remaining product work after the preview.23 source milestone

This is an implementation/acceptance inventory, not a declaration that the product roadmap is finished. The user deferred physical-Mac testing; that deferral does not convert untested behavior into acceptance.

| Area | Remaining source or design work | Acceptance dependency |
| --- | --- | --- |
| P0/P1 | Finish native setup/error recovery based on actual failures | Apple Silicon HVF Windows/Ubuntu install, reboot, Finder/Dock, sleep/wake, normal-user Windows agent |
| P2 | Windows live two-way sharing or conflict-aware file synchronization, live-drive invalidation/reconnect beyond startup-unavailable roots | Native guest accounts, Windows ARM sharing backend/driver and physical external storage |
| P3 | Dedicated video transport/encoding feasibility, video frame dropping/backpressure beyond the single-worker snapshot scheduler, dynamic dialogs/delegation, icons and launcher regeneration, viewer clipboard/document UX | Installed Windows ARM and Mac app suite; foreground shortcuts, trackpad, latency/CPU/image-quality measurements; secure/GPU windows remain explicit |
| P4 | Independent clone identity/TPM design, portable archive/compatibility UX, interruption cleanup and Windows protected restore | Adequate destination storage and verified Windows TPM/protected-data recovery; clone and source must run independently |
| P5 | Live supported host limits, memory/storage pressure policy, sleep policy, multi-viewer coordination and repeatable boot/input/load benchmarks | Actual Mac resource/energy observations, data preservation and lifecycle acceptance |
| P6 | Audio/microphone integration, friendly USB inventory/attach/detach/hotplug, Ubuntu3D and Windows ARM graphics feasibility | Real Mac peripherals, host permission paths and compatible guest drivers. Current QEMU/HVF backend has no Apple GPU assignment path |
| P7 | Recent-app UX, updater authenticity/rollback and clean-Mac upgrade/uninstall acceptance; disposable guest automation beyond owned backup jobs | Trusted release/signing design, clean Mac and secure Developer ID/notarization access |

The current app viewer is bounded JPEG snapshots. Protocol/input improvements do not establish video, universal Windows compatibility or a performance advantage over Parallels. Linux cloud fixtures and native CI remain useful independent evidence; they cannot replace the hardware/driver/guest gates above. Continue the approved roadmap in dependency order, with cloud-independent work where a concrete design and useful acceptance check exist.

Continuation order and per-milestone acceptance: [next delivery plan](continuation-preview21.md). First detailed task: [viewer scheduling](../superpowers/plans/2026-10-16-viewer-scheduling.md). These are proposed future work, not completed milestones.

## Current priority: Coherence continuous video

The owner prioritizes continuous smooth video with low resource use. Other feature expansion is deferred during this milestone. [Proposed video architecture and acceptance gates](../superpowers/specs/2026-10-18-coherence-video-design.md). This is a design proposal, not implemented video or an accepted performance result.
