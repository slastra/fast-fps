# A tour of the code

Every source file starts with a comment saying what it is for. This page is the map.

## The idea

- **The console is the interface.** There are no menus. Hosting, joining, settings, bots and
  tests are all console commands (`~` in game, `+command` on the command line, or a script).
- **The server decides, clients ask.** One machine is the authority for damage, score and
  spawning. "Server" includes playing offline: the same code runs either way.
- **You move yourself.** Each machine simulates its own character and tells the server where
  it is. That trusts clients, which is fine among friends on a LAN.
- **Looks are generated.** The map, the sky, the guns and their icons, the bodies and the
  sounds are all built from code at startup. The only asset file is the HUD font.

## Folders

```
src/
  main.rs        Assembles the app from the plugins below. Nothing else.
  config.rs      The game's name and the numbers you are most likely to tune.
  physics.rs     Collision layers: who can touch whom.
  render.rs      The camera and its post-processing: bloom, shadows, anti-aliasing.
  sky.rs         The four times of day: sky dome, sun or moon, light, fog, exposure.
  lowpoly.rs     The shape kit: beveled blocks, crates, barrels, rocks, pines, grass.

  console/       The drop-down console. The whole user interface.
    mod.rs         Commands, the registry, typing, `help`.
    ui.rs          The panel.
    script.rs      Launch arguments, `exec` files, `wait`. The test harness runs on this.
    settings.rs    Values saved to config.cfg between runs.

  net/           Multiplayer plumbing.
    protocol.rs    Everything sent over the network. Start here.
    session.rs     host / connect / disconnect / status / name.
    discovery.rs   Finding games on the LAN.

  server/        The rules. Runs when hosting or offline, never on a client.
    mod.rs         The order the rules run in each frame.
    players.rs     Joining, leaving, spawning, positions, hitboxes.
    combat.rs      A shot or knife swing arrives; decide what it hit and what that does.
    grenades.rs    Thrown grenades: their flight, fuse and blast.
    dummies.rs     Practice targets.
    bots.rs        Practice bots and the `bot` commands.

  player/        The character you control.
    mod.rs         Spawning it, reporting its position, respawning.
    input.rs       Key bindings and mouse capture.
    view.rs        The view rig, sensitivity, field of view.

  weapon/        Guns, the knife and the grenade.
    defs.rs        Stats and spray patterns. Tune or add weapons here.
    models.rs      What each weapon looks like.
    arsenal.rs     What you're holding, switching, the scope.
    firing.rs      Accuracy, shooting, reloading, recoil.
    knife.rs       Slashes and stabs.
    throw.rs       Throwing the grenade.
    view_model.rs  The gun on your screen and in other players' hands.
    icons.rs       HUD icons, rendered from the gun models at startup.
    fx.rs          Tracers, sparks, bullet holes.
    blast.rs       Grenades in flight and explosions, as everyone sees them.

  avatar/        Other players' bodies.
    rig.rs         The skeleton: eleven bones and their boxes.
    ik.rs          Two-bone inverse kinematics.
    anim.rs        Turns "moving at this speed, crouched, aiming up" into a pose.
    ragdoll.rs     Physics corpses.

  world/         The map.
    arena.rs       The layout. Edit this to change the map.
    builder.rs     block, trim, ramp, crate_stack, barrel, rock, bush, pine, pillar;
                   merges static meshes per area (finish).
    lights.rs      Lamps, the campfire, embers and fireflies.
    dummy.rs       How practice dummies look and react.

  hud/           The heads-up display.
    mod.rs         The theme and the layout: every element is spawned here.
    crosshair.rs   Accuracy crosshair, reload bar, scope.
    panels.rs      Health, weapon and ammo, slots, status, scoreboard.
    feed.rs        The kill feed.
  sound/         Sound effects, synthesized at startup.
  debug/         Commands for testing: screenshot, tp, look, r_* switches, and the probe
                 (state / assert / wait_until).

scripts/
  harness.sh     Run console scripts against the game; PASS or FAIL.
  check.sh       Format, lint, unit tests and every scenario.
  new-game.sh    Start a new game from this project.
tests/harness/   The scripted scenarios, and suite.txt listing what check.sh runs.
cfg/             Console scripts for players (cfg/bots.cfg).
assets/          The HUD font and its license.
```

## What happens when you shoot someone

Following one shot through the code shows how the pieces connect.

1. **`weapon/firing.rs` `fire`** (your machine). You click. It works out the direction from the
   view rig, the spray pattern and the accuracy cone, draws your tracer immediately, and sends
   a `Fire` message with the directions.
2. **`server/combat.rs` `receive_fire`** (the server). It ray casts each direction against the
   world and the hitboxes. On a hit it lowers `Health`, and on a kill it updates `Score`,
   tells the victim where to respawn (`Respawn`), and writes to the kill feed (`Feed`).
3. The server sends `ShotFx` to everyone else (so they see a tracer), `HitConfirm` to you (so
   your crosshair flashes), and `Damaged` to everyone (so the victim flinches or ragdolls).
4. **`Health` and `Score` are replicated components**, so every machine's copy updates on its
   own. `hud/panels.rs` just reads them.
5. **`avatar/mod.rs` `damaged`** (every machine). On `Damaged` it knocks the victim's upper
   body, or on a kill builds a ragdoll at the position in the message.

A knife swing takes the same road with a `Melee` message (`weapon/knife.rs`, then
`receive_melee` in `server/combat.rs`). A grenade is different because it exists for a while:

1. **`weapon/throw.rs`** winds the arm up, then sends `Throw`.
2. **`server/grenades.rs` `receive_throws`** spawns a physics body with a fuse and a
   replicated `Projectile`, so every client sees it fly (`weapon/blast.rs` gives it a model
   and moves it to the replicated position).
3. **`tick_grenades`** sets it off: everyone within range whom no wall shields takes damage
   through the same `Hits` helper the guns use (`server/combat.rs`), so kills, the feed and
   respawns follow one set of rules. Then `Exploded` goes to everyone for the flash, smoke,
   scorch mark and bang.

Two kinds of thing cross the network, both defined in `net/protocol.rs`:

- **Replicated components** (`Player`, `Pose`, `Health`, `Score`, `Loadout`, …): the server
  owns them and every client gets a copy that stays up to date.
- **Messages**: one-off events. Client to server (`MoveInput`, `Fire`, `SetName`, …) to ask
  for something; server to clients (`ShotFx`, `Damaged`, `Respawn`, …) to announce something.

## What runs where

| | Offline | Host | Client |
|---|---|---|---|
| `server/` rules | yes | yes | no |
| Your own movement (`player/`) | yes | yes | yes |
| Drawing others (`avatar/`), effects, HUD | yes | yes | yes |
| Bots | yes | yes | seen as players |

The switch is one run condition, `in_state(ClientState::Disconnected)`, read as "this machine
is not a client". When you `connect`, your local world is thrown away and replaced by the
server's (`net/session.rs`).

## Each frame, roughly

1. **PreUpdate**: input is read; the console issues at most one script line; network messages
   arrive; the server rules run in the order listed in `server/mod.rs`.
2. **Fixed update** (64 times a second): physics and the character controller.
3. **Update**: every console command handler; weapons (the ordered chain in `weapon/mod.rs`);
   animation; effects; HUD.
4. **PostUpdate**: the console prints; replication is sent.

Console commands are issued in PreUpdate so that every handler, in any module, sees them in
Update of the same frame. That is what lets a test script rely on "one line, one frame".
