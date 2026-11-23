# Performance

How to measure the game's speed, what each graphics setting costs, and what was learned
making it faster. Measure before and after any change to lights, shadows, post-processing or
the level.

## Measuring

```sh
scripts/bench.sh --release                 # invisible display, 3840x2160
scripts/bench.sh --release -s 1920x1080    # another size
scripts/bench.sh --release -c "r_ssao 0"   # with console commands run first
scripts/bench.sh --release --window        # fullscreen on this desktop (takes the screen)
scripts/bench.sh                           # the dev build, as `cargo run` plays
```

It runs `tests/harness/bench.cfg`, a fixed route of seven views (the arena from a platform,
the campfire, along a wall, the far forest, a bot firefight, and two views at night), holds
still at each for three seconds, and prints a table:

```
view               avg ms    fps   p99 ms    worst   gpu ms   cpu ms  cpu p99
arena                6.21    161     8.69      9.9     4.93     2.04     3.39
```

| Column | |
|---|---|
| `avg ms`, `fps`, `p99 ms`, `worst` | Time between frames. Only trustworthy with `--window`. |
| `gpu ms` | GPU time per frame, from GPU timestamps. Reliable even on the invisible display. |
| `cpu ms`, `cpu p99` | CPU time of the game's own update. Also reliable anywhere. |

Below the table are the slowest GPU passes per view. `auxiliary_views` is shadow maps.
Results are saved in `target/bench/`.

In the game, `perf_reset` starts a measurement and `perf` prints the same numbers for one
spot. GPU timing is on under the harness, or when the game starts with `GPU_TIMINGS=1`.

**To find out what a slow moment in real play was,** start the game with `PERF_LOG` set and
play normally:

```sh
GPU_TIMINGS=1 PERF_LOG=perf-log.txt cargo run --release
```

It writes one line per second: fps, average, p99 and worst frame time, GPU and CPU time,
window size, where the camera was, and whether the window was unfocused or the console open.

The HUD's FPS is the average over the last 120 frames. Bevy's own smoothed FPS barely smooths
at high frame rates (its smoothing factor is in seconds, about 16 ms), so one slow frame made
the readout drop to that frame's rate, and a game running at 150 fps could show 70.

**Read GPU and CPU times, not fps, from invisible runs.** The invisible display paces frames
on its own schedule, so the same build can show 100 fps one run and 140 the next. GPU and CPU
times barely move between runs.

## What each setting costs

GPU ms at 3840x2160 on an RTX 4080 SUPER, release build, range over the bench route.

| `r_quality` | GPU ms | What it turns off |
|---|---|---|
| `high` (default) | 5.0-5.9 | Nothing. The game as designed. |
| `medium` | 2.8-4.1 | One lamp shadow instead of two, two sun cascades instead of four, low-quality SSAO, no contact shadows. Hard to tell from high. |
| `low` | 1.1-1.4 | All lamp and campfire shadows, SSAO, contact shadows, bloom. One sun cascade, FXAA. |

Measured one at a time from `high` (savings in GPU ms at 4K):

| Feature | Saves | |
|---|---|---|
| Each lamp's shadow | ~0.7 | Six extra renders of everything near it. Also CPU: see below. |
| SSAO | ~1.3 | Its pass time at 4K, high quality. Low quality is ~0.4. |
| Contact shadows | 0.1-0.25 per light | Turning them off on all 11 lights saved 1.1-2.6. Now only the sun or moon and the campfire have them. |
| Bloom, fog, anti-aliasing | under 0.3 each | |
| Sun cascades | under 0.2 | Four is fine. |

**Shadow-casting lights also cost CPU.** Every view the renderer draws (the screen, and six
for each point light with shadows) has to consider every separate mesh. At 1280x720, where
the GPU isn't the limit, frame time was 3.2 ms with no shadows and 11 ms with lamp and
campfire shadows on, before the level's meshes were merged. Keep the number of separate
meshes down (the level builder merges them, see `world/builder.rs`) and the number of
shadow-casting point lights small.

## What was done, and the results

Started from a report of frames as low as 54 fps on a 4080 at 4K.

1. **Measurement first:** `perf`, `bench.cfg` and `bench.sh`.
2. **Contact shadows only on the sun or moon and the campfire.** All nine lamps had them.
3. **Lamp shadows follow the camera.** Three fixed lamps had shadows wherever you were. Now
   the two nearest the camera have them (`r_lamp_shadows`).
4. **Merged the level's static meshes** into one per 24 m area (about 260 meshes before).
5. **`r_quality` presets** for slower machines.

GPU ms, release build, 3840x2160, invisible display:

| View | Before | After |
|---|---|---|
| arena | 6.52 | 5.10 |
| campfire | 7.58 | 5.87 |
| wall | 5.60 | 5.24 |
| forest | 5.35 | 4.97 |
| firefight | 7.81 | 5.44 |
| arena, night | 6.59 | 5.18 |
| campfire, night | 8.20 | 5.94 |

Fullscreen on the 4K TV (119 Hz), release build, after steps 2 and 3: 150-178 fps average,
p99 frame time 6.9-8.1 ms on every view. The target was 120 fps with p99 under 8.3 ms.

**Dev builds are slower on the CPU side.** The dev build optimizes the game's own code less
(`opt-level = 1`) and shows more uneven frame times: p99 9.7-11.7 ms in the same fullscreen
run, against 6.9-8.1 ms in release. Raising the dev opt-level to 3 made no measurable
difference to update time and slowed rebuilds by about 10%, so it stays at 1. To judge speed,
use a release build.

## Real play on the desktop

30 seconds of play at 4K on `medium`, logged with `PERF_LOG`, on Wayland and then XWayland:
both averaged 120 fps, GPU about 3 ms, and nearly every second had a frame of about 16 ms.
The game's own CPU time was 3-7 ms (p99 about 9 ms), against about 2 ms on the bench route,
even while standing still. XWayland made no difference, so it isn't Bevy's Wayland frame
pacing issue ([#21833](https://github.com/bevyengine/bevy/issues/21833)). Not yet profiled:
the next step is a CPU trace of real play.

## Not measured yet

- Angelo's laptop (ThinkPad T470s, Intel HD 520). `low` is meant for it; whether it holds
  60 fps there is unknown.
- 1440p and 1080p in fullscreen.
