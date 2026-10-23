# Mac Minimal UI Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement this approved visual direction inline.

**Goal:** Apply selected design 1 to the existing FreeLane desktop manager.

**Architecture:** Keep egui and existing VM operations. Recompose the library sidebar, selected VM header, preview, hardware summary and tabs. Bundle a generated preview illustration; render real snapshots when available.

**Tech Stack:** Rust, eframe/egui, PNG assets.

**Spec:** `docs/design-preview/01-mac-minimal.png`, selected by the user.

## Global Constraints

- Existing native app, no replacement web dashboard.
- English and Vietnamese through the existing language selector.
- Preserve lifecycle guards, settings, sharing, applications and recovery.
- Status is observed data; never infer that a VM is off from an unavailable status.
- Native macOS window decorations remain owned by the OS; do not draw traffic lights.
- This cloud task is already isolated; work in the existing checkout (onboarding skill).

## Review Focus

- Unknown/running/paused states must not be labeled powered off.
- Narrow 800×600 window must retain primary actions and scrollable content.
- Long names and paths must not push actions outside the window.
- Unselected VM switching and bookmark repair must retain busy guards.
- Live screenshot input remains attached to the real snapshot rectangle.

### Task 1: Assets and composition

**Files:** `crates/desktop/assets/`, `crates/desktop/src/interface.rs`, `crates/desktop/src/main.rs`, `crates/desktop/src/i18n.rs`.

- [x] Bundle the blue illustration and licensed OS/control icons, with attribution.
- [x] Recompose sidebar, VM header, preview, compact summary and overview/sharing tabs.
- [x] Keep optional capture controls and lifecycle operations accessible through menus.
- [x] Build desktop and run existing desktop tests; visual-only changes need screenshot verification rather than tests that mirror layout constants.

### Task 2: Visual and interaction verification

**Files:** `docs/design-preview/`, `design-qa.md`.

- [x] Launch only an owned Xvfb display with private fixture VM profiles and real sparse qcow2 files, never boot them.
- [x] Capture VI/EN at normal and minimum window sizes; exercise tabs, settings, VM selection, search, menus.
- [ ] Compare the selected reference and actual capture in one comparison image. Fix actionable mismatches and recapture.
- [x] Run formatting/clippy and record physical Mac rendering limitations.

Final font-adjustment screenshot recapture was interrupted. Existing captures are before that change; visual QA remains blocked on recapture. Source/build/tests are ready for review.

## 2026-10-23 continuation

- [x] Finish semibold capture on current source (earlier interrupted step resolved).
- [x] Reduce compact-window preview/sidebar density, keep three everyday tabs and retain secondary views in More.
- [x] Capture VI/EN overview, compact menu, Display, Applications and Sharing; compare source/rendered full view and focused header.
- [x] Workspace tests284passed/9ignored; desktop clippy/build and formatting passed. See latest `design-qa.md` result.
