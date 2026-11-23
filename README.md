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
click to fire, R to reload, Tab for scores, Esc to release the mouse.

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
| `r_aa <smaa\|taa\|fxaa\|off>`, `r_ssao`, `r_bloom`, `r_fog`, `r_contact`, `r_shadows`, `r_viewmodel` | render switches |
| `exec <file>`, `wait <secs>`, `screenshot <name>`, `tp`, `look`, `spin`, `fire`, `pos` | scripting and testing |

`help` lists everything. Tab completes command names.

## How it fits together

- **Authority.** Damage, kills, respawns and dummies run whenever this machine isn't a
  client, so the same code drives offline play and hosting (Replicon's listen-server pattern).
- **Movement is client-side.** Each machine runs its own kinematic controller and streams its
  pose to the server. The server does the authoritative hit-scan against player hitboxes.
  This trusts clients, which is fine for a LAN.
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
scripts/harness.sh tests/harness/lan_host.cfg tests/harness/lan_client.cfg   # host + client
scripts/harness.sh -v tests/harness/viewmodel.cfg                    # visible, nested sway
```

`-c DIR` reuses a config directory between runs (see `settings_save.cfg` and
`settings_load.cfg`).

## Credits

Night Sky HDRI 001 by ambientCG, CC0. Project structure started from
[bevy_new_2d](https://github.com/TheBevyFlock/bevy_new_2d).

## License

MIT or Apache-2.0.
