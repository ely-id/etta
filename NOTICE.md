# Where Etta comes from

Etta began as **Marea**, the desktop companion written in
[pleamar](https://github.com/k4ditano/pleamar) by Abel
([k4ditano/marea-plm](https://github.com/k4ditano/marea-plm)). The ball, her
face, how she breathes, looks, sleeps, celebrates, travels between monitors and
wears her wardrobe are Marea's, and Etta keeps them as they were: the contract
that says how each of those animations must feel is kept in
[`docs/contract/ANIMACIONES.md`](docs/contract/ANIMACIONES.md), with the
measurements and the evidence it refers to.

What Etta changes is what is around her: the panels, the clock, the OSDs, the
lock screen's padlock, the capture flow, and her integration with the
[Halley](https://github.com/saltnpepper97/halley) compositor.

## Licences

- Everything in this repository is under the BSD 3-Clause License in
  [`LICENSE`](LICENSE), which keeps Marea's copyright notice as that licence
  requires.
- `halley/` is the adapter that talks to Halley through its public SDK,
  `halley-api`, which is GPL-3.0-only. The adapter's own source follows the
  BSD 3-Clause License, but the executable built from it links the SDK and is
  therefore distributed under GPL-3.0-only (`halley/COPYING`). Etta talks to it
  as a separate process.
- `wallpapers/` holds field shaders for Halley's wallpaper. The ones written for
  Etta are BSD 3-Clause; Halley's own bundled `space` shader is not copied here.
