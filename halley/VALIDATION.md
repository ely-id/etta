# Halley v0.7.0 compatibility validation

Validated on 2026-10-01 against upstream Marea commit
`a58918912df332267e07b43c180c039a1abb6b0f`, its pinned Pleamar 0.2.6
(`8bb920f0bbb23e12f180f81bdcc20b86b8ee1108`), and Halley v0.7.0
(`7eb8cd8e6e7153cbb6815129358af9b30a6a21fa`).

**Result: partially compatible.** Upstream startup and generic services work,
but window integration, output capture selection, and fullscreen command
handling need the changes in this branch. Focused-window screenshots remain
unsupported by API v1, and the tested recorder fails protocol negotiation.

## Architecture and reproduced failures

Pleamar provides layer-shell, Wayland output discovery, screencopy and
ext-session-lock. Its generic window backend needs
`zwlr_foreign_toplevel_manager_v1`, which the running Halley does not advertise.
Halley does advertise layer-shell v5, screencopy v3, session-lock v1,
xdg-output v3, wl_output v4, and xdg-activation v1. Activation tokens alone do
not enumerate windows or replace the missing foreign-toplevel operations.

Marea's original monitor/capture helpers call `hyprctl` under Hyprland and
`pleamar-wm hyprctl` elsewhere. The latter is unavailable in a Halley session:
an output capture fell back to region selection, and a focused-window capture
produced no file. Pleamar's window service also reported unavailable while
Halley's public API reported a focused native test window. Under fullscreen,
Marea received its search command but remained in the occluded top layer.

The isolated `marea-halley` executable uses the supported, version-negotiated
SDK for window snapshots, events, focus and restore; ordinary Wayland output
metadata supplies logical capture bounds. Marea retains its existing services
on other compositors. Halley is detected by its normal desktop identifier;
there is no compositor impersonation or private IPC implementation.

Audio uses PulseAudio/PipeWire, media uses MPRIS, notifications and status
notifiers use session D-Bus, and NetworkManager/BlueZ use system D-Bus. File
and application search do not require compositor IPC. Niri uses Pleamar's
generic window path; pleamar-wm-specific behavior remains separately gated.
Marea captures directly with grim/slurp and records with wf-recorder; these
operations do not depend on desktop portals.

## Runtime matrix after the changes

PASS means the stated operation was exercised in the isolated session.
PARTIAL means only part of the capability or environment was validated.

| Capability | Status | Evidence or limit |
| --- | --- | --- |
| Layer-shell startup | PASS | Actual Halley connection and first frame; scene validation passed. |
| Persistent Marea surface | PASS | Process and command socket survived service, capture, lock and fullscreen tests. |
| Keyboard-triggered commands | PASS | A temporary Halley F11 binding invoked the Marea wrapper; Escape closed command search. |
| Notifications | PASS | D-Bus Notify and GetServerInformation calls returned successfully. |
| Audio/media | PASS | Real audio state and device discovery; disposable MPRIS metadata and PlayPause/Next/Previous calls. Audio mutations were not exercised. |
| Wi-Fi/Bluetooth/system controls | PARTIAL | Actual NetworkManager/BlueZ state queried. Radio changes, pairing, power actions and logout were not exercised. |
| Lock screen | PARTIAL | Compositor confirmed session lock; screencopy denied while locked; invalid PAM credential rejected; controlled test unlock restored capture. Valid-password unlock not tested. |
| Screenshots | PARTIAL | Output PNG measured 1280x800; pointer-selected region measured 201x151. Focused-window capture is explicitly cancelled: API v1 lacks presented geometry. |
| Screen recording | FAIL | wf-recorder 0.6.0 bound linux-dmabuf v4 while software Halley advertised v3. Its no-dmabuf option also failed during binding. Native DRM recording not tested. |
| File/app search | PASS | Installed application and disposable file found through actual Marea commands. |
| Tray/status notifier | PASS | Disposable item registration reached Marea; activation reached its D-Bus method. Full dbusmenu coverage not tested. |
| Output enumeration | PASS | wl_output/xdg-output returned the actual named test output and logical bounds. |
| Monitor-follow behavior | PARTIAL | Focused window resolved to the sole output. Cross-output movement not exercised. |
| Focused monitor detection | PARTIAL | Focused SDK node supplied its output. API v1 lacks selected-output state on an empty multi-output desktop. |
| Open-window enumeration | PASS | SDK initial snapshot and state-change events reached Marea's window integration. |
| Focused-window detection | PASS | Actual native client reported focused through the adapter. |
| Window activation/restore | PASS | Marea search activation and restore command reached Halley; collapsed client restored and focused. |
| Fullscreen interaction | PASS | Native client confirmed fullscreen size; command composition changed and keyboard input closed search. |
| Physical multi-monitor behavior | NOT TESTABLE | Offscreen winit supplies one output. Rotated/scaled/negative output geometry covered by unit tests only. |
| Portal integration | PARTIAL | Actual Halley portal Screenshot succeeded; ScreenCast CreateSession, SelectSources, Start and Close succeeded with a PipeWire stream. Consumer frames and native portal frontend routing not tested. |
| Startup/autostart | PARTIAL | Source launcher and restart passed. Native session launcher/autostart and Nix package build not exercised. |
| pleamar-wm-only behavior | NOT APPLICABLE | Halley session does not expose pleamar-wm services. |

## Test boundaries and checks

The release Halley binary ran with its winit backend inside an offscreen Xvfb
host, software Mesa rendering, a private session bus, and a disposable home.
The release Pleamar runtime and Marea connected to that compositor, not the
active Hyprland session. No desktop configuration was changed and no Halley
installation or source modification was made. Captured images were used only
for numeric validation and were not displayed or included in this report.

The active graphical session was preserved, so native DRM, hardware input,
physical multi-monitor, GPU-specific rendering and native autostart remain
unverified. An attempted nested Hyprland regression session also failed its
linux-dmabuf v4 binding against the test host; no existing-compositor runtime
regression result is claimed. Existing non-Halley branches were reviewed and
retained. No workaround was added for the external recorder's binding error.

Automated checks: release build with the committed lockfile; three adapter
unit tests; cargo fmt check; cargo clippy across all targets with warnings
denied; launcher shell syntax; Pleamar scene check; git diff whitespace check.
The scene check retains the same pre-existing loose-clip note as upstream.
Nix was unavailable, so the package wiring was reviewed but not built.

```sh
cargo build --release --locked --manifest-path halley/Cargo.toml
cargo test --locked --manifest-path halley/Cargo.toml
cargo fmt --manifest-path halley/Cargo.toml -- --check
cargo clippy --locked --manifest-path halley/Cargo.toml --all-targets -- -D warnings
sh -n marea
pleamar --check marea.plm
git diff --check
```
