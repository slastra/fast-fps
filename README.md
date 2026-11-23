# fast-fps

A small low-poly LAN shooter in [Bevy](https://bevyengine.org/) 0.20. A walled courtyard at
night, lit by lamps and a campfire, with a ruined keep, raised platforms and practice dummies.
There are no menus: everything runs from the `~` console.

The look follows Jan Hohenheim's [Foxtrot](https://github.com/janhohenheim/foxtrot) demo:
no ambient light, a night-sky environment map, warm point lights, HDR with bloom. Movement is
[bevy_ahoy](https://github.com/janhohenheim/bevy_ahoy) (Quake-style, with mantling and ledge
climbs), physics is [Avian](https://github.com/Jondolf/avian), and networking is
[bevy_replicon](https://github.com/simgine/bevy_replicon) over renet UDP.

## Run

```sh
cargo run
```

Click to grab the mouse. WASD, Space to jump (hold to mantle), Ctrl or C to crouch, left
click to fire, right click to scope (sniper), R to reload, 1-5 or the wheel to switch
weapons, Q for the last weapon, Tab for scores, Esc to release the mouse.

## Weapons

| | Fire | Mag | Damage (head) | Notes |
|---|---|---|---|---|
| AK | auto, 600 rpm | 30 | 34 (136) | climbs 9 shots, then snakes left, right, left |
| MP5 | auto, 800 rpm | 30 | 24 (72) | gentle climb, falls off with range |
| Glock | semi | 20 | 22 (70) | gets wild if you click too fast |
| Shotgun | pump | 8 | 9 x 17 (27) | wide fixed spread, short range |
| Sniper | bolt | 5 | 115 (230) | only accurate scoped and still; two zoom levels |

Spraying works like Counter-Strike. Each automatic weapon follows a fixed pattern, so the
same spray lands in the same shape every time. The view kicks along half of it; pull the
mouse down against it to keep shots on target. On top of the pattern there's a random cone
that grows when moving, jumping or spraying and shrinks when crouched. The crosshair gap is
that cone, projected to the screen.

## Animation

Other players are a jointed low-poly skeleton, animated procedurally. Nothing is keyframed.

- **Locomotion.** Feet follow a stride cycle along the direction of travel and the legs are
  solved to them with inverse kinematics, so walking, running, strafing, backpedaling and
  crouch-walking all come from the same code. Jumps tuck the legs; landings dip the hips.
- **Aim.** The gun is held along the aim direction and both arms are solved to its grips, so
  hands stay on the weapon at any pitch. Each weapon has its own hold. Shots kick the gun,
  reloads work the magazine, and hits rock the upper body away from the shot.
- **Ragdolls.** A kill builds a physics copy of the body in the exact pose it died in: ball
  joints at the spine, neck, shoulders and hips, hinges at elbows and knees. It's thrown the
  way the shot was going, harder for the shotgun and sniper, and the gun drops separately.
  Corpses are cosmetic, simulated on each machine, and fade after 12 seconds.

Each position update carries velocity plus crouch, airborne and reload flags, which is all
the animation needs. You don't see your own body, only your view model.

### Practice bots

Bots are players with no client behind them, for watching animations and target practice.
They work when hosting or offline, and clients see them as ordinary players.

```
~  exec cfg/bots.cfg        # four bots: walking, strafing, running, crouching
~  bot                      # one more, five meters in front of you
~  bot_do 5 jump            # orders: idle walk run strafe back crouch stand jump
~  bot_do all fire 10       #         reload fire weapon look pin free remove
```

Bots walk fixed circles (`pin` makes them animate in place), pass through cover, and their
shots do no damage.

## Play on a LAN

One machine hosts, the others join:

```
~  host              # listens on UDP 27015 and announces itself on 27016
~  connect           # joins the first server found on the LAN
~  connect 192.168.1.20
```

Launch arguments are console commands, so `cargo run -- +name alice +host` works too.

## Console

| Command | |
|---|---|
| `host [port]`, `connect [ip[:port]]`, `disconnect`, `find`, `status` | networking |
| `name <name>`, `sens <value>`, `fov <degrees>` | saved to `~/.config/fast-fps/config.cfg` |
| `kill` | respawn |
| `weapon <name>`, `zoom <level>` | weapons |
| `bot [x y z yaw]`, `bot_do <id\|all> <order> [arg]` | practice bots |
| `r_aa <smaa\|taa\|fxaa\|off>`, `r_ssao`, `r_bloom`, `r_fog`, `r_contact`, `r_shadows`, `r_viewmodel`, `r_ambient <brightness>` | render switches |
| `exec <file>`, `wait <secs>`, `screenshot <name>`, `tp`, `look`, `spin`, `fire`, `pos` | scripting and testing |

`help` lists everything. Tab completes command names.

## How it fits together

- **Authority.** Damage, kills, respawns and dummies run whenever this machine isn't a
  client, so the same code drives offline play and hosting (Replicon's listen-server pattern).
- **Movement is client-side.** Each machine runs its own kinematic controller and streams its
  pose to the server. The server does the authoritative hit-scan against player hitboxes,
  which shrink when a player crouches. This trusts clients, which is fine for a LAN.
- **The level is generated from a fixed seed**, so every peer builds identical colliders.
- **The gun is drawn by the main camera**, scaled down toward the eye so it stays inside the
  character's collision radius and never clips walls. No second camera means no
  multi-camera HDR compositing and no TAA ghosting.
- **Sounds are synthesized** into WAV buffers at startup. The only asset is a CC0 night-sky
  cubemap.

## Test harness

`scripts/harness.sh` runs one or more console scripts, each in its own game instance, in a
throwaway headless sway, then collects screenshots and console logs under `target/harness/`.
Nothing injects input; the scripts drive the game from inside.

```sh
scripts/harness.sh tests/harness/artifacts.cfg                       # render check while turning
scripts/harness.sh -s 1920x1080 tests/harness/spray.cfg              # every weapon's spray on a wall
scripts/harness.sh tests/harness/anim.cfg                            # every animation state, on bots
scripts/harness.sh -t 80 tests/harness/ragdoll.cfg                   # kills and ragdolls
scripts/harness.sh tests/harness/lan_host.cfg tests/harness/lan_client.cfg   # host + client
scripts/harness.sh -v tests/harness/viewmodel.cfg                    # visible, nested sway
```

`cargo test` covers the pose solver: every pose in a sweep of speeds, stances, aim angles
and weapons must be finite, keep feet above ground, keep hands on the gun, and bend elbows
and knees only the way the ragdoll hinges allow.

`-c DIR` reuses a config directory between runs (see `settings_save.cfg` and
`settings_load.cfg`).

## Credits

Night Sky HDRI 001 by ambientCG, CC0. Project structure started from
[bevy_new_2d](https://github.com/TheBevyFlock/bevy_new_2d).

## License

MIT or Apache-2.0.
