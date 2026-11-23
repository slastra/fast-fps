# fast-fps

A small low-poly LAN shooter in [Bevy](https://bevyengine.org/) 0.20, built to be copied and
turned into your own game. A walled courtyard at night, five guns with learnable spray
patterns, animated players, ragdolls, practice bots, and LAN multiplayer. There are no menus:
everything runs from the `~` console, and the same console is how the tests drive the game.

![Bots firing in a line](docs/img/bots-firing.jpg)

## Run

```sh
cargo run
```

The first build compiles the engine and takes a while. After that it is quick.

Click to grab the mouse. WASD to move, Space to jump (hold at a ledge to climb), Ctrl or C to
crouch, left click to fire, right click to scope (sniper), R to reload, 1-5 or the wheel to
switch weapons, Q for the last weapon, Tab for scores, Esc to release the mouse.

For something to shoot at: press `~` and type `exec cfg/bots.cfg`.

## Play on a LAN

One machine hosts, the others join. Press `~`, then:

```
host                    listens on UDP 27015 and announces itself on the LAN
connect                 joins the first server found
connect 192.168.1.20    or name it
```

Launch arguments are console commands too: `cargo run -- +name alice +host`.

## Make your own game

```sh
scripts/new-game.sh ~/Projects/Rust/space-arena
```

That makes a complete, working copy under a new name. It gets its own window title, settings
folder and network protocol, so it can never join a game of the original by accident. Three
files change a game's character the most:

- `src/config.rs`: health, run speed, jump height, gravity
- `src/weapon/defs.rs`: the guns and their spray patterns
- `src/world/arena.rs`: the map

## Working on it

| | |
|---|---|
| [AGENTS.md](AGENTS.md) | Start here, especially if you are an AI agent: commands, rules, traps. |
| [docs/tour.md](docs/tour.md) | A map of the code and how a shot travels through it. |
| [docs/recipes.md](docs/recipes.md) | Step-by-step: add a weapon, change the map, add a command, sync a value. |
| [docs/harness.md](docs/harness.md) | The test harness: scripting the game and checking what it does. |
| [docs/console.md](docs/console.md) | Every console command. |

```sh
scripts/check.sh --quick     # format, lint, unit tests, and a smoke test of the real game
scripts/check.sh             # everything
```

## What's in it

**Weapons.** AK, MP5, Glock, shotgun and sniper. Spraying works like Counter-Strike: each
automatic weapon follows a fixed pattern, the view kicks along half of it, and you pull the
mouse down against it to stay on target. On top of that is a random cone that grows when you
move, jump or spray and shrinks when you crouch. The crosshair gap is that cone, drawn to
scale. The sniper is only accurate scoped and standing still.

| | Fire | Mag | Damage (head) |
|---|---|---|---|
| AK | auto, 600 rpm | 30 | 34 (136) |
| MP5 | auto, 800 rpm | 30 | 24 (72) |
| Glock | semi | 20 | 22 (70) |
| Shotgun | pump | 8 | 9 x 17 (27) |
| Sniper | bolt | 5 | 115 (230) |

**Animation.** Other players are a jointed skeleton animated in code, with nothing keyframed.
Feet follow a stride cycle and legs are solved to them with inverse kinematics, so walking,
strafing, backpedaling and crouch-walking come from one piece of code. Arms are solved to the
gun's grips, so hands stay on the weapon at any aim angle.

**Ragdolls.** A kill builds a physics copy of the body in the exact pose it died in and
throws it the way the shot was going. The gun drops separately.

![Ragdolls](docs/img/ragdolls.jpg)

**Networking.** One machine is the authority for damage, score and spawning; the same code
runs offline. Each player simulates their own movement and reports it, which trusts clients
and is fine for a LAN.

**Generated content.** The map, guns, bodies and sounds are built from code at startup. The
only asset file is a night-sky image.

## Credits

The look follows Jan Hohenheim's [Foxtrot](https://github.com/janhohenheim/foxtrot) demo.
Movement is his [bevy_ahoy](https://github.com/janhohenheim/bevy_ahoy), physics is
[Avian](https://github.com/Jondolf/avian), and networking is
[bevy_replicon](https://github.com/simgine/bevy_replicon) over renet. Night Sky HDRI 001 by
ambientCG, CC0. Project structure started from
[bevy_new_2d](https://github.com/TheBevyFlock/bevy_new_2d).

## License

MIT or Apache-2.0.
