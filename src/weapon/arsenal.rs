//! What you're carrying: the current weapon, ammo, and timers, plus
//! switching weapons and the sniper scope.

use bevy::{input::mouse::MouseWheel, prelude::*};
use rand::{SeedableRng, rngs::StdRng};

use crate::{
    console::{ConsoleCommand, Print},
    net::SelectWeapon,
    player::GameInput,
    sound::{Sounds, play},
    weapon::defs::{WeaponDef, WeaponKind},
};

/// Set by `r_viewmodel 0` to keep your own gun off screen.
#[derive(Resource, Default)]
pub struct HideViewModel(pub bool);

/// Shots queued by the `fire` console command (for scripted tests).
#[derive(Resource, Default)]
pub struct ScriptedFire(pub u32);

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

#[derive(Resource)]
pub struct Arsenal {
    pub current: WeaponKind,
    last: WeaponKind,
    pub ammo: [u32; 5],
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
            rng: StdRng::seed_from_u64(1),
        }
    }
}

impl Arsenal {
    pub fn def(&self) -> &'static WeaponDef {
        self.current.def()
    }

    pub fn scoped(&self) -> bool {
        self.zoom > 0
    }

    fn select(&mut self, kind: WeaponKind) -> bool {
        if kind == self.current {
            return false;
        }
        self.last = self.current;
        self.current = kind;
        self.draw = kind.def().draw_secs;
        self.reloading = 0.0;
        self.zoom = 0;
        self.spray = 0.0;
        self.kick = 0.0;
        true
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
            Some(kind) => {
                if arsenal.select(kind) {
                    select.write(SelectWeapon(kind));
                }
            }
            None => {
                print.write(Print::error("usage: weapon <ak|mp5|glock|shotgun|sniper>"));
            }
        }
    }
}

pub fn switch_weapon(
    input: Res<GameInput>,
    keys: Res<ButtonInput<KeyCode>>,
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
    let slots = [
        (KeyCode::Digit1, WeaponKind::Ak),
        (KeyCode::Digit2, WeaponKind::Mp5),
        (KeyCode::Digit3, WeaponKind::Glock),
        (KeyCode::Digit4, WeaponKind::Shotgun),
        (KeyCode::Digit5, WeaponKind::Sniper),
    ];
    let mut wanted = slots
        .iter()
        .find(|(key, _)| keys.just_pressed(*key))
        .map(|(_, kind)| *kind);
    if keys.just_pressed(KeyCode::KeyQ) {
        wanted = Some(arsenal.last);
    }
    if scroll != 0.0 {
        let i = arsenal.current.index() as i32 + if scroll < 0.0 { 1 } else { -1 };
        wanted = Some(WeaponKind::ALL[i.rem_euclid(5) as usize]);
    }
    if let Some(kind) = wanted
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
