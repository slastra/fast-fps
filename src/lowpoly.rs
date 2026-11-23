//! The low-poly look: flat-shaded meshes with color baked into each face,
//! and a small kit of shapes built from code.
//!
//! Two ideas carry most of the style:
//! - **Every face has its own shade.** [`facet`] gives each triangle a
//!   slightly different tint, so flat surfaces read as hand-made.
//! - **Edges are beveled and a little lighter.** [`block`] makes a box whose
//!   edges catch the light, which is the difference between "programmer
//!   boxes" and something that looks built.

use std::f32::consts::TAU;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};
use rand::{Rng, rngs::StdRng};

// ---------------------------------------------------------------------------
// Coloring
// ---------------------------------------------------------------------------

/// Flat-shade a mesh and give each triangle its own shade of `color`.
pub fn facet(mesh: Mesh, color: Color, jitter: f32, rng: &mut StdRng) -> Mesh {
    facet_with(mesh, rng, |_, _, rng| shade(color, jitter, rng))
}

/// Like [`facet`], but pick each face's color from its center and normal.
pub fn facet_with(
    mut mesh: Mesh,
    rng: &mut StdRng,
    mut color: impl FnMut(Vec3, Vec3, &mut StdRng) -> [f32; 4],
) -> Mesh {
    mesh.duplicate_vertices();
    mesh.compute_flat_normals();
    // Keep only what every kit piece has, so any two can be merged.
    mesh.remove_attribute(Mesh::ATTRIBUTE_UV_0);
    let positions = positions(&mesh);
    let mut colors = Vec::with_capacity(positions.len());
    for tri in positions.as_chunks::<3>().0 {
        let centroid = (tri[0] + tri[1] + tri[2]) / 3.0;
        let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero();
        let c = color(centroid, normal, rng);
        colors.extend([c; 3]);
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

pub fn shade(color: Color, jitter: f32, rng: &mut StdRng) -> [f32; 4] {
    let k = if jitter > 0.0 {
        1.0 + rng.random_range(-jitter..=jitter)
    } else {
        1.0
    };
    scaled(color, k)
}

/// `color` with its brightness multiplied by `k`.
pub fn scaled(color: Color, k: f32) -> [f32; 4] {
    let c = color.to_linear();
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

/// Smooth value noise in `[-1, 1]`, for patches of color on the ground.
pub fn noise2(x: f32, z: f32, seed: u32) -> f32 {
    let (x0, z0) = (x.floor(), z.floor());
    let (fx, fz) = (x - x0, z - z0);
    let (ux, uz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let corner = |dx: f32, dz: f32| hash3(Vec3::new(x0 + dx, 0.0, z0 + dz), seed);
    let a = corner(0.0, 0.0) + (corner(1.0, 0.0) - corner(0.0, 0.0)) * ux;
    let b = corner(0.0, 1.0) + (corner(1.0, 1.0) - corner(0.0, 1.0)) * ux;
    a + (b - a) * uz
}

// ---------------------------------------------------------------------------
// Building meshes from triangles
// ---------------------------------------------------------------------------

/// A mesh from loose triangles. Winding doesn't matter for convex shapes
/// around `center`: each triangle is flipped to face away from it.
fn soup(triangles: &[[Vec3; 3]], center: Vec3) -> Mesh {
    let mut out: Vec<[f32; 3]> = Vec::with_capacity(triangles.len() * 3);
    for [a, b, c] in triangles {
        let normal = (*b - *a).cross(*c - *a);
        let outward = (*a + *b + *c) / 3.0 - center;
        if normal.dot(outward) >= 0.0 {
            out.extend([a.to_array(), b.to_array(), c.to_array()]);
        } else {
            out.extend([a.to_array(), c.to_array(), b.to_array()]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, out)
    .with_computed_flat_normals()
}

/// Combine meshes into one. They must have the same attributes, which
/// anything that has been through [`facet`] does.
pub fn merge(parts: impl IntoIterator<Item = Mesh>) -> Mesh {
    let mut parts = parts.into_iter();
    let mut out = parts.next().expect("merge needs at least one mesh");
    for part in parts {
        out.merge(&part).expect("meshes to merge must match");
    }
    out
}

/// A box with chamfered edges, `bevel` wide.
pub fn beveled_box(size: Vec3, bevel: f32) -> Mesh {
    let h = size / 2.0;
    let b = bevel.min(h.min_element() * 0.9).max(0.0);
    // A point on the face across `axis`, in the corner given by `signs`.
    let point = |axis: usize, signs: [f32; 3]| {
        let mut p = [0.0; 3];
        for i in 0..3 {
            p[i] = signs[i] * if i == axis { h[i] } else { h[i] - b };
        }
        Vec3::from(p)
    };
    let mut tris: Vec<[Vec3; 3]> = Vec::with_capacity(44);
    let mut quad = |p: [Vec3; 4]| {
        tris.push([p[0], p[1], p[2]]);
        tris.push([p[0], p[2], p[3]]);
    };
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        let signs = |along: f32, su: f32, sv: f32| {
            let mut s = [0.0; 3];
            s[axis] = along;
            s[u] = su;
            s[v] = sv;
            s
        };
        // The two flat faces across this axis.
        for s in [-1.0, 1.0] {
            quad([
                point(axis, signs(s, 1.0, 1.0)),
                point(axis, signs(s, -1.0, 1.0)),
                point(axis, signs(s, -1.0, -1.0)),
                point(axis, signs(s, 1.0, -1.0)),
            ]);
        }
        // The four chamfers running along this axis.
        for su in [-1.0, 1.0] {
            for sv in [-1.0, 1.0] {
                quad([
                    point(u, signs(1.0, su, sv)),
                    point(u, signs(-1.0, su, sv)),
                    point(v, signs(-1.0, su, sv)),
                    point(v, signs(1.0, su, sv)),
                ]);
            }
        }
    }
    // The eight clipped corners.
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            for sz in [-1.0, 1.0] {
                let signs = [sx, sy, sz];
                tris.push([point(0, signs), point(1, signs), point(2, signs)]);
            }
        }
    }
    soup(&tris, Vec3::ZERO)
}

/// The standard building block: a beveled box whose chamfered edges are a
/// little lighter than its faces, as if worn.
pub fn block(size: Vec3, color: Color, jitter: f32, rng: &mut StdRng) -> Mesh {
    let bevel = (size.min_element() * 0.09).clamp(0.004, 0.07);
    facet_with(beveled_box(size, bevel), rng, |_, normal, rng| {
        let on_face = normal.abs().max_element() > 0.99;
        let k = if on_face { 1.0 } else { 1.22 };
        let mut c = shade(color, jitter, rng);
        for v in &mut c[..3] {
            *v *= k;
        }
        c
    })
}

/// A wooden crate: a box set inside a frame of beams.
pub fn crate_box(size: f32, color: Color, rng: &mut StdRng) -> Mesh {
    let beam = size * 0.11;
    let inner = size - beam * 0.6;
    let mut parts = vec![block(Vec3::splat(inner), color, 0.1, rng)];
    let frame = color.darker(0.07);
    let edge = (size - beam) / 2.0;
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        let mut dims = Vec3::splat(beam);
        dims[axis] = size;
        for su in [-1.0, 1.0] {
            for sv in [-1.0, 1.0] {
                let mut at = Vec3::ZERO;
                at[u] = su * edge;
                at[v] = sv * edge;
                parts.push(block(dims, frame, 0.08, rng).translated_by(at));
            }
        }
    }
    merge(parts)
}

/// A barrel standing on the ground at the origin.
pub fn barrel(radius: f32, height: f32, rng: &mut StdRng) -> Mesh {
    let wood = Color::srgb(0.36, 0.22, 0.12);
    let iron = Color::srgb(0.1, 0.1, 0.11);
    let mut body = Cylinder::new(radius, height)
        .mesh()
        .resolution(10)
        .segments(2)
        .build();
    // Bulge the middle.
    warp(&mut body, |v| {
        let bulge = 1.0 + 0.1 * (1.0 - (v.y / (height / 2.0)).powi(2));
        Vec3::new(v.x * bulge, v.y, v.z * bulge)
    });
    let mut parts = vec![facet(body, wood, 0.12, rng)];
    for y in [-0.3, 0.3] {
        let band = Cylinder::new(radius * 1.09, height * 0.07)
            .mesh()
            .resolution(10)
            .build()
            .translated_by(Vec3::Y * y * height);
        parts.push(facet(band, iron, 0.05, rng));
    }
    merge(parts).translated_by(Vec3::Y * height / 2.0)
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

/// Paint a rock: stone, with moss where it faces the sky.
pub fn mossy(mesh: Mesh, stone: Color, rng: &mut StdRng) -> Mesh {
    let moss = Color::srgb(0.2, 0.27, 0.13);
    facet_with(mesh, rng, |_, normal, rng| {
        let top = ((normal.y - 0.45) / 0.4).clamp(0.0, 1.0);
        shade(stone.mix(&moss, top * 0.8), 0.16, rng)
    })
}

/// A bush: a few squashed blobs, sitting on the ground at the origin.
pub fn bush(size: f32, rng: &mut StdRng) -> Mesh {
    let dark = Color::srgb(0.08, 0.2, 0.11);
    let light = Color::srgb(0.17, 0.32, 0.14);
    let blobs = rng.random_range(2..=4);
    let mut parts = Vec::new();
    for i in 0..blobs {
        let r = size * rng.random_range(0.45..0.75);
        let angle = i as f32 / blobs as f32 * TAU + rng.random_range(0.0..1.0);
        let at = Vec3::new(angle.cos(), 0.0, angle.sin()) * size * 0.4 + Vec3::Y * r * 0.55;
        let mesh = rock(Vec3::new(r, r * 0.75, r), rng.random()).translated_by(at);
        parts.push(facet_with(mesh, rng, |_, normal, rng| {
            // Lighter on top, where the light lands.
            let lit = ((normal.y + 0.2) / 1.2).clamp(0.0, 1.0);
            shade(dark.mix(&light, lit), 0.14, rng)
        }));
    }
    merge(parts)
}

/// A pine: a tapered trunk and tiers of drooping, slightly ragged cones.
pub fn pine(height: f32, rng: &mut StdRng) -> Mesh {
    let trunk_h = height * 0.3;
    let trunk = ConicalFrustum {
        radius_top: height * 0.022,
        radius_bottom: height * 0.04,
        height: trunk_h,
    }
    .mesh()
    .resolution(6)
    .build()
    .translated_by(Vec3::Y * trunk_h * 0.5);
    let mut parts = vec![facet(trunk, Color::srgb(0.24, 0.15, 0.09), 0.12, rng)];

    let deep = Color::srgb(0.04, 0.12, 0.09);
    let tip = Color::srgb(0.1, 0.24, 0.14);
    let tiers = 5;
    for i in 0..tiers {
        let t = i as f32 / (tiers - 1) as f32;
        let radius = height * (0.27 - t * 0.2);
        let cone_h = height * (0.3 - t * 0.08);
        let base = trunk_h * 0.62 + t * (height - trunk_h * 0.62 - cone_h * 0.55);
        let yaw = rng.random_range(0.0..TAU);
        let seed = rng.random::<u32>();
        let mut cone = Cone {
            radius,
            height: cone_h,
        }
        .mesh()
        .resolution(8)
        .build()
        .rotated_by(Quat::from_rotation_y(yaw));
        // Ragged, drooping skirt: push the rim around and pull it down.
        warp(&mut cone, |v| {
            let rim = ((cone_h * 0.5 - v.y) / cone_h).clamp(0.0, 1.0);
            let wobble = Vec3::new(hash3(v, seed), 0.0, hash3(v, seed + 9)) * 0.1 * radius;
            v + (wobble - Vec3::Y * 0.12 * cone_h) * rim * rim
        });
        let cone = cone.translated_by(Vec3::Y * (base + cone_h * 0.5));
        parts.push(facet_with(cone, rng, |c, _, rng| {
            // Lighter toward the tip of each tier.
            let up = ((c.y - base) / cone_h).clamp(0.0, 1.0);
            shade(deep.mix(&tip, up), 0.2, rng)
        }));
    }
    merge(parts)
}

/// Grass tufts scattered at `spots` (x, z), merged into one mesh. Each tuft
/// is a few thin spikes; none of it has collision.
pub fn grass(spots: &[Vec2], rng: &mut StdRng) -> Mesh {
    let dark = Color::srgb(0.1, 0.2, 0.09);
    let light = Color::srgb(0.24, 0.36, 0.14);
    let mut tris: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    for spot in spots {
        let blades = rng.random_range(3..=5);
        for _ in 0..blades {
            let angle = rng.random_range(0.0..TAU);
            let lean = Vec3::new(angle.cos(), 0.0, angle.sin());
            let h = rng.random_range(0.14..0.36);
            let w = rng.random_range(0.02..0.04);
            let root = Vec3::new(spot.x, 0.0, spot.y) + lean * rng.random_range(0.0..0.1);
            let apex = root + Vec3::Y * h + lean * h * rng.random_range(0.2..0.6);
            // A three-sided spike, so it shows from every direction.
            let side = lean.cross(Vec3::Y);
            let base = [
                root + lean * w,
                root - lean * w * 0.5 - side * w,
                root - lean * w * 0.5 + side * w,
            ];
            let color = dark.mix(&light, rng.random::<f32>());
            for i in 0..3 {
                let (a, b) = (base[i], base[(i + 1) % 3]);
                tris.extend([a.to_array(), b.to_array(), apex.to_array()]);
                // Darker at the root, lighter at the tip.
                let tip = scaled(color, 0.85 + 0.15 * i as f32);
                colors.extend([scaled(color, 0.5), scaled(color, 0.5), tip]);
            }
        }
    }
    // Normals point straight up so a tuft is lit like the ground it grows
    // from, instead of going dark on the side facing away from the light.
    let normals = vec![[0.0, 1.0, 0.0]; tris.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, tris)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
}

/// A square grid of `cells` x `cells` quads, `size` meters across, with heights
/// from `height`. Returned un-faceted so a collider can be built from it.
pub fn grid(size: f32, cells: u32, mut height: impl FnMut(f32, f32) -> f32) -> Mesh {
    let n = cells + 1;
    let step = size / cells as f32;
    let half = size / 2.0;
    let mut positions = Vec::with_capacity((n * n) as usize);
    for z in 0..n {
        for x in 0..n {
            let px = x as f32 * step - half;
            let pz = z as f32 * step - half;
            positions.push([px, height(px, pz), pz]);
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
    .with_inserted_indices(Indices::U32(indices))
    .with_computed_normals()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn beveled_box_fills_its_bounds_and_faces_outward() {
        let size = Vec3::new(2.0, 1.0, 0.5);
        let mesh = beveled_box(size, 0.05);
        let points = positions(&mesh);
        assert_eq!(points.len(), 44 * 3);
        let max = points.iter().fold(Vec3::ZERO, |m, p| m.max(p.abs()));
        assert!(max.distance(size / 2.0) < 1e-5);
        for tri in points.as_chunks::<3>().0 {
            let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]);
            let center = (tri[0] + tri[1] + tri[2]) / 3.0;
            assert!(normal.dot(center) > 0.0, "a face points inward");
        }
    }

    /// Everything that gets merged has to carry the same attributes.
    #[test]
    fn kit_pieces_build_and_merge() {
        let mut rng = StdRng::seed_from_u64(1);
        let base = block(Vec3::ONE, Color::WHITE, 0.1, &mut rng);
        for mesh in [
            crate_box(1.2, Color::WHITE, &mut rng),
            barrel(0.4, 1.0, &mut rng),
            bush(0.8, &mut rng),
            pine(7.0, &mut rng),
            grass(&[Vec2::ZERO, Vec2::ONE], &mut rng),
        ] {
            assert!(mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some());
            let mut combined = base.clone();
            combined.merge(&mesh).unwrap();
        }
    }
}
