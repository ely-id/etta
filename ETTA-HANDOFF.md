# Etta — handoff

Status on **2026-10-04**, in the afternoon. The “fields, launcher, Deriva,
Tailscale, and Color Picker” phase is in the last four commits in the §4
table. Written so work can resume in a new session without rereading the
previous conversation.

---

## 1. What the project is

**Etta** is a desktop shell for the Wayland compositor **Halley**
(<https://github.com/saltnpepper97/halley>, 0.7.0, API 1). It was born from
**Marea** (`k4ditano/marea-plm`, BSD-3), from the `ely-id/marea-plm` fork,
branch `halley-compat`. The fork has the Halley adapter and was imported
without history. License attribution is in `LICENSE` and `NOTICE.md`.

- **The name:** comet 1P/Halley produces the Eta Aquariids meteor shower; hence
  Etta.
- **The runtime:** the installed **pleamar 0.2.8**. pleamar is **not forked**:
  source at `~/Work/pleamar`, origin `k4ditano/pleamar`, commit `4163c91`;
  binary at `~/.local/bin/pleamar`; 0.2.6 backup at
  `~/.local/state/etta-pleamar-0.2.6.bak`.
- **Language:** the product (text, comments, commits) is **English only**.
  Conversation with the user is in Portuguese.

### Rules that do not change

1. **The pet stays intact:** body, face, accessories, movements, animations, and
   reactions. Only **where she interacts with the panels** may change, along
   with the approved adjustments:
   - larger size (56 px default, options 46/52/56/64);
   - OSD glyphs in the eyes, thick enough to preserve her face;
   - the lock morph on the lock screen;
   - the new wardrobe pieces.
2. **Priorities:** architecture, then functionality, then UX/UI.
3. **External or irreversible actions require confirmation:** push, publish,
   change the user's configuration. The user approves each action.
4. **Commits:** one per milestone, ending with
   `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Local Git
   identity: “Ely”.

### Stages

- Stage 1 (understanding): ✓
- Stage 1.5 (study): ✓
- Stage 2 (interactive prototypes): ✓, approved. Canvas with 27 artboards at
  <https://claude.ai/artifact/BtBjMxWBmidS49AJUyvfMv>, files
  `project/*.dc.html`. It is the **visual reference for everything**.
- **Stage 3 (implementation): in progress.**
  - Phase 1: UX/UI.
  - Phase 2: correct integration with Halley.
  - Both phases are progressing together, because each milestone is tested
    live in Halley.

---

## 2. Where things are

| Path | What it is |
| --- | --- |
| `etta.plm` | The scene (≈4,900 lines): the pet, surfaces, and rules. Imports `ui/*.plm` and `common/*.plm` |
| `etta.luau` | The logic: services, persistence, and external processes |
| `ui/tokens.plm` | Design tokens (`tk.*`: colors, type scale, radii). Each token documents its usage |
| `ui/kit.plm` | Kit: `Btn`, `Ib`, `Toggle`, `Slider`, `Seg` (up to 5 items), `Chip`, `Tag`, `Kbd`, `Row`, `Bubble`, `FieldBox` (the plate for a text field), plus the `Kit.i_*` figures (75 SVG icons in `ui/icons/`) |
| `ui/panels.plm` | All control center pages as components (see §4) |
| `ui/showcase.plm` | Kit showcase scene |
| `common/accessories.plm` | Piece physics: `Perched`, `Pinned`, `Tagalong`. **Do not touch** (it belongs to the pet) |
| `wardrobe/*.svg` | The 20 pieces |
| `wallpapers/*.frag` | 6 custom field shaders, plus `space.png` (photo of Halley's shader; see `NOTICE.md`) |
| `halley/` | `etta-halley` adapter (Rust): `outputs` with transform and scale, `window …`. Tests: `cargo test --release --offline` |
| `tools/shader-thumb/` | `etta-shader-thumb`: Halley GLSL ES → GLSL 450 → glslangValidator → SPIR-V → wgpu → PNG. Builds offline |
| `tools/reservas` | Node reader for Claude Code, Codex, Copilot (via `gh api /copilot_internal/user`), and Grok quotas (no API: the card says so) |
| `deriva/` | Deriva library (`deriva-worker`, already compiled). Tests: `cargo test --release --offline` **with `XDG_DATA_HOME` pointing to a temporary folder** (tests isolate with `ETTA_DERIVA_DIR`; until this milestone they used the old name `MAREA_DERIVA_DIR` and would have run against the real library) |
| `etta` | Launcher: `start`, `stop`, `status`, `report`, and `<event>`. Adds `halley/`, `tools/shader-thumb/`, and `deriva/` to PATH, exports `ETTA_DIR`, and runs `--stall 0` |
| `docs/contract/` | Contract inherited from Marea (animations and measurements) |

**Runtime data:**

- `~/.local/share/pleamar/etta/`: `settings.json`, `history.json`,
  `halley-wallpaper.rune`.
- Log: `~/.local/state/etta/etta.log`. The launcher keeps **one** previous
  log, so a second restart erases a crash log.
- Cache: `~/.cache/etta/` (shader thumbnails and media covers).
- User shaders: `~/.config/etta/shaders/`.

---

## 3. Running and testing (on the user's machine)

```sh
etta start --screen HDMI-A-1        # or just `etta start`; the link is in ~/.local/bin/etta
etta status
pleamar --check etta.plm            # compiles the scene and everything it imports
pleamar --say etta "fact open true" # does something live: fact X V · get X · emit EVENT [payload]
grim -o HDMI-A-1 /tmp/s.png && magick /tmp/s.png -rotate 180 /tmp/v.png   # HDMI is physically rotated 180°
```

- **Hot reload:** the scene and logic reload when saved.
- **What requires restarting the instance:**
  - new surfaces;
  - environment variables.
- **Permissions** (`permissions { run: …; services: … }`) reload on save in
  pleamar 0.2.8: the log says “permissions now: …”. Even so, every new binary
  called with `run()` must be added to the list.
- **To restart without killing the shell itself:**
  `kill $(pgrep -f "^/home/ely/.local/bin/pleamar --scene etta.plm")`, then
  `etta start`. **Do not use** `pkill -f "pleamar --scene etta.plm"`: it
  matches its own command line and kills the shell.
- **Lock-screen preview without locking the session:**
  `fact lock.preview true`, or Settings → Lock → Preview.
- **Simulated capture selection:** `fact sel.x/y/w/h …`, then
  `emit shot_commit`.

In a cloud session, **there is no Halley or Wayland**. You can edit and, if
pleamar is compiled there, run `pleamar --check etta.plm`. Visual testing can
only be done on the user's machine.

---

## 4. Status: what is done (commits)

| Commit | Milestone |
| --- | --- |
| `0c67b3d`…`294fdc8` | Import, rename Marea→Etta, English only, everything in **Geist** (`family: "Geist"` in each text) |
| `846a92d` | Tokens, icons, kit |
| `d4a4fed` `f7e8dc2` | Clock and battery always next to her (card with weather, upcoming events); OSDs (volume, mute, brightness, mic, caps, charger, low battery, device, network, DND) |
| `083008f` | Control center and 13 pages as components; new quick settings (keep awake = `systemd-inhibit`, power = `powerprofilesctl`, night light = `wlsunset`); Bluetooth lifts rfkill; per-app volume; scrollable Settings; Agents with 4 providers and chips; hourly forecast; History with search and filters |
| `62f05b8` | First review fixes: `Ib` did not emit clicks; media (also paused, cover, player); Bluetooth in My devices/Nearby; **no 60 fps limit** (judder at 100 Hz); toast and swell redesigned (her mechanics preserved) |
| `90afc94` | Wardrobe: 5 slots (`wear.head/face/ears/neck/side`), 20 pieces, tab by slot, preview; companions to her left, after the stones |
| `7acfab3` | Wallpapers in Halley: field shaders and thumbnails; applying writes `halley-wallpaper.rune`, which `halley.rune` includes with `gather` |
| `0c5fd00` | Lock screen: she turns into the lock (dots, red `> <`, `^ ^` as the shackle opens; unlocks 0.9 s later); safe preview |
| `4efb5c8` | Custom region capture (frozen frame, guides, measurements, her as W×H HUD; release saves to `~/Pictures/Screenshots/Screenshot_…png` + clipboard; Esc cancels; R/W/S switch mode); “saved” card shared with recordings |
| `0dd7a84` | Look: **Glass blocked on Halley** (screencopy crash); Frosted is translucent only |
| `c7a2f92` | Native recording tested (NVENC + audio), `~/Videos/Recordings/Recording_…mp4`, 180° rotation marked with `ffmpeg -display_rotation` |
| `3c51cf2` | **Stability on Halley** (see §4.1) and **Finder, menu, Haven capsule, and “Welcome back”** redesigned according to the prototypes |
| `cf036be` | **Centered text fields** (an `input`'s `at` is its top corner; `FieldBox` in the kit, `tk.field.*`/`tk.mid.*` tokens) and **Finder sees new apps**: Etta reads `.desktop` files itself on each opening, following pleamar's `apps` service rules (what it lists is what `apps.launch` accepts); searches by file, program, class, keywords, and ordered letters |
| `8a9ca38` | **Deriva**: color-coded chips (links, files, images, notes), “Clear” with a three-option dialog (trash · permanently delete · cancel) and 5 s “Undo”; reads the entire library in pages; no “Clear” during a search. `deriva-worker`: batched `trash`/`untrash`, `purge` (rows + files no one else uses; never with `--db`), `list --before` |
| `42e5a28` | **Tailscale**: pill next to the clock (the 9 dots: lit T = connected, wavy = transferring, mint dot = file received) that opens a card: this device, the others (MagicDNS name), copy address, send files (zenity or drop onto the row), received files in `~/Downloads/Taildrop` with a notification, connect/disconnect |
| (this commit) | **Color Picker** (Super+Shift+P, menu, Finder): capture overlay without dimming, with a ×10 magnifier she holds, HEX/RGB read from the frame; the color goes to the clipboard and to **mini Etta**, which emerges from inside her, follows her (below the stones when she is out; to the right, near the clock, when she is tucked away), uses her face (`Face` gained `fg`/`hole`, with the previous defaults), and opens the color panel (HEX highlighted, RGB light, each format copies on click; “Clear” sends it back inside Etta) |

### 4.1 What the stability round fixed (user test report)

- **Clicks leaking between panels:** `Seg` declared `zone box opt.$i`
  inside a component. Names with `$` lose the instance suffix, so all `Seg`
  instances shared 5 zones: “Face” in Wardrobe also selected Frosted + eDP-1,
  and any index 2 enabled “Follow focus”. Now each option is a `SegOpt` copy.
- **Dead keyboard** (Esc, Finder, R/W/S): only the last
  `keyboard: … while` declaration applied (the one in lockpreview). There is
  now **one** declaration, on the main surface, `exclusive`. In Halley,
  `on_demand` only acquires the keyboard on click/hover in a layer that is
  already on-demand. While Etta holds the keyboard, Halley keybinds are
  disabled; so she mirrors Super+Space, Super+Shift+L/S/V, and the volume,
  brightness, and media keys. An `on focus … { blur }` rule removes focus from
  invisible fields.
- **Esc** closes whatever is open, on any page. Previously it cancelled the
  Wi-Fi password, city editor, and session confirmation.
- **Capture:** inside a named surface, `pointer.y` is in the plane
  (+10000 per copy). Each copy learns its own offset (`ox`/`oy`) from the
  zone. The overlay opens on **all** monitors; the clicked one is captured.
  Window is hidden on Halley. Right-click and Cancel exit. The adapter gained
  `selected` (the output selected by Halley).
- **Shaders/Deriva:** the scroll zone was over the cards. It now comes before
  them. The 7th shader was unreachable: the scroll limit rounded down to 0.
  There is now “1–6 of 7” pagination with ‹ ›.
- **Settings/Tray/Calendar:** zones for scrolled rows did not respect `view`.
  Each control is now active only when its entire row is visible, and the
  header is drawn after the list.
- **Clock frame:** a delayed transition survived a newer one. Targets are now
  `if(cond, 1, 0)`, and the card is clipped.
- **Lock preview:** clicking the background no longer closes it. It has a
  Close pill, opens on her monitor, and does not show suspend/shutdown.
- **Persistence:** the monitor name is saved only when you choose it. “View
  rules” in the capsule had no handler; it has one now.

---

## 5. Changes made on the user's machine (outside the repo)

- **`~/.config/halley/halley.rune`** (backups:
  `halley.rune.bak-before-etta-wallpaper` and `.bak-before-etta-binds`):
  - `gather "~/.local/share/pleamar/etta/halley-wallpaper.rune" as etta_wall`;
    the `wallpaper:` block reads `etta_wall.etta.*`;
  - autostart: `once "/home/ely/.local/bin/etta start"` (previously `marea`);
  - keybinds, with absolute path `/home/ely/.local/bin/etta …`:
    - Super+Space `search`;
    - Super+Shift+L `lock`;
    - Super+Shift+S `shot_region`;
    - Super+Shift+V `record_toggle`;
    - brightness, media, and microphone mute.
- **Link** `~/.local/bin/etta` → `~/Work/etta/etta`.
- **Marea (PID 943) was left running** until the next login; the user can run
  `marea stop`.
- **Tailscale:** the user became an operator (`sudo tailscale set --operator=$USER`,
  which they ran themselves). Etta created `~/Downloads/Taildrop` and keeps a
  `tailscale file get --loop` running to receive files.
- **`settings.json`** gained `colors` (the HEX values, newest first) and
  `color_format` (`hex`/`rgb`).
- **Pending approval:** the Color Picker shortcut in `halley.rune`
  (see §7). Until then, Super+Shift+P only works while Etta holds the
  keyboard.
- **Test files in the user's folders** (can be deleted):
  - `~/Pictures/Screenshots/Screenshot_2026-10-03_06-54-25.png`;
  - `~/Videos/Recordings/Recording_2026-10-03_06-57-34.mp4`, `06-58-11`,
    `06-58-33`, and `07-00-01`.

---

## 6. Lessons learned (save hours)

**pleamar (scene):**

- `text` with `size`, `weight`, and `family` **only accepts literals**. To vary
  weight or size, use two texts with `show`.
- A color `let` inside a component cannot see other color lets. Put
  `pick()`/`mix()` directly in `color:`.
- Text `pick()` accepts literals, live text, and slots, but **not nested
  `pick`**: use a single index with `if()`.
- Texts cannot be used in expressions (`x != ""` does not compile). Use a fact
  from the logic or a `measure`.
- `measure name` must be **declared** (`measure x`) before using `measure: x`
  in a text, including inside components.
- A `let` cannot see facts and props declared inside `repeat`. Use
  `prop … ~16ms` with `follow`.
- Enum values cannot collide with other names. Hence the prefixes:
  `o_`, `h_`, `f_`, `e_`, `n_`, `s_`, `l_`, `sm_`, `rk_`.
- **`grid` draws every possible record in the model** (up to `max`): guard it
  with `show: r.index < model.count`. `column` and `row` with `for` do not
  have this problem.
- Library components **do not export** zone names to the scene: `on
  scroll/press` rules for a zone stay inside the component. Layout names inside
  a component are also internal (`list.content` does not resolve; calculate
  it yourself).
- A standalone `clip` applies to the end of the **group**, or to the next
  `surface` if declared at scene level. Always wrap it in a `group`.
- `shader x = file "…"` in a library is relative to the library
  (`"../shaders/…"`).
- `shader { at: }` is the **corner**; `zone box { at: }` is the **center**;
  `box { from: }` is the corner.
- Events with `->` reach the logic; without the arrow they remain in the
  scene. Rule payload: `on event(v) { … }`.
- `on focus` and `on blur` do not receive an input name.
- There is no crosshair cursor (vocabulary: default, pointer, text, grab,
  grabbing).
- The `lock` kind: the plane is centered on the monitor;
  `lockscreen.width/height` gives its actual size.
- For `screens: each` surfaces with rules that read the pointer, open only on
  the target monitor (`open: … and screen.index == X`).
- **A name with `$` inside a component is shared by all copies** (no instance
  suffix). In a component, use fixed names or a sub-copy (`SegOpt`).
- **`keyboard: … while` is global to the scene**: the last declaration wins,
  and the mode is the main surface's. Declare it only on the main surface.
- **Use `exclusive` in Halley**, because `on_demand` does not receive the
  keyboard when opening. With exclusive, Halley keybinds stop working: mirror
  the important ones in the scene.
- When acquiring the keyboard, pleamar focuses an input (even an invisible
  one), and the field consumes letters and Enter before the rules. `blur`
  fixes it. Enter in a field submits even with Ctrl.
- **Zones ignore `clip` and `view:`**: a scrolled-out row remains clickable.
  Guard by position (`live:`/`active:`).
- **On a named surface, `pointer.x/y` are in plane coordinates** (copy n at
  y + 10000·n); `local.x/y` in a zone rule are correct.
- **A delayed transition is never cancelled**, and its target is read when it
  fires. Write `if(cond, 1, 0)`, never `1`.
- `on still X for T` fires **once**: if the guard is false at that moment, it
  does not fire again. (That is how `face = happy` got stuck.) Prefer gestures
  (`play …`) for short reactions.
- The renderer paints the entire frame on Halley. `PLEAMAR_FULL_REPAINT`
  applies only to frames borrowed from pleamar-wm.
- Simulated mouse (`--mouse`) uses **plane** coordinates: copy 0 of the first
  named surface is at y+10000. `--mouse "focus+@… key:Escape@… type:abc@…"`
  simulates the keyboard. This can test real components in a small scene that
  imports `ui/`.

- **An `input`'s `at` is the top-left corner** (no `anchor`; line height =
  size × 1.3). To center at a `y`, use `y - size × 0.65`
  (`tk.mid.s14`, etc.). The field's clickable zone follows the same `at`.
- **A color does not come from a fact**, but a color is three expressions per
  channel and `mix` is linear in each: `mix(mix(mix(#000000,#ff0000,r),
  mix(#00ff00,#ffff00,r),g), mix(mix(#0000ff,#ff00ff,r),
  mix(#00ffff,#ffffff,r),g), b)` gives exactly (r, g, b).
- **Only one gesture slot per scene**: a long (5 s) `state` gesture rejects
  her reactions while it lasts. For a fuse, use `spin` (and end it with the
  clock: `spin` stops with reduced motion).
- `on change X` only fires when the value changes: to do it “again, unchanged,”
  use a counter (`drift.seq`).
- Images are sampled with a **linear** filter (the ×10 magnifier stays smooth).
- A `name.$k` zone in a scene `repeat` is called `name.0`, `name.1`… for
  `on("drop:name.0")` in the logic.

**Luau (logic):**

- The main chunk **is at the 200-local limit** (two more local functions
  caused the logic to fail in this milestone): put new code inside
  `;(function() … end)()`, in `fx.*`/`hooks.*`, or in a named function (see the
  end of `etta.luau`).
- `run` and `spawn` only return **stdout** (stderr goes to the null device).
- pleamar's `apps` service reads `.desktop` files **once**, and a second
  `sys.watch("apps")` only adds a listener (it does not reread them): hence
  the custom scan (`fx.read_apps`).
- A process argument has a size limit (128 KB): ID lists are sent in batches
  (`fx.drift_batches`, one thousand at a time).

**Tailscale and Agents:**

- `tailscale up` with **any** flag (even `--timeout`) requires repeating all
  non-default preferences (here `--ssh`) and refuses to run. The button uses
  `tailscale up` without flags, with `spawn` and a 20 s timeout from Etta.
- The Claude Code CLI token (`~/.claude/.credentials.json`) expires in about
  8 h and only the CLI renews it; the desktop app stores a different one.
  `tools/reservas` runs `claude -p /usage --no-session-persistence` (local
  command: no turn, no cost) when it has expired, at most every 10 min, then
  reads the server. Codex is read from the server
  (`chatgpt.com/backend-api/wham/usage`, the same as `/status`) using the token
  from `~/.codex/auth.json`, without renewing it.
- In the Claude API, `is_active` marks the limit that matters most now, not
  whether the window has started.
- `files.write` only writes to Etta's data folder.
- `sys.watch(service)` can be registered **once**. Additional hooks use the
  `also.network/bluetooth` table.
- On reload, the logic can start before the scene (one “no fact called X”
  error): touch `etta.luau` again.
- `sys.ask("auth.check")` blocks the logic. The scene displays the states
  itself.

**Halley:**

- Screencopy rotates monitors with `transform 180` **twice**. HDMI captures
  and video come out upside down and are corrected by Etta (see `freeze()` and
  `upside_down`). **Report to Halley.**
- pleamar's Glass (`lens`) uses `zwlr_screencopy`, and Halley refuses the
  buffer (“buffer does not match the advertised capture format”). It is a
  protocol error and pleamar exits. **Report to Halley and/or pleamar.**
- Halley's layer-shell blur is rule-based (`layer-rule namespace`) and applies
  to the entire surface. All pleamar surfaces use the `pleamar` namespace,
  including the full-screen catcher, so **only the panel cannot be blurred**.
- API v1 does not expose the presented geometry of windows: capture's
  **Window** mode does not work on Halley.
- The adapter's “focused output” comes from the focused window. Without a
  focused window, Etta falls back to the monitor where it lives.
- `halleyctl config verify -c FILE` validates a copy. Halley automatically
  reloads files combined with `gather`.
- The “selected output” (the last one clicked) comes from
  `client.trail(None).output`. Full-screen capture and recording use it.
- **Permissions** (`services:`) now reload on save: `clipboard` and
  `clipboard.*` were added without restarting.

---

## 7. Next milestone

**First, the things from this milestone that can only be confirmed by live
testing:**

- Tailscale: send and receive files with the iPhone (Taildrop; the Tailscale
  app must be open/connected on the iPhone); drop a file onto a card row; the
  zenity dialog receiving keyboard input in Halley.
- Color Picker on both monitors (HDMI is rotated 180°), arrow keys to move the
  pixel, Enter, H/R, Esc; mini Etta emerging, following Etta (both out and
  tucked away), and being absorbed by “Clear”.
- **Shortcut to approve** in `halley.rune` (`keybinds:` block, alongside
  Etta's other bindings):

  ```rune
    "$var.mod+shift+p" "/home/ely/.local/bin/etta pick_color"
  ```

  (`Super+Shift+C` is already Halley's `cluster-mode`.)

**Known limitations in this milestone:**

- The `model frames` declares `image 1920, 1080`: on a monitor that is not
  16:9, the frozen frame is drawn fitted, and the picker (which reads the pixel
  from the file) would be misaligned with what it displays. Both monitors are
  1920×1080 today.
- In pleamar, drag-and-drop (data device) tests zones using the surface's local
  coordinates, without the plane origin: over the `catcher`, a drop would hit
  a zone on the main surface somewhere else. Therefore the `catcher` closes
  while something is being dragged (`drag.over`).
- The magnifier enlarges using a linear filter; the pixel to be picked is
  painted over it using the exact color read from the frame.

**What came before this milestone (still applies):**

**Polish and optimization round**, along with the live test results from this
milestone:

- fine alignment of all pages against the prototypes (requested by the user);
- perceived smoothness: measure again with `etta report`;
- live-check what only real testing can confirm (see the list in the final
  milestone message): physical keyboard in Finder and the menu, Esc on each
  page, capture on both monitors, Ctrl+Enter, “Keep what I copy” (clipboard
  via data-control in Halley), and urgent notifications reaching Haven.

**Incorrect values in the user's `settings.json`** (written by the `Seg` bug):
`follow: true`, `home: "eDP-1"`, `lock_idle: 2`, `snooze: 2`. The user should
reselect these in Settings. Do not edit the file without asking.

**Known backlog** (outside this milestone; record and schedule):

- **Lock when idle:** the adapter needs to use `ext_idle_notifier_v1`. Fact
  `idle.k` already exists and is saved.
- **Caps Lock:** currently read with `cat` every 300 ms. Move it to the
  adapter.
- **Keep awake:** ideally through `zwp_idle_inhibit` in the adapter. Currently
  uses logind.
- **Window capture on Halley:** delegate to Halley's native selector
  (`client.capture(CaptureMode::Window, …)` in halley-api) using
  `etta-halley capture window`.
- **Stuck face:** the generic rule that resets `face = happy` (device/network
  OSD, agents) fires only once. It can leave the happy face stuck if something
  else is happening at that moment. Make it robust.
- **Agents:** Codex appears as “No window read yet” when the latest rollout
  does not have `rate_limits`.
- **README and NOTICE:** update with installation on Halley (keybinds,
  autostart, `gather`).
- **Report to Halley:** double rotation in screencopy, screencopy buffer
  format, and lack of presented window geometry in API v1.
- **Report to pleamar** (texts ready in this session's diagnosis,
  `~/.cache/etta-dev/diagnosis.md`):
  - name with `$` in component without instance suffix;
  - global `keyboard: … while` (last one wins);
  - zones ignoring `clip`/`view`;
  - `pointer.x/y` in plane coordinates on named surfaces;
  - delayed transition not cancelled;
  - invisible field that takes focus when keyboard is acquired;
  - (from this stage; the user asked **not to report yet**) `apps` service not
    rereading `.desktop` files; `input` without `anchor` or placeholder color;
    image without `nearest` filter; color not coming from a fact.

## 8. How the user works

- Communication in **Portuguese**. They like short updates during long tasks.
- For changes to `halley.rune` or any of their configuration: **show the
  snippet first**. The wallpaper structure with `gather`, keybinds, and
  autostart have already been approved and applied.
- They validate everything live and come back with a list of inconsistencies.
- The remote repository is **private**: `github.com/ely-id/etta`.
