//! The skeleton other players are drawn with: eleven bones, each a joint
//! entity with one or more boxes hanging off it.
//!
//! Positions are relative to the player entity, which sits at the collider
//! center, 0.9 m above the feet, facing -Z. Limb bones point down their local
//! -Y so the IK solver and the ragdoll hinges agree on axes.

use bevy::prelude::*;
use rand::{SeedableRng, rngs::StdRng};

use crate::{lowpoly, render, weapon};

/// Ground level relative to the player entity.
pub const FEET_Y: f32 = -0.9;
/// Upper and lower arm lengths.
pub const ARM: (f32, f32) = (0.29, 0.29);
/// Thigh and shin lengths.
pub const LEG: (f32, f32) = (0.37, 0.37);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bone {
    Hips,
    Torso,
    Head,
    UpperArmL,
    ForearmL,
    UpperArmR,
    ForearmR,
    ThighL,
    ShinL,
    ThighR,
    ShinR,
}

pub const BONE_COUNT: usize = 11;

impl Bone {
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn offset(self) -> Vec3 {
        Vec3::from(SKELETON[self.index()].offset)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Paint {
    Team,
    TeamDark,
    Dark,
    Boot,
    /// The glowing visor.
    Glow,
    /// Bone and claw.
    Pale,
    /// Dull iron.
    Metal,
}

pub struct BoxDef {
    pub size: [f32; 3],
    pub center: [f32; 3],
    pub paint: Paint,
}

/// How the ragdoll connects a bone to its parent. Angles in radians.
#[derive(Clone, Copy)]
pub enum JointKind {
    Root,
    Ball {
        swing: f32,
        twist: f32,
    },
    /// Rotation about local X between `min` and `max`.
    Hinge {
        min: f32,
        max: f32,
    },
}

pub struct BoneDef {
    pub bone: Bone,
    pub parent: Option<Bone>,
    /// Joint position in the parent's frame at rest.
    pub offset: [f32; 3],
    /// The first box is also the ragdoll collider.
    pub boxes: &'static [BoxDef],
    pub joint: JointKind,
}

const fn bx(size: [f32; 3], center: [f32; 3], paint: Paint) -> BoxDef {
    BoxDef {
        size,
        center,
        paint,
    }
}

const UPPER_ARM: &[BoxDef] = &[bx([0.12, 0.30, 0.13], [0.0, -0.14, 0.0], Paint::TeamDark)];
const FOREARM: &[BoxDef] = &[
    bx([0.10, 0.25, 0.11], [0.0, -0.115, 0.0], Paint::Team),
    bx([0.09, 0.08, 0.10], [0.0, -0.27, 0.0], Paint::Dark),
];
const THIGH: &[BoxDef] = &[bx([0.16, 0.38, 0.18], [0.0, -0.18, 0.0], Paint::Dark)];
const SHIN: &[BoxDef] = &[
    bx([0.14, 0.30, 0.16], [0.0, -0.14, 0.0], Paint::Dark),
    bx([0.15, 0.10, 0.27], [0.0, -0.32, -0.05], Paint::Boot),
];

/// Parents always come before their children.
pub const SKELETON: [BoneDef; BONE_COUNT] = [
    BoneDef {
        bone: Bone::Hips,
        parent: None,
        offset: [0.0, -0.14, 0.0],
        boxes: &[bx([0.38, 0.18, 0.24], [0.0, 0.02, 0.0], Paint::Dark)],
        joint: JointKind::Root,
    },
    BoneDef {
        bone: Bone::Torso,
        parent: Some(Bone::Hips),
        offset: [0.0, 0.10, 0.0],
        boxes: &[
            bx([0.50, 0.50, 0.28], [0.0, 0.27, 0.0], Paint::Team),
            bx([0.32, 0.30, 0.10], [0.0, 0.30, 0.18], Paint::Dark),
        ],
        joint: JointKind::Ball {
            swing: 0.5,
            twist: 0.4,
        },
    },
    BoneDef {
        bone: Bone::Head,
        parent: Some(Bone::Torso),
        offset: [0.0, 0.54, 0.0],
        boxes: &[
            bx([0.30, 0.30, 0.30], [0.0, 0.17, 0.0], Paint::TeamDark),
            bx([0.26, 0.07, 0.04], [0.0, 0.19, -0.15], Paint::Glow),
        ],
        joint: JointKind::Ball {
            swing: 0.7,
            twist: 0.8,
        },
    },
    BoneDef {
        bone: Bone::UpperArmL,
        parent: Some(Bone::Torso),
        offset: [-0.31, 0.46, 0.0],
        boxes: UPPER_ARM,
        joint: JointKind::Ball {
            swing: 1.9,
            twist: 1.4,
        },
    },
    BoneDef {
        bone: Bone::ForearmL,
        parent: Some(Bone::UpperArmL),
        offset: [0.0, -0.29, 0.0],
        boxes: FOREARM,
        joint: JointKind::Hinge { min: 0.0, max: 2.5 },
    },
    BoneDef {
        bone: Bone::UpperArmR,
        parent: Some(Bone::Torso),
        offset: [0.31, 0.46, 0.0],
        boxes: UPPER_ARM,
        joint: JointKind::Ball {
            swing: 1.9,
            twist: 1.4,
        },
    },
    BoneDef {
        bone: Bone::ForearmR,
        parent: Some(Bone::UpperArmR),
        offset: [0.0, -0.29, 0.0],
        boxes: FOREARM,
        joint: JointKind::Hinge { min: 0.0, max: 2.5 },
    },
    BoneDef {
        bone: Bone::ThighL,
        parent: Some(Bone::Hips),
        offset: [-0.10, -0.04, 0.0],
        boxes: THIGH,
        joint: JointKind::Ball {
            swing: 1.5,
            twist: 0.6,
        },
    },
    BoneDef {
        bone: Bone::ShinL,
        parent: Some(Bone::ThighL),
        offset: [0.0, -0.37, 0.0],
        boxes: SHIN,
        joint: JointKind::Hinge {
            min: -2.5,
            max: 0.0,
        },
    },
    BoneDef {
        bone: Bone::ThighR,
        parent: Some(Bone::Hips),
        offset: [0.10, -0.04, 0.0],
        boxes: THIGH,
        joint: JointKind::Ball {
            swing: 1.5,
            twist: 0.6,
        },
    },
    BoneDef {
        bone: Bone::ShinR,
        parent: Some(Bone::ThighR),
        offset: [0.0, -0.37, 0.0],
        boxes: SHIN,
        joint: JointKind::Hinge {
            min: -2.5,
            max: 0.0,
        },
    },
];

/// One drawn box, kept so a ragdoll can be built from the same meshes.
#[derive(Clone)]
pub struct RigPart {
    pub bone: Bone,
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
    pub center: Vec3,
    pub glow: bool,
}

#[derive(Component, Clone)]
pub struct AvatarRig {
    /// Parent of every bone; hide this to hide the body.
    pub root: Entity,
    pub bones: [Entity; BONE_COUNT],
    /// Holds the gun models. Positioned in `root` space by the animation.
    pub gun: Entity,
    pub parts: Vec<RigPart>,
    /// The vertex-colored material shared by body and gun.
    pub material: Handle<StandardMaterial>,
}

pub fn player_color(owner: u64) -> Color {
    let hue = if owner == 0 {
        24.0
    } else {
        (owner.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40) as f32 % 360.0
    };
    Color::hsl(hue, 0.65, 0.5)
}

/// How a body is painted and shaped: its main color, the darker trim, the
/// glow of the visor or eyes, and for a mob, parts it lacks or adds.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub body: Color,
    pub trim: Color,
    pub glow: Color,
    pub eyes: Eyes,
    /// Bones drawn without their boxes (a crawler's legs). The bones are
    /// still there, so the skeleton and ragdoll work the same.
    pub hide: &'static [Bone],
    /// Boxes added to bones (a bloater's belly).
    pub extras: &'static [Extra],
}

impl Look {
    /// Painted these colors, shaped like a player.
    pub const fn new(body: Color, trim: Color, glow: Color) -> Self {
        Self {
            body,
            trim,
            glow,
            eyes: Eyes::Visor,
            hide: &[],
            extras: &[],
        }
    }
}

/// What glows on the face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Eyes {
    /// A players' visor: one bar.
    Visor,
    /// Two close dots.
    Dots,
}

/// A box added to a bone, in the bone's frame.
#[derive(Clone, Copy, Debug)]
pub struct Extra {
    pub bone: Bone,
    pub size: [f32; 3],
    pub center: [f32; 3],
    pub paint: Paint,
}

pub const fn extra(bone: Bone, size: [f32; 3], center: [f32; 3], paint: Paint) -> Extra {
    Extra {
        bone,
        size,
        center,
        paint,
    }
}

/// Meshes and materials that bodies of one kind share (mobs of a kind all
/// look alike, so they're built once). Filled by the first body built.
#[derive(Default)]
pub struct Shared {
    meshes: Vec<Handle<Mesh>>,
    materials: Option<(Handle<StandardMaterial>, Handle<StandardMaterial>)>,
}

impl Shared {
    /// The glowing material (eyes, boils) bodies of this kind share.
    pub fn glow(&self) -> Option<&Handle<StandardMaterial>> {
        self.materials.as_ref().map(|m| &m.1)
    }
}

/// Build a player's joint hierarchy and boxes under `player`, in their
/// color, carrying every gun.
pub fn spawn(
    commands: &mut Commands,
    player: Entity,
    owner: u64,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> AvatarRig {
    let team = player_color(owner);
    let look = Look::new(team, team.darker(0.14), team);
    spawn_with(commands, player, owner, look, true, meshes, materials, None)
}

/// Build a body under `parent`: painted as `look`, its boxes shaped from
/// `seed`, with the guns or not, and sharing `shared` meshes when given.
#[allow(clippy::too_many_arguments)]
pub fn spawn_with(
    commands: &mut Commands,
    parent: Entity,
    seed: u64,
    look: Look,
    guns: bool,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    mut shared: Option<&mut Shared>,
) -> AvatarRig {
    let mut rng = StdRng::seed_from_u64(seed);
    let team = look.body;
    let made = shared.as_ref().and_then(|s| s.materials.clone());
    let (material, glow) = made.unwrap_or_else(|| {
        let material = materials.add(StandardMaterial {
            perceptual_roughness: 0.85,
            ..default()
        });
        let glow = materials.add(StandardMaterial {
            base_color: look.glow,
            emissive: look.glow.to_linear() * 8.0,
            ..default()
        });
        (material, glow)
    });
    if let Some(shared) = shared.as_mut() {
        shared.materials = Some((material.clone(), glow.clone()));
    }
    let player = parent;
    let mut box_index = 0;

    let root = commands
        .spawn((
            Name::new("Avatar"),
            Transform::default(),
            Visibility::default(),
            ChildOf(player),
        ))
        .id();

    let mut bones = [Entity::PLACEHOLDER; BONE_COUNT];
    let mut parts = Vec::new();
    for def in &SKELETON {
        let parent = def.parent.map_or(root, |p| bones[p.index()]);
        let joint = commands
            .spawn((
                Transform::from_translation(Vec3::from(def.offset)),
                Visibility::default(),
                ChildOf(parent),
            ))
            .id();
        bones[def.bone.index()] = joint;

        // This bone's boxes, as the look shapes them.
        let mut boxes: Vec<(Vec3, Vec3, Paint)> = Vec::new();
        if !look.hide.contains(&def.bone) {
            for b in def.boxes {
                let (size, center) = (Vec3::from(b.size), Vec3::from(b.center));
                if matches!(b.paint, Paint::Glow) && look.eyes == Eyes::Dots {
                    for side in [-1.0, 1.0] {
                        let dot = Vec3::new(0.065 * side, 0.01, 0.0);
                        boxes.push((Vec3::new(0.06, 0.06, 0.04), center + dot, Paint::Glow));
                    }
                } else {
                    boxes.push((size, center, b.paint));
                }
            }
        }
        for e in look.extras.iter().filter(|e| e.bone == def.bone) {
            boxes.push((Vec3::from(e.size), Vec3::from(e.center), e.paint));
        }

        for (size, center, paint) in boxes {
            let is_glow = matches!(paint, Paint::Glow);
            let reuse = shared
                .as_ref()
                .and_then(|s| s.meshes.get(box_index).cloned());
            let (mesh, mat) = if is_glow {
                let mesh = reuse.unwrap_or_else(|| meshes.add(Cuboid::from_size(size)));
                (mesh, glow.clone())
            } else {
                let color = match paint {
                    Paint::Team => team,
                    Paint::TeamDark => look.trim,
                    Paint::Dark => Color::srgb(0.09, 0.09, 0.11),
                    Paint::Boot | Paint::Glow => Color::srgb(0.04, 0.04, 0.05),
                    Paint::Pale => Color::srgb(0.81, 0.78, 0.69),
                    Paint::Metal => Color::srgb(0.26, 0.27, 0.3),
                };
                let mesh = reuse
                    .unwrap_or_else(|| meshes.add(lowpoly::block(size, color, 0.08, &mut rng)));
                (mesh, material.clone())
            };
            if let Some(shared) = shared.as_mut()
                && shared.meshes.len() == box_index
            {
                shared.meshes.push(mesh.clone());
            }
            box_index += 1;
            let mut part = commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_translation(center),
                ChildOf(joint),
            ));
            if is_glow {
                part.insert(render::no_shadow());
            }
            parts.push(RigPart {
                bone: def.bone,
                mesh,
                material: mat,
                center,
                glow: is_glow,
            });
        }
    }

    let gun = commands
        .spawn((
            Name::new("Gun mount"),
            Transform::default(),
            Visibility::default(),
            ChildOf(root),
        ))
        .id();
    if guns {
        let gun_material = MeshMaterial3d(material.clone());
        commands.entity(gun).with_children(|mount| {
            weapon::spawn_avatar_guns(mount, meshes, &gun_material, &mut rng);
        });
    }

    AvatarRig {
        root,
        bones,
        gun,
        parts,
        material,
    }
}
