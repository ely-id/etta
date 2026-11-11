<h1 align="center">Etta</h1>

<p align="center"><em>A small companion for the Halley desktop.</em></p>

Etta is a desktop companion for the [Halley](https://github.com/saltnpepper97/halley)
compositor: a little ball with a face that lives at the top of your screen and
is your whole shell — clock and battery, notifications, control center, finder,
captures and recordings, lock screen. She is written in
[pleamar](https://github.com/k4ditano/pleamar).

<p align="center">
  <img src="docs/images/control-center.png" alt="Etta's control center" width="320" />
  <img src="docs/images/wardrobe.png" alt="Etta's wardrobe and appearance settings" width="320" />
</p>
<p align="center">
  <img src="docs/images/wallpapers.png" alt="Etta's wallpaper and shader settings" width="320" />
  <img src="docs/images/lock-screen.png" alt="Etta's lock screen" width="320" />
</p>

Her name comes from Halley's comet: the trail of debris 1P/Halley leaves behind
gives the Eta Aquariids meteor shower. From *Eta*, Etta — the small character
that lives alongside Halley.

Etta began as [Marea](https://github.com/k4ditano/marea-plm), by Abel. See
[`NOTICE.md`](NOTICE.md) for what she keeps from Marea and what is new.

## What is Marea's, what is hers

The character is Marea's and is kept as she was. What Etta changes is what is
around her, and the compositor she lives on.

| Kept from Marea | New in Etta |
| --- | --- |
| The ball and her face | The Halley adapter (`halley/`), and six wallpaper shaders written for Halley |
| How she breathes, looks, sleeps and celebrates | The control center and its pages, rebuilt as components on shared design tokens and a kit of 75 icons (`ui/`) |
| How she travels between monitors | The clock and battery card beside her, and the OSDs |
| How she wears her wardrobe | The lock screen, where she becomes the padlock |
| Her own shaders (`shaders/`) | Her own capture overlay, and recording |
| Deriva's library (`deriva/`) | The finder, the Tailscale pill and the colour picker |
| | A wardrobe of twenty pieces in five spots, where Marea had four pieces |

The contract that says how each of Marea's animations must feel, with its
measurements and evidence, is kept in [`docs/contract/`](docs/contract/).

## What she needs

- The [Halley](https://github.com/saltnpepper97/halley) compositor. The adapter
  is pinned to Halley 0.7.0.
- [pleamar](https://github.com/k4ditano/pleamar) 0.2.8 or newer on the `PATH`.
- The Geist typeface, which every text is set in.
- `cargo`, only to build the adapter, the first time she starts.
- Tailscale, only for her Tailscale pill.

## Run her

```sh
git clone https://github.com/ely-id/etta ~/Work/etta
~/Work/etta/etta start            # her log goes to ~/.local/state/etta/etta.log
```

On Halley she builds her small adapter (`halley/`) the first time she starts,
if `cargo` is installed.

In `halley.rune`:

```rune
autostart:
  once "/home/YOU/Work/etta/etta start"
end

keybinds:
  "$var.mod+space" "/home/YOU/Work/etta/etta search"
  "$var.mod+shift+l" "/home/YOU/Work/etta/etta lock"
  "$var.mod+shift+s" "/home/YOU/Work/etta/etta shot_region"
  "$var.mod+shift+v" "/home/YOU/Work/etta/etta record_toggle"
  "$var.mod+shift+p" "/home/YOU/Work/etta/etta pick_color"
end
```

For her Tailscale pill to send and receive files, your user has to be
Tailscale's operator, once: `sudo tailscale set --operator=$USER`. What
arrives goes to `~/Downloads/Taildrop`.

## Talk to her

```sh
etta start | stop | status | report
etta search | lock | shot_region | shot_screen | shot_window | record_toggle | pick_color
etta volume_up | volume_down | mute_output | mute_mic | bright_up | bright_down
```

Click her for the control center, right-click for her menu. The pill beside
the clock is Tailscale: your devices, their addresses, and files sent to them
by dropping them on their row. A colour picked (her menu, the finder or
Super+Shift+P) goes to the clipboard and to a little her who keeps them. Her settings are
saved in `~/.local/share/pleamar/etta/settings.json`.

## How she is made

| Path | What it is |
| --- | --- |
| `etta.plm` | The scene: her, her surfaces and their rules |
| `etta.luau` | The logic: services, persistence and external processes |
| `ui/` | The design tokens, the kit every panel is built from, and the control center pages |
| `common/` | The physics of the pieces she wears |
| `wardrobe/` | Her twenty pieces |
| `shaders/` | Her own shaders |
| `wallpapers/` | Wallpaper shaders for Halley |
| `halley/` | The Halley adapter, in Rust ([its notes](halley/README.md)) |
| `deriva/` | Deriva's library, in Rust |
| `tools/` | The wallpaper thumbnail renderer, and the reader of the agents' quotas |
| `etta` | The launcher: starts her, stops her, and tells her things |
| `docs/contract/` | The animation contract kept from Marea |

## Where she stands

She was built in October 2026, against Halley 0.7.0, and has been tried on one
laptop with a second monitor. The Glass look is kept off on Halley, which
rejects its screencopy buffer.

## License

BSD 3-Clause, keeping Marea's notice. The Halley adapter executable is
GPL-3.0-only. Details in [`NOTICE.md`](NOTICE.md).
