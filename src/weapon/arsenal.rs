//! What you're carrying: the current weapon, ammo, and timers, plus
//! switching weapons and the sniper scope.

use bevy::{input::mouse::MouseWheel, prelude::*};
use rand::{SeedableRng, rngs::StdRng};

use crate::{
    config::DUEL_LOW_DEG,
    console::{ConsoleCommand, Print},
    net::{Kit, LocalId, Out, Player, SelectWeapon, WeaponMods},
    player::GameInput,
    sound::{Sounds, play},
    weapon::defs::{WeaponDef, WeaponKind},
};

/// Set by `r_viewmodel 0` to keep your own gun off screen.
#[derive(Resource, Default)]
pub struct HideViewModel(pub bool);

/// Clicks queued by the `fire` and `fire_alt` console commands (for scripted
/// tests): left and right mouse button. `fire_hold` holds a button down for a
/// number of seconds instead (charging a grenade).
#[derive(Resource, Default)]
pub struct ScriptedFire {
    pub primary: u32,
    pub alt: u32,
    pub hold: f32,
    pub hold_alt: f32,
}

/// The knife or arm motion in progress, for the view model.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Swing {
    #[default]
    None,
    /// A slash across, alternating sides: +1 sweeps to the left, -1 to the right.
    Slash(f32),
    Stab,
    /// Winding up to throw (the grenade still in hand), then the follow-through.
    Throw,
}

/// Set by a server hit confirmation; the HUD fades it out.
#[derive(Resource, Default)]
pub struct HitMarker {
    pub time: f32,
    pub kill: bool,
}

/// Scales `tan(fov / 2)`: 1.0 unzoomed, smaller when scoped. The view
/// settings in `player.rs` apply it to the camera and the mouse.
#[derive(Resource)]
pub struct Zoom(pub f32);

impl Default for Zoom {
    fn default() -> Self {
        Self(1.0)
    }
}

impl PartialEq for Zoom {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

const SLOT_KEYS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

#[derive(Resource)]
pub struct Arsenal {
    pub current: WeaponKind,
    last: WeaponKind,
    pub ammo: [u32; WeaponKind::COUNT],
    /// Seconds of reload left for the current weapon.
    pub reloading: f32,
    /// Seconds until a freshly drawn weapon can fire.
    pub draw: f32,
    pub cooldown: f32,
    /// Position in the spray pattern (fractional while recovering).
    pub spray: f32,
    pub since_shot: f32,
    /// Decaying punch from single-shot weapons, degrees.
    pub kick_punch: Vec2,
    /// Current inaccuracy cone half-angle, degrees. The crosshair shows this.
    pub inaccuracy: f32,
    /// Zoom level index; 0 is unscoped.
    pub zoom: usize,
    pub buffered_click: f32,
    pub kick: f32,
    pub flash: f32,
    pub bob: f32,
    pub since_pump: f32,
    /// The current swing and how far through it is, 0 to 1.
    pub swing: Swing,
    pub swing_t: f32,
    /// A grenade being charged: seconds the button has been held, and
    /// whether it is a lob (right button).
    pub charge: Option<(f32, bool)>,
    /// A flintlock shot on its way: seconds until the spark lights the
    /// powder and it fires.
    pub priming: Option<f32>,
    /// The trigger does nothing this frame. Cleared at the start of every
    /// frame; a game mode sets it in `PreUpdate` while it wants it (a duel
    /// before "Draw!", a downed player in survival).
    pub locked: bool,
    /// This player is out of play (`Out`), so their gun isn't shown.
    pub out: bool,
    /// What a game mode lets this player use (`Kit`) and how it changed
    /// their weapons (`WeaponMods`), copied from their player by
    /// [`local_kit`]. Without a mode: everything, unlimited, unchanged.
    pub owned: u16,
    pub kit: Option<Kit>,
    pub mods: WeaponMods,
    /// Rounds this machine has fired, per weapon (wrapping), to compare with
    /// `Kit::spent`.
    pub fired: [u16; WeaponKind::COUNT],
    /// How far the gun is out of the holster, 0 to 1. Always 1 outside a
    /// duel; a duelist starts each round at 0 and draws with Space after
    /// "Draw!" (`duel.rs`).
    pub raised: f32,
    /// A duelist has drawn this round, so the pistol is coming up.
    pub drawing: bool,
    pub rng: StdRng,
}

impl Default for Arsenal {
    fn default() -> Self {
        Self {
            current: WeaponKind::Ak,
            last: WeaponKind::Glock,
            ammo: WeaponKind::ALL.map(|k| k.def().mag),
            reloading: 0.0,
            draw: 0.0,
            cooldown: 0.0,
            spray: 0.0,
            since_shot: 10.0,
            kick_punch: Vec2::ZERO,
            inaccuracy: 0.0,
            zoom: 0,
            buffered_click: 0.0,
            kick: 0.0,
            flash: 0.0,
            bob: 0.0,
            since_pump: 10.0,
            swing: Swing::None,
            swing_t: 1.0,
            charge: None,
            priming: None,
            locked: false,
            out: false,
            owned: ALL_WEAPONS,
            kit: None,
            mods: WeaponMods::default(),
            fired: [0; WeaponKind::COUNT],
            raised: 1.0,
            drawing: false,
            rng: StdRng::seed_from_u64(1),
        }
    }
}

/// Every weapon, as an owned mask.
const ALL_WEAPONS: u16 = (1 << WeaponKind::COUNT) - 1;

/// Shots this machine fired that the server hasn't counted yet, at most
/// this many; more means the counts belong to different kits.
const IN_FLIGHT_MAX: u16 = 200;

impl Arsenal {
    pub fn def(&self) -> &'static WeaponDef {
        self.current.def()
    }

    pub fn owns(&self, kind: WeaponKind) -> bool {
        self.owned & (1 << kind.index()) != 0
    }

    /// Magazine size, with upgrades.
    pub fn mag(&self, kind: WeaponKind) -> u32 {
        WeaponMods::mag_of(Some(&self.mods), kind)
    }

    pub fn reload_secs(&self, kind: WeaponKind) -> f32 {
        kind.def().reload_secs * self.mods.reload[kind.index()]
    }

    pub fn draw_secs(&self, kind: WeaponKind) -> f32 {
        kind.def().draw_secs * self.mods.swap
    }

    pub fn pellets(&self, kind: WeaponKind) -> u32 {
        WeaponMods::pellets_of(Some(&self.mods), kind)
    }

    pub fn automatic(&self, kind: WeaponKind) -> bool {
        kind.def().automatic || WeaponMods::has(self.mods.automatic, kind)
    }

    /// Rounds left for `kind`, magazine and spare together, counting shots
    /// the server hasn't seen yet. `None` is unlimited.
    pub fn rounds_left(&self, kind: WeaponKind) -> Option<u32> {
        let kit = self.kit.as_ref()?;
        let left = kit.left(kind)?;
        let in_flight = self.fired[kind.index()].wrapping_sub(kit.spent[kind.index()]);
        let in_flight = if in_flight > IN_FLIGHT_MAX {
            0
        } else {
            in_flight
        };
        Some(left.saturating_sub(in_flight) as u32)
    }

    /// Spare rounds for `kind`, beyond what's in the magazine. `None` is
    /// unlimited.
    pub fn spare(&self, kind: WeaponKind) -> Option<u32> {
        self.rounds_left(kind)
            .map(|left| left.saturating_sub(self.ammo[kind.index()]))
    }

    /// A round of `kind` left this machine.
    pub fn count_shot(&mut self, kind: WeaponKind) {
        let i = kind.index();
        self.fired[i] = self.fired[i].wrapping_add(1);
    }

    pub fn scoped(&self) -> bool {
        self.zoom > 0
    }

    /// Switch to `kind`; false if already holding it.
    pub fn select(&mut self, kind: WeaponKind) -> bool {
        if kind == self.current {
            return false;
        }
        self.last = self.current;
        self.current = kind;
        self.draw = self.draw_secs(kind);
        self.reloading = 0.0;
        self.zoom = 0;
        self.spray = 0.0;
        self.kick = 0.0;
        self.swing = Swing::None;
        self.swing_t = 1.0;
        self.charge = None;
        self.priming = None;
        true
    }

    /// The pistol is in its holster and hasn't been drawn.
    pub fn holstered(&self) -> bool {
        self.raised < 1.0 && !self.drawing
    }

    /// How far up the gun is, eased: 0 at the hip, 1 aimed.
    pub fn raise(&self) -> f32 {
        let t = self.raised.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// How far below the aim a shot fired now goes, in degrees: a shot taken
    /// before the pistol is all the way up hits low.
    pub fn low_deg(&self) -> f32 {
        DUEL_LOW_DEG * (1.0 - self.raise())
    }

    /// The weapon held before this one.
    pub fn last(&self) -> WeaponKind {
        self.last
    }

    /// Start a swing animation.
    pub fn start_swing(&mut self, swing: Swing) {
        self.swing = swing;
        self.swing_t = 0.0;
    }
}

pub fn weapon_commands(
    mut cmds: MessageReader<ConsoleCommand>,
    mut arsenal: ResMut<Arsenal>,
    mut select: MessageWriter<SelectWeapon>,
    mut print: MessageWriter<Print>,
) {
    for cmd in cmds.read() {
        if cmd.name == "zoom" {
            let levels = arsenal.def().zoom.len();
            arsenal.zoom = cmd
                .arg(0)
                .and_then(|z| z.parse::<usize>().ok())
                .unwrap_or(0)
                .min(levels);
            continue;
        }
        if cmd.name != "weapon" {
            continue;
        }
        match cmd.arg(0).and_then(WeaponKind::parse) {
            Some(kind) if !arsenal.owns(kind) => {
                print.write(Print(format!("you don't have the {}", kind.def().name)));
            }
            Some(kind) => {
                if arsenal.select(kind) {
                    select.write(SelectWeapon(kind));
                }
            }
            None => {
                print.write(Print::error(
                    "usage: weapon <ak|mp5|glock|shotgun|sniper|knife|grenade|flintlock>",
                ));
            }
        }
    }
}

/// Set by a game mode while it uses the number keys for something else (a
/// menu); they don't switch weapons then.
#[derive(Resource, Default)]
pub struct MenuKeys(pub bool);

pub fn switch_weapon(
    input: Res<GameInput>,
    keys: Res<ButtonInput<KeyCode>>,
    menu: Res<MenuKeys>,
    mut wheel: MessageReader<MouseWheel>,
    mut arsenal: ResMut<Arsenal>,
    mut select: MessageWriter<SelectWeapon>,
    mut commands: Commands,
    sounds: Res<Sounds>,
) {
    let scroll: f32 = wheel.read().map(|w| w.y).sum();
    if !input.enabled {
        return;
    }
    // Number keys pick weapons in the order they are listed in `WeaponKind::ALL`.
    let mut wanted = WeaponKind::ALL
        .into_iter()
        .zip(SLOT_KEYS)
        .filter(|_| !menu.0)
        .find(|(_, key)| keys.just_pressed(*key))
        .map(|(kind, _)| kind);
    if keys.just_pressed(KeyCode::KeyQ) {
        wanted = Some(arsenal.last);
    }
    if scroll != 0.0 {
        // The next weapon you own that way.
        let step = if scroll < 0.0 { 1 } else { -1 };
        let mut i = arsenal.current.index() as i32;
        for _ in 0..WeaponKind::COUNT {
            i = (i + step).rem_euclid(WeaponKind::COUNT as i32);
            if arsenal.owns(WeaponKind::ALL[i as usize]) {
                break;
            }
        }
        wanted = Some(WeaponKind::ALL[i as usize]);
    }
    if let Some(kind) = wanted
        && arsenal.owns(kind)
        && arsenal.select(kind)
    {
        select.write(SelectWeapon(kind));
        play(&mut commands, &sounds.click, 0.4);
    }
}

pub fn zoom_input(
    input: Res<GameInput>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut arsenal: ResMut<Arsenal>,
    mut zoom: ResMut<Zoom>,
    mut commands: Commands,
    sounds: Res<Sounds>,
) {
    let levels = arsenal.def().zoom;
    if input.enabled
        && mouse.just_pressed(MouseButton::Right)
        && !levels.is_empty()
        && arsenal.reloading <= 0.0
    {
        arsenal.zoom = (arsenal.zoom + 1) % (levels.len() + 1);
        play(&mut commands, &sounds.zoom, 0.35);
    }
    let target = if arsenal.zoom == 0 {
        1.0
    } else {
        levels[arsenal.zoom - 1]
    };
    zoom.set_if_neq(Zoom(target));
}

/// Copy what the game mode gave this player (their `Kit` and `WeaponMods`,
/// and whether they're `Out`) into the arsenal, and put away a weapon they
/// no longer own.
pub fn local_kit(
    local: Res<LocalId>,
    players: Query<(&Player, Option<&Kit>, Option<&WeaponMods>, Has<Out>)>,
    mut arsenal: ResMut<Arsenal>,
    mut select: MessageWriter<SelectWeapon>,
) {
    let mine = players.iter().find(|p| p.0.owner == local.0);
    let (kit, mods, out) = match mine {
        Some((_, kit, mods, out)) => (kit.cloned(), mods.cloned().unwrap_or_default(), out),
        None => (None, WeaponMods::default(), false),
    };
    if arsenal.kit != kit {
        // A new kit's counts don't match ours; start from the server's.
        if let Some(kit) = &kit {
            for i in 0..WeaponKind::COUNT {
                if arsenal.fired[i].wrapping_sub(kit.spent[i]) > IN_FLIGHT_MAX {
                    arsenal.fired[i] = kit.spent[i];
                }
            }
        }
        arsenal.owned = kit.as_ref().map_or(ALL_WEAPONS, |k| k.owned);
        arsenal.kit = kit;
    }
    if arsenal.mods != mods {
        arsenal.mods = mods;
    }
    if arsenal.out != out {
        arsenal.out = out;
    }
    // No more rounds in the magazine than it holds (a card that made it
    // bigger may be gone), or than there are left at all.
    for kind in WeaponKind::ALL {
        let i = kind.index();
        let holds = if kind.def().class == crate::weapon::Class::Gun {
            WeaponMods::mag_of(Some(&arsenal.mods), kind)
        } else {
            u32::MAX
        };
        let most = arsenal
            .rounds_left(kind)
            .map_or(holds, |left| left.min(holds));
        if arsenal.ammo[i] > most {
            arsenal.ammo[i] = most;
        }
    }
    if !arsenal.owns(arsenal.current)
        && let Some(kind) = WeaponKind::ALL.into_iter().find(|k| arsenal.owns(*k))
        && arsenal.select(kind)
    {
        select.write(SelectWeapon(kind));
    }
}
