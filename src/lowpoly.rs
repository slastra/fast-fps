//! Helpers for the faceted low-poly look: flat normals and a slightly
//! different shade on every face, baked into vertex colors.

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};
use rand::{Rng, rngs::StdRng};

/// Flat-shade a mesh and give each triangle its own shade of `color`.
pub fn facet(mesh: Mesh, color: Color, jitter: f32, rng: &mut StdRng) -> Mesh {
    facet_with(mesh, rng, |_, rng| shade(color, jitter, rng))
}

/// Like [`facet`], but pick each face's color from its centroid.
pub fn facet_with(
    mut mesh: Mesh,
    rng: &mut StdRng,
    mut color: impl FnMut(Vec3, &mut StdRng) -> [f32; 4],
) -> Mesh {
    mesh.duplicate_vertices();
    mesh.compute_flat_normals();
    let positions = positions(&mesh);
    let mut colors = Vec::with_capacity(positions.len());
    for tri in positions.as_chunks::<3>().0 {
        let centroid = (tri[0] + tri[1] + tri[2]) / 3.0;
        let c = color(centroid, rng);
        colors.extend([c; 3]);
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

pub fn shade(color: Color, jitter: f32, rng: &mut StdRng) -> [f32; 4] {
    let c = color.to_linear();
    let k = 1.0 + rng.random_range(-jitter..=jitter);
    [c.red * k, c.green * k, c.blue * k, 1.0]
}

pub fn positions(mesh: &Mesh) -> Vec<Vec3> {
    match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(p)) => p.iter().map(|&v| Vec3::from(v)).collect(),
        _ => Vec::new(),
    }
}

/// Move every vertex by a deterministic function of its position, so vertices
/// that share a position (UV seams) stay welded together.
pub fn warp(mesh: &mut Mesh, mut f: impl FnMut(Vec3) -> Vec3) {
    if let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for v in p.iter_mut() {
            *v = f(Vec3::from(*v)).into();
        }
    }
}

/// Cheap hash noise in `[-1, 1]` for a quantized position.
pub fn hash3(p: Vec3, seed: u32) -> f32 {
    let q = (p * 1000.0).round();
    let mut h = (q.x as i32 as u32).wrapping_mul(0x8da6_b343)
        ^ (q.y as i32 as u32).wrapping_mul(0xd816_3841)
        ^ (q.z as i32 as u32).wrapping_mul(0xcb1a_b31f)
        ^ seed.wrapping_mul(0x1656_67b1);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// A lumpy rock: a coarse icosphere pushed around and squashed.
pub fn rock(size: Vec3, seed: u32) -> Mesh {
    let mut mesh = Sphere::new(1.0).mesh().ico(1).unwrap();
    warp(&mut mesh, |v| {
        let n = Vec3::new(hash3(v, seed), hash3(v, seed + 1), hash3(v, seed + 2));
        (v + n * 0.28) * size
    });
    mesh
}

/// A pine: a hexagonal trunk with three stacked cones.
pub fn pine(height: f32, rng: &mut StdRng) -> Mesh {
    let trunk_h = height * 0.28;
    let trunk = Cylinder::new(height * 0.035, trunk_h)
        .mesh()
        .resolution(6)
        .build()
        .translated_by(Vec3::Y * trunk_h * 0.5);
    let mut mesh = facet(trunk, Color::srgb(0.24, 0.15, 0.09), 0.1, rng);
    let green = Color::srgb(0.07, 0.17, 0.11);
    for i in 0..3 {
        let t = i as f32 / 3.0;
        let radius = height * (0.27 - t * 0.07);
        let cone_h = height * 0.42;
        let y = trunk_h * 0.7 + t * height * 0.34 + cone_h * 0.5;
        let yaw = rng.random_range(0.0..std::f32::consts::TAU);
        let mut cone = Cone {
            radius,
            height: cone_h,
        }
        .mesh()
        .resolution(7)
        .build()
        .rotated_by(Quat::from_rotation_y(yaw))
        .translated_by(Vec3::Y * y);
        let seed = rng.random::<u32>();
        warp(&mut cone, |v| {
            v + Vec3::new(hash3(v, seed), 0.0, hash3(v, seed + 9)) * 0.08 * height
        });
        let cone = facet(cone, green, 0.22, rng);
        mesh.merge(&cone).unwrap();
    }
    mesh
}

/// A square grid of `cells` x `cells` quads, `size` meters across, with heights
/// from `height`. Returned un-faceted so a collider can be built from it.
pub fn grid(size: f32, cells: u32, mut height: impl FnMut(f32, f32) -> f32) -> Mesh {
    let n = cells + 1;
    let step = size / cells as f32;
    let half = size / 2.0;
    let mut positions = Vec::with_capacity((n * n) as usize);
    let mut uvs = Vec::with_capacity((n * n) as usize);
    for z in 0..n {
        for x in 0..n {
            let px = x as f32 * step - half;
            let pz = z as f32 * step - half;
            positions.push([px, height(px, pz), pz]);
            uvs.push([x as f32 / cells as f32, z as f32 / cells as f32]);
        }
    }
    let mut indices = Vec::with_capacity((cells * cells * 6) as usize);
    for z in 0..cells {
        for x in 0..cells {
            let i = z * n + x;
            // Alternate the diagonal so the faceting doesn't look like stripes.
            if (x + z) % 2 == 0 {
                indices.extend([i, i + n, i + 1, i + 1, i + n, i + n + 1]);
            } else {
                indices.extend([i, i + n, i + n + 1, i, i + n + 1, i + 1]);
            }
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
    .with_computed_normals()
}
