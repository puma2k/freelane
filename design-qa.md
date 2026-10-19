# Mac Minimal native desktop design QA

Source visual truth: `docs/design-preview/01-mac-minimal.png` (user selected design 1).
Implementation: Rust/egui desktop manager; owned Linux Xvfb, software rendering, isolated fixture library. No VM boot or real Mac acceptance claimed.
Viewport: 1440×1024 and 800×600 client pixels, density 1. Source 1487×1058 pixels includes outer margin/macOS traffic lights; implementation captures client content, with OS decorations deliberately left to the OS. Browser/CSS/console checks are inapplicable to this native app.
State: two fixture VM profiles with real sparse qcow2 files, selected Windows/Ubuntu; unavailable runtime honestly shown as unknown, not the mock's powered-off status. No configured share fabricated.

## Comparison history

1. Initial capture: `/workspace/video-transport-tmp/ui-design-20261019/implemented-vi.png`.
   - P1: state overlay child layout advanced the parent cursor, overlapping summary and tabs with artwork. Fixed by painting through an independent child UI after reserving the full image rectangle.
   - P2: actions/summary columns collapsed to content widths. Fixed explicit column/title minimum widths.
2. Second capture: `implemented-vi-v2.png`.
   - P2: artwork preserved its aspect ratio by shrinking width, leaving a large right gap. Fixed cover UV crop with exact display bounds.
   - P2: sidebar drifted from the approved split-view proportion. Fixed full-width use of the 300px default panel.
3. Final captures: `docs/design-preview/implemented-mac-minimal-vi.png`, `implemented-mac-minimal-en-small.png`, `implemented-sharing-vi.png`, `implemented-small-scrolled.png`. Full comparison `comparison-mac-minimal.png`; readable focused header comparison `comparison-header.png` (raw 1× crops). Inspected native settings, Ubuntu selection, sharing/app tabs, footer menu and minimum-window scrolling on the owned display. Earlier overlap, collapsed columns and unused image width are resolved.
4. Focused header comparison found the mock's semibold weight missing: egui `strong()` adjusts text color, not weight. Added a real bundled bold fallback and macOS variable-font wght=600 selection. Build/clippy pass; test fixture initialization now calls the same font setup as production. Final native screenshot recapture after this last font adjustment was interrupted, so visual acceptance remains blocked for that last change. Committed screenshots are explicitly the captures before the semibold change, not claimed to show the last font adjustment.

## Required fidelity surfaces

- Typography: macOS system font retained on Mac; bundled DejaVu fallback on Linux. Body15/button14/small12, selected title32; dedicated semibold family, truncation for names and paths. Linux font shape/antialiasing differs from the SF mock by platform.
- Layout: split sidebar, unified compact toolbar, selected-machine actions, large preview, three-column summary, tabs, detail/share columns. Compact header puts actions on a second line, main content scrolls at 800×600.
- Colors: blue primary #0070f5, selected sidebar #dae9fd, gray sidebar #f4f5f8, white main, charcoal text. No website or invented dashboard added.
- Assets: generated raster illustration, licensed Simple Icons OS marks and Tabler controls, embedded and cached per context. Real VM snapshots replace the illustration when captured. No screenshot used as the entire UI.
- Copy: VI/EN via the existing selector. Real status and share configuration replace example mock data; accurate GiB units replace the mock's GB. Existing extra Display/Details operations remain reachable rather than removed for superficial fidelity.

## Review

Fresh read-only code review: no important regressions found in lifecycle guards, selection/relinking guards, sharing/clipboard/transfers/app/recovery access, image input mapping or asset caching. Review found inaccurate GB label; corrected to GiB.

## Follow-up polish / limits

- P3: Ubuntu currently uses the same decorative blue illustration, with actual Ubuntu identity and configuration rendered separately.
- Physical Apple Silicon: SF rendering, native chrome/material, actual VM display and input, and sleep/wake remain for the previously deferred Mac acceptance.

final result: blocked

## Verification

Final workspace rerun after semibold and fixture setup fix: 284 passed, 9 ignored, 0 failures. Desktop build and clippy passed. Font-family test setup initially caused two renderer-test failures after semibold; fixed by installing production styles in the test context, without changing assertions. No claim that an interrupted screenshot step passed.
