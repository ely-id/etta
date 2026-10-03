# Halley adapter

`marea-halley` supplies Marea's existing window operations through Halley's
supported `halley-api` SDK. Halley v0.7.0 does not advertise the foreign-toplevel
protocol used by Pleamar's generic window service. The adapter streams an
initial window snapshot and focus/window changes, and activates or restores
windows by their semantic IDs. Output names and logical capture bounds come
from `wl_output` and `xdg-output`, including transformed and scaled outputs.

Marea selects this backend when `XDG_CURRENT_DESKTOP` includes `Halley`. Its
launcher builds the adapter on first use when Cargo is available. An already
installed `marea-halley` on PATH also works. To build and check it separately:

```sh
cargo build --release --locked --manifest-path halley/Cargo.toml
cargo test --locked --manifest-path halley/Cargo.toml
```

The SDK is pinned to Halley v0.7.0, commit `7eb8cd8e`, and negotiates API v1.
There is no private IPC codec in this adapter and no compositor impersonation.
The adapter source follows Marea's BSD-3-Clause license. The SDK is GPL-3.0-only,
so the combined adapter executable is distributed under GPL-3.0-only; that
license is included in `COPYING`. Marea communicates with it as a separate
process.

Two API limitations remain. On an empty multi-output desktop the SDK does not
report the compositor's selected output; Marea can follow window focus, and
single-output capture remains unambiguous. Focused-window screenshots need the
window's presented rectangle, which API v1 does not expose: SDK node positions
are Field coordinates, affected by camera movement and fullscreen. Marea
cancels that operation explicitly instead of cropping an unrelated region.
Region and output screenshots retain Marea's existing screencopy path, and
recording retains its existing `wf-recorder` path.

Marea also selects the overlay layer for command search on Halley while a
window is fullscreen. The existing condition is retained on other compositors.
See [VALIDATION.md](VALIDATION.md) for runtime coverage and remaining limits.
