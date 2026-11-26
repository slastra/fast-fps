# Recipes

Step-by-step instructions for common changes. Each ends with how to check it worked.
`docs/tour.md` explains where things are and why; `docs/harness.md` explains the tests.

## Tune a number

Health, run speed, jump height, gravity, ports, how long corpses last: `src/config.rs`.
Weapon numbers: `src/weapon/defs.rs`. Change the value and run the game.

Check: `scripts/check.sh --quick`. Movement and recoil *feel* can only be judged by playing.

## Add a console command

1. Register it in the `plugin` function of the module it belongs to:
   ```rust
   app.console_command("heal", "[amount]", "restore your health");
   ```
2. Handle it in a system in the same module, and add that system to `Update`:
   ```rust
   fn heal_command(mut cmds: MessageReader<ConsoleCommand>, mut print: MessageWriter<Print>) {
       for cmd in cmds.read() {
           if cmd.name != "heal" {
               continue;
           }
           let Some(amount) = cmd.arg(0).map_or(Some(100.0), |a| a.parse::<f32>().ok()) else {
               print.write(Print::error("usage: heal [amount]"));
               continue;
           };
           // ...do it...
       }
   }
   ```
3. Report bad input with `Print::error`, so a script that misuses it fails.

A command runs on the machine where it was typed. If it should change the game for everyone
(health, score, spawning), it has to ask the server: see "Send something to the server".

Check: a script that runs the command and asserts the result.

## Add a weapon

Weapons are a fixed list. The compiler will point at most places you miss. Each has a
`class`: `Gun` (bullets, the steps below), `Knife` or `Grenade`. A new melee weapon or
throwable needs its own input system beside `weapon/knife.rs` or `weapon/throw.rs` and a
server handler like `receive_melee` (`server/combat.rs`) or `server/grenades.rs`, and should
deal damage through `Hits` so kills and the feed work like every other weapon.

1. **`src/weapon/defs.rs`**
   - add a variant to `enum WeaponKind`
   - add it to `ALL`, in the same position, and bump `COUNT`
   - add a line to `parse` (its names, and its slot number)
   - add a `WeaponDef` to `DEFS`, **in the same position as in the enum**, with
     `class: Class::Gun`
   - optionally give it a spray pattern (a list of `(shot, x degrees, y degrees)` keyframes)
   - `delay` is the time from the click to the shot (the flintlock's spark); 0 for most guns
2. **`src/weapon/models.rs`**: add its shape to `model` (boxes and cylinders, a muzzle point,
   and where it rests on screen).
3. **`src/sound/mod.rs`**: add its shot sound to the `shots` array, in the same position
   (make one as in "Add a sound" below).
4. **`src/avatar/anim.rs`**: add it to `hold` (how other players hold it).
5. **`src/avatar/ragdoll.rs`**: add it to `knock` (how hard it throws a body).
6. Update the usage text of the `weapon` and `bot_do` commands (search for `sniper>`).

Number keys, the HUD slots and the scroll wheel pick it up from `ALL` automatically, and its
HUD icon is rendered from the model you made in step 2.

Check: `cargo test` (the animation tests cover every weapon), then add it to
`tests/harness/spray.cfg` and look at the bullet holes.

## Change the map

The map is `spawn_level` in `src/world/arena.rs`. It is built from helpers on the `Builder` in
`src/world/builder.rs`; each one adds a mesh *and* a matching collider:

| Helper | |
|---|---|
| `b.block(center, size, rotation, color, jitter)` | A beveled box you can stand on. `jitter` is how much each face's shade varies. |
| `b.trim(center, size, color)` | A decorative box with **no** collision: ledges, beams, caps, banners. |
| `b.ramp(low, high, width)` | A walkable ramp between two points on its surface. |
| `b.crate_stack(base, count)`, `b.barrel(at)` | Cover. |
| `b.rock(at, size)`, `b.pine(at, height, collide)`, `b.pillar(at, height)` | Scenery with collision. |
| `b.bush(at, size)` | Scenery without collision. |
| `lamp(&mut b, at, lantern)`, `campfire(&mut b, at, flame)` | Lights (`world/lights.rs`). The two lamps nearest the camera cast shadows. |

Blocks, crates, barrels, rocks, pillars and pines remember the ground they take up, and
crates, barrels, rocks and pines won't be placed where something already is. So order matters:
build structures first, then scatter things around them. If you add something solid with
`b.solid` directly, add its `Footprint` to `b.taken` too.

The builder doesn't spawn a separate entity per piece. It queues meshes by area and merges
them when `b.finish()` runs at the end of `spawn_level`, because the renderer pays for every
separate mesh in every view it draws. Colliders stay separate. If something must stay its own
entity (it moves, or you want to find it in a query), use `b.single` and give it its own
collider.

The outer wall has twelve bricked-up breaches (`BREACHES` in `arena.rs`): each segment is built
as two piers and a lintel around a hole, filled by a plug carrying a `Breach` marker. Survival
opens them as its cracks (`survival/cracks.rs`). Moving a breach moves a crack: keep them on a
segment's center, and check the crack's route still reaches the keep.

New shapes go in `src/lowpoly.rs`: build a mesh, color it with `facet` or `block`, and add a
builder helper that spawns it with a collider.

Y is up, the ground is at 0, and a meter is a meter (a player is 1.8 tall). Also in
`arena.rs`: `SPAWNS` (where players appear; Y 1.2 drops them onto the ground), `DUMMIES`, and
`ARENA` (half the width of the walled square).

The builder has two random number generators. Use `b.rng` for anything with collision: every
machine builds the map itself, and they must all build the same one or shots will hit walls
that aren't there for someone else. Use `b.paint` for shading and decoration, so cosmetic
changes never move the map. Leave the yard south of the keep empty (`in_yard`); the test
scenarios use it as a stage.

Check: a script that sets `sky day`, teleports to a few viewpoints and takes screenshots
(`tests/harness/look.cfg` is a starting point). Make sure no spawn point is inside something (`state x y z` after spawning).
Then `scripts/bench.sh` before and after, since a map change can change the frame rate (see
`docs/perf.md`).

## Change the sky or the mood

Each time of day is one `Look` in `SkyKind::look` in `src/sky.rs`: sky colors from ground to
zenith, where the sun or moon sits, the main light, the fill light, fog and exposure. Change
the numbers and compare with `sky next` in the console.

To add a time of day, add a variant to `SkyKind`, to `ALL` and `name`, and a `Look` for it.

Three things to know. Sky colors are shown as written, while the light on the world is scaled
by `ev100` (higher is darker), so a bright sky over a dark world means the exposure or the
light is off, not the sky. Fog should sit close to the horizon color, so the far forest melts
into the sky. And leave color grading contrast alone: it crushes dark scenes to black.

Check: `scripts/harness.sh -s 1600x900 tests/harness/look.cfg` and look at the screenshots.

## Add a sound

1. Add a prompt to `SOUNDS` in `scripts/sounds.py`: what it sounds like, the longest it may
   run, and whether to keep only the first sound when the model makes several.
2. Run `scripts/sounds.py gen <name> -n 3` for three takes (needs `OPENROUTER_API_KEY`; a take
   costs a cent or two). Listen to them in `target/sounds/`, then `scripts/sounds.py use <name>
   <take>` trims the one you like, levels it and writes `assets/sounds/<name>.wav`.
3. Add a field to `Sounds` in `src/sound/mod.rs` and load it in `make_sounds` with
   `load(recorded!("<name>"))`. Keep sounds short; they are built into the binary.
4. Play it with `play(&mut commands, &sounds.your_sound, volume)`.

The model puts follow-up noises (a pump, a bolt) seconds after the main sound, so make those
on their own and mix them in: see `pump` and `bolt` in `scripts/sounds.py`.

Check: by ear. The harness can't hear.

## Sync a new value to every player

Example: every player has a `Coins(u32)` everyone can see.

1. Define it in `src/net/protocol.rs`:
   ```rust
   #[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default)]
   pub struct Coins(pub u32);
   ```
2. Add `.replicate::<Coins>()` in `register`, in the same file.
3. Put it on players where the server spawns them: `spawn_player` in
   `src/server/players.rs` (and bots, in `src/server/bots.rs`).
4. Change it only in `src/server/`. Every machine can read it with an ordinary query.

Check: publish it to the probe and assert it from a client in a LAN script.

## Send something to the server

Example: a client asks to buy something.

1. Define the message in `src/net/protocol.rs`, and register it in `register`:
   ```rust
   #[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
   pub struct Buy(pub u32);
   // in register:
   .add_client_message::<Buy>(Channel::Ordered)
   ```
2. Send it from the client with a `MessageWriter<Buy>`. This also works offline and on the
   host: the message is delivered locally.
3. Receive it in `src/server/` with `MessageReader<FromClient<Buy>>`, and add that system to
   the chain in `src/server/mod.rs` so it only runs with authority. `find_player` in
   `server/players.rs` turns the sender into their player entity. See `receive_suicide` for a
   complete small example.
4. Never trust the contents: check ranges, and check the sender can actually do it.

For the other direction (server tells clients), use `add_server_message` and write a
`ToClients { targets, message }`. `Feed` is a small example.

## Make something checkable in tests

Publish it to the probe from any system:

```rust
fn publish(mut probe: ResMut<Probe>, coins: Query<&Coins>) {
    probe.set("coins", coins.iter().map(|c| c.0).sum::<u32>());
}
```

Then scripts can `assert coins == 3` and `wait_until coins >= 1 5`. Built-in values are in
`collect` in `src/debug/probe.rs`; add to it for anything that belongs there.

## Add a game mode

Duel (`src/server/duel.rs`, `src/duel.rs`) is the small worked example; survival
(`src/server/survival/`, `src/survival/`, [survival.md](survival.md)) is the big one. Keep a
mode in its own modules: the rest of the game should never ask which mode is on. Where the
core can't do what a mode needs, add a general hook to the core instead, as below.

1. **Register it.** `app.game_mode("name")` makes `mode name` switch to it. The `mode`
   command resets everyone (scores, health, `Kit`, `WeaponMods`, `Out`...) and the `Rules` to
   their defaults, then sends `ModeChange { to }`. Run the mode's systems
   `.after(ModeSwitch)`.
2. **State everyone can see.** One replicated component on its own entity (`Duel`,
   `Survival`), spawned when `ModeChange` names the mode and despawned when it names another.
   Its existence is the mode: run the mode's systems with
   `run_if(any_with_component::<Survival>)`. Put its wire types in their own `register`
   called at the end of `net::protocol::register`, as `survival::protocol` is.
3. **Rules on the server**, in `src/server/<mode>/`. Keep the round logic a plain function of
   the state (`duel::step`, `survival::waves::step`) so it can be unit tested without the
   game. The hooks:
   - `Rules`: respawns, friendly fire, the practice dummies, player ragdolls.
   - `Died { victim, killer }`: what a kill means (the duel puts the loser `Out`; survival
     puts them on their knees). `Unstuck` is the `kill` command when respawns are off.
   - `Out` (not drawn, can't be hurt or fire), `Disarmed` (can't fire), `Untouchable`.
   - `Kit` (which weapons a player owns, finite ammo) and `WeaponMods` (damage, magazine,
     reload, pierce, pellets, full auto, move speed...), both replicated so the player's own
     game follows them.
   - `Mob`, plus `Health`, `Pose` and `hitbox()`: an enemy that isn't a player. Weapons hurt it
     like anything else; when killed it gets `Dead` and is cleared away. Its look comes from
     `app.mob_look(kind, ...)` on the client.
   - `HitLanded` and `Boom`: react to every hit and blast without touching the weapons.
     `blast()` is the grenade's area damage, for a mode's own explosions.
4. **What players see**, in `src/<mode>/` or `src/<mode>.rs`, reading the replicated state:
   banners, marks, sounds. Limits on the local player are set fresh every frame in
   `PreUpdate`: `MoveLimits` (speed, jumping, kneeling view, pose flags others see),
   `Arsenal::locked` (the trigger), `MenuKeys` (1-3 for a menu, not weapons).

Publish the mode's state to the probe (`duel`, `survival`, `wave`...) so a scenario can follow
it, as `tests/harness/duel.cfg` and `survival.cfg` do. Add the name to `MODES` in
`lobby/src/main.rs` and to `create` in `src/net/lobby.rs` to offer it online.

## Add a bot order

`bot_commands` in `src/server/bots.rs` has one `match` over order names. Add an arm that
changes a field on `Bot`, and act on that field in `drive`, which builds the bot's `Pose` each
frame exactly as a real client would report it. Add the name to the command's help text.

## Add to the HUD

Every HUD element is spawned in `spawn_hud` in `src/hud/mod.rs` with a marker component, and
updated by a small system in `src/hud/panels.rs` that finds it by that marker. Copy
`HealthText` and `vitals` as a pattern. Use the `text` helper and the theme colors at the top
of `hud/mod.rs` so new elements match. Sizes are in pixels at 720p and scale with the window.

Check: `tests/harness/hud.cfg` shows every HUD state; add yours to it.

## Add a scenario to the test suite

Write `tests/harness/yours.cfg` (it must end with `quit`), run it with `scripts/harness.sh`,
and add a line to `tests/harness/suite.txt`. Each line there is the arguments for one
`harness.sh` run.

## Start a new game from this one

```sh
scripts/new-game.sh ~/Projects/Rust/space-arena
```

The copy has its own name in the window title, settings folder and network protocol, so it
can't join a game of the original. Then make it yours: `src/config.rs`, `src/weapon/defs.rs`,
`src/world/arena.rs` and `src/sky.rs` are the files that change a game's character the most.
