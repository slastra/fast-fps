//! Bodies for mobs (`Mob`): a game mode's enemies, drawn with the players'
//! skeleton. A mode gives each `kind` a look with [`MobLookAppExt::mob_look`];
//! every mob of a kind shares one set of meshes, so a crowd costs little more
//! to build than one.

use bevy::{platform::collections::HashMap, prelude::*};

use super::{
    anim::AvatarAnim,
    rig::{self, Look, Shared},
};
use crate::net::Mob;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<MobLooks>()
        .init_resource::<SharedRigs>()
        .add_observer(dress_mob);
}

/// How mobs of one kind look.
#[derive(Clone, Copy, Debug)]
pub struct MobLook {
    pub look: Look,
    /// Carry a gun (the held weapon's model), or go empty-handed.
    pub gun: bool,
}

#[derive(Resource, Default)]
pub struct MobLooks(HashMap<u8, MobLook>);

#[derive(Resource, Default)]
struct SharedRigs(HashMap<u8, Shared>);

pub trait MobLookAppExt {
    /// Draw mobs of `kind` like this.
    fn mob_look(&mut self, kind: u8, look: MobLook) -> &mut Self;
}

impl MobLookAppExt for App {
    fn mob_look(&mut self, kind: u8, look: MobLook) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<MobLooks>()
            .0
            .insert(kind, look);
        self
    }
}

fn dress_mob(
    add: On<Add<Mob>>,
    mobs: Query<&Mob>,
    looks: Res<MobLooks>,
    mut shared: ResMut<SharedRigs>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(mob) = mobs.get(add.entity) else {
        return;
    };
    let grey = Color::srgb(0.45, 0.45, 0.45);
    let look = looks.0.get(&mob.kind).copied().unwrap_or(MobLook {
        look: Look::new(grey, grey.darker(0.14), Color::WHITE),
        gun: false,
    });
    let rig = rig::spawn_with(
        &mut commands,
        add.entity,
        u64::from(mob.kind),
        look.look,
        look.gun,
        &mut meshes,
        &mut materials,
        Some(shared.0.entry(mob.kind).or_default()),
    );
    commands
        .entity(add.entity)
        .insert((rig, AvatarAnim::default()));
}
