# Measurements: Marea in Quickshell vs. Marea at high tide

Both, on the same machine, on the same monitor, at the same time. Marea is the
one Abel uses every day (`~/Proyectos/proyecto-marea`, 370 QML files);
`marea-plm` is this folder.

How to measure, so the results can be repeated:

```sh
# CPU, memory, and threads for one process, over N seconds
medir.sh <pid> 30 "etiqueta"
```

It reads `/proc/<pid>/stat` twice, N seconds apart, for CPU time, and
`/proc/<pid>/status` and `smaps_rollup` for memory. PSS is what it actually
occupies: shared memory divided among the processes that share it.

## September 21, 2026 · the entire control center

With sound and the session controls included, the panel at 520 × 510 and the
surface at 820 × 680. Measured with the bar open and animating, on HDMI-A-1 at
60 Hz.

| | CPU | RSS | PSS | frame |
| --- | --- | --- | --- | --- |
| marea-plm, animating | 8,6 % | 130,9 MB | 113,5 MB | 16,84 ms · p99 17,06 |

And the day's scare, which is the thing worth recording. With the face chain
written as `x + (k - x) * t`—each link naming the previous one **twice**—and
fifteen links, the same scene measured **41,09 ms per frame (24 fps), 100,6 %
CPU, and 1 058 MB**. It was not the runtime: yesterday's scene with today's
binary ran at 16,8 ms. It was that a `let` is substituted wherever it is named,
so the expression tree **doubled with every link**: 2¹⁵ nodes per property,
evaluated twice per frame.

It was fixed in both places. In the scene, by writing all 48 links as
`x * (1 - t) + k * t`, which names `x` once. And in pleamar, which no longer
substitutes a large `let`: it calculates it once per frame. With that, the
scene that blew up is back to **16,78 ms and 8,87 %** without changing a line
in it.

## September 20, 2026 · the little ball

Just the little ball: its body, its face as it breathes, looks around, and
blinks, the aura, and the notification drop. Marea, meanwhile, is the whole
thing (with its panels, services, and face with accessories behind it), so
**this comparison is not fair yet**: it is a starting point, not the result.

| | CPU | RSS | PSS | threads | first frame |
| --- | --- | --- | --- | --- | --- |
| Marea (Quickshell) | 12,98 % | 245,8 MB | 204,7 MB | 14 | not measured |
| marea-plm, animating | 4,93 % | 111,7 MB | 94,1 MB | 13 | 175 ms |
| marea-plm, idle | 0,32 % | 109,3 MB | 92,2 MB | 13 | 191 ms |

**What it tells us.** While animating—breathing, blinking, looking around—
pleamar costs a third of the CPU and less than half the memory. And **at rest
it drops to 0,3 %**, because the renderer sleeps when there is nothing to move:
Quickshell stays at 13 % even when nothing is happening on screen.

The 4,93 % for animation is about 0,8 ms of CPU per frame at 60 Hz. That points
to the first item to investigate: the drawing list is rebuilt from scratch
every frame, whether anything changed or not (pleamar limitation P8).

## September 20, 2026 · the entire card

Now with everything: the little ball, the card that comes out of its body, the
time, date, volume, all nine desktops with their pill, and the title of the
frontmost window. That is 62 instructions, 14 animated properties, and 11
zones.

| | CPU | RSS | PSS | threads |
| --- | --- | --- | --- | --- |
| Marea (Quickshell) | 13,15 % | 197,4 MB | 156,5 MB | 14 |
| marea-plm, card open | 5,32 % | 112,9 MB | 96,0 MB | 17 |
| marea-plm, card closed | 5,05 % | 112,9 MB | 96,0 MB | 17 |

**What it tells us.** Opening the entire card costs 0,3 % more than keeping it
closed. In other words: **what gets drawn barely matters**. And the reverse is
interesting too: going from a circle to a full bar did not move the needle.

## Where the 5 % comes from

With `PLEAMAR_CRONO=1`, pleamar prints a breakdown of every frame:

```
crono · hueco 16,04 · apuntar 0,10 · cerrar 0,21 · mandar+presentar 0,23 ms
```

The 16 ms spent “waiting for the opening” is the monitor's wait at 60 Hz, and
it is **sleep**, not CPU. The actual work costs 0,5 ms: preparing the commands,
closing the command buffer, and submitting it—the Vulkan wgpu floor, which has
to be paid even when only a circle is moving. It is recorded in pleamar as
limitation P11, along with its plan.

For comparison, Quickshell spends about 2 ms of CPU per frame doing the same
thing.

## September 20, 2026 · with the effects enabled

On top of the above: the desktop pill **stretches as it travels** (at the speed
of its own spring, which only the renderer knows), each desktop **lights up**
when hovered, with its own spring, and when the minute changes **the time
slides in from above** with a shove that the spring returns. That is 24
animated properties, 71 instructions, and 32 rules.

| | CPU | RSS | PSS | threads |
| --- | --- | --- | --- | --- |
| Marea (Quickshell) | 13,16 % | 188,1 MB | 147,3 MB | 14 |
| marea-plm, with effects | 5,40 % | 113,1 MB | 96,4 MB | 17 |

**What it tells us.** Ten more animated properties, three new effects, and
nineteen more rules: **0,08 % CPU**. Adding motion here costs nothing because
the expense is the floor for presenting a frame, and that is already paid.

That is the opposite of what happens in QtQuick, where each `NumberAnimation`
and each `Behavior` is an object evaluated on the UI thread.

## A bug uncovered by writing this

The card was supposed to arrive without a single line of logic, but it did
not: if a scene did not have its `.luau` next to it, pleamar gave it a dummy
script that did not set up its services, and the clock stayed at `--:--`.
Fixed in pleamar the same day (commit “A scene without logic also sets up its
services”). Actually writing it finds things no checklist can.

## September 20, 2026 · frames and startup

With Abel's permission, Marea was relaunched with Qt's timer
(`QSG_RENDER_TIMING=1`) and returned to its original place with its exact
environment in under a minute.

**Time until it appears:**

| | until first frame |
| --- | --- |
| Marea (Quickshell) | 1876 ms (1579 just to read its configuration) |
| marea-plm | 175 ms |

**What it does per frame**, while left idle:

| | frames in 20 s | cost of each |
| --- | --- | --- |
| Marea (Quickshell) | 475 (~24 per second) | 0,61 ms · polish 0,05 · sync 0,12 · render 0,11 · swap 0,16 |
| marea-plm | 1200 (60 per second) | 0,50 ms · prepare 0,10 · close 0,21 · submit 0,23 |

**What it tells us, and this is the most interesting part.** Marea draws **half
as many frames** as marea-plm, and each frame costs **the same**, but it uses
**two and a half times more CPU**. In other words, its cost is not in drawing.
It is in everything around it—the QML bindings, JavaScript, and services—which
is exactly what does not exist in pleamar, because the renderer executes what
is declared and the logic only counts things.

And when there is nothing to move, marea-plm drops to 0,3 % and Marea does not:
it stays where it is.

## September 20, 2026 · the fair comparison

Both running at once, both freshly started, both animating on the same monitor.
This is the comparison that counts.

| | CPU | actual memory (PSS) | frames per second | CPU per rendered frame |
| --- | --- | --- | --- | --- |
| Marea (Quickshell) | 6,17 % | 335,8 MB | ~24 | 0,257 % |
| marea-plm | 4,96 % | 81,3 MB | 60 | 0,083 % |

**What it tells us.** The CPU difference by itself is 20 %, not three times:
that should be said. But marea-plm is drawing **two and a half times more
frames**. For every frame that reaches the screen, pleamar spends **three
times less**.

And there is no argument about memory: **four times less**, with the same bar.

## September 20, 2026 · opening and closing

Open each one's card, measure, close it, and measure again. Marea is asked
through its own interface (`quickshell ipc call marea open`), without touching
the mouse; marea-plm is controlled with `pleamar --decir`.

| | closed | open | closed again |
| --- | --- | --- | --- |
| Marea (Quickshell) | 6,17 % | 12,28 % | 10,40 %, and still 9,50 % after 30 s |
| marea-plm | 4,39 % | 4,89 % | 4,50 % |

**What it tells us.** Opening Marea's card **doubles** its CPU, and closing it
does **not return to the original level**: it stays halfway there, still there
half a minute later. What was instantiated on opening stays instantiated, with
its bindings alive.

In marea-plm, opening costs half a point and closing gives it all back, because
nothing is instantiated: the card was already declared, and what changes is a
number animated by the renderer.

**Honesty note**: Marea's card contains much more than marea-plm's (panels,
chat, agents). The 12,28 % is not “the same thing drawn at a higher cost.”
What is comparable is **the pattern**: in QML, opening something creates
objects that stay; here, opening means moving a property.

## A caveat about these measurements

Freshly started, Marea uses 5,75 % CPU and 386 MB; the same Marea after hours of
running uses 13,2 % and 188 MB. Quickshell's numbers **move** depending on how
long it has been running and what is open. Pleamar's, measured three times in
three different states, stayed between 5,0 and 5,4 %.

When measuring again, measure both at once and after they have been running
for the same amount of time.

## What remains to be measured

- **GPU**: `nvidia-smi` reports 926 MiB and 7 % for the entire session, not per
  process.
- Marea's card open with its panels inside, which is when Quickshell actually
  instantiates things. This requires asking Abel to interact with the bar while
  measuring.
- And the same over a longer period: an hour with both running.
