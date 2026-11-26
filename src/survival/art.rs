//! Pictures for the cards that aren't guns (guns use the HUD's own icons):
//! a round of each special ammo, two rounds for the Twin Belt, an ammo box
//! for Supplies and a medkit for Patch Up. Drawn in code at startup, like
//! everything else in the game, from a few shapes on a 64-unit square,
//! smoothed by sampling each pixel 16 times.

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use super::cards::Ammo;

/// Pixels a side.
const SIZE: u32 = 128;

#[derive(Resource)]
pub struct CardArt {
    /// One per `Ammo`, in its order.
    pub ammo: [Handle<Image>; 6],
    pub twin: Handle<Image>,
    pub supplies: Handle<Image>,
    pub patch: Handle<Image>,
}

impl CardArt {
    pub fn ammo(&self, ammo: Ammo) -> Handle<Image> {
        self.ammo[ammo as usize].clone()
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(PreStartup, make_art);
}

fn make_art(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let mut add = |shapes: Vec<Shape>| images.add(paint(&shapes));
    let ammo = ALL_AMMO.map(|a| add(round(a, Vec2::ZERO, 1.0)));
    let mut pair = round(Ammo::Incendiary, Vec2::new(-9.0, 2.0), 0.85);
    pair.extend(round(Ammo::Frost, Vec2::new(9.0, -2.0), 0.85));
    let twin = add(pair);
    commands.insert_resource(CardArt {
        ammo,
        twin,
        supplies: add(ammo_box()),
        patch: add(medkit()),
    });
}

const ALL_AMMO: [Ammo; 6] = [
    Ammo::Incendiary,
    Ammo::Frost,
    Ammo::Shock,
    Ammo::Piercing,
    Ammo::Explosive,
    Ammo::Leech,
];

/// The color of an ammo type's tip, also used for its card.
pub fn tint(ammo: Ammo) -> Color {
    match ammo {
        Ammo::Incendiary => Color::srgb(1.0, 0.48, 0.18),
        Ammo::Frost => Color::srgb(0.56, 0.83, 1.0),
        Ammo::Shock => Color::srgb(1.0, 0.88, 0.29),
        Ammo::Piercing => Color::srgb(0.85, 0.86, 0.9),
        Ammo::Explosive => Color::srgb(1.0, 0.29, 0.23),
        Ammo::Leech => Color::srgb(0.75, 0.15, 0.23),
    }
}

// ---- Shapes -----------------------------------------------------------------

/// Something to fill, in 64-unit coordinates with y down, and its color.
enum Shape {
    Rect(Vec2, Vec2, Color),
    Circle(Vec2, f32, Color),
    /// A line from one point to another, this thick.
    Line(Vec2, Vec2, f32, Color),
    Polygon(Vec<Vec2>, Color),
    /// A rectangle shaded left to right, like a brass case.
    Brass(Vec2, Vec2),
}

impl Shape {
    /// The color at `p`, if the shape covers it.
    fn at(&self, p: Vec2) -> Option<[f32; 4]> {
        let rgba = |c: &Color| c.to_srgba().to_f32_array();
        match self {
            Shape::Rect(a, b, c) => (p.cmpge(*a).all() && p.cmple(*b).all()).then(|| rgba(c)),
            Shape::Circle(o, r, c) => (p.distance(*o) <= *r).then(|| rgba(c)),
            Shape::Line(a, b, w, c) => {
                let ab = *b - *a;
                let t = ((p - *a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                (p.distance(*a + ab * t) <= w / 2.0).then(|| rgba(c))
            }
            Shape::Polygon(points, c) => inside(points, p).then(|| rgba(c)),
            Shape::Brass(a, b) => {
                if !(p.cmpge(*a).all() && p.cmple(*b).all()) {
                    return None;
                }
                // Bright down the middle, darker at the edges.
                let t = (p.x - a.x) / (b.x - a.x);
                let k = 0.72 + 0.42 * (1.0 - (2.0 * t - 1.0).abs()).powf(0.7);
                Some([0.80 * k, 0.58 * k, 0.22 * k, 1.0])
            }
        }
    }
}

/// Even-odd point-in-polygon.
fn inside(points: &[Vec2], p: Vec2) -> bool {
    let mut odd = false;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let (a, b) = (points[i], points[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            odd = !odd;
        }
        j = i;
    }
    odd
}

/// Fill the shapes in order (later ones on top) into an image.
fn paint(shapes: &[Shape]) -> Image {
    const SAMPLES: u32 = 4;
    let mut data = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    let unit = 64.0 / SIZE as f32;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut sum = [0.0f32; 4];
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let p = Vec2::new(
                        (x as f32 + (sx as f32 + 0.5) / SAMPLES as f32) * unit,
                        (y as f32 + (sy as f32 + 0.5) / SAMPLES as f32) * unit,
                    );
                    if let Some(c) = shapes.iter().rev().find_map(|s| s.at(p)) {
                        // Premultiply, so edges blend into transparency.
                        for k in 0..3 {
                            sum[k] += c[k] * c[3];
                        }
                        sum[3] += c[3];
                    }
                }
            }
            let n = (SAMPLES * SAMPLES) as f32;
            let alpha = sum[3] / n;
            for k in 0..3 {
                let v = if sum[3] > 0.0 { sum[k] / sum[3] } else { 0.0 };
                data.push((v.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            data.push((alpha * 255.0).round() as u8);
        }
    }
    Image::new(
        Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

// ---- Pictures -----------------------------------------------------------------

/// A round of `ammo`: brass case and rim, a tip in its color with a
/// highlight, and its mark on the case. Moved by `at`, scaled by `k`.
fn round(ammo: Ammo, at: Vec2, k: f32) -> Vec<Shape> {
    let t = |x: f32, y: f32| at + Vec2::new(32.0 + (x - 32.0) * k, 34.0 + (y - 34.0) * k);
    let tip = tint(ammo);
    // The ogive: two curves up to the point.
    let mut ogive = Vec::new();
    for i in 0..=12 {
        let s = i as f32 / 12.0;
        ogive.push(t(
            22.0 + 10.0 * (1.0 - (1.0 - s).powi(2)),
            30.0 - 26.0 * s.powf(1.6),
        ));
    }
    for i in (0..=12).rev() {
        let s = i as f32 / 12.0;
        ogive.push(t(
            42.0 - 10.0 * (1.0 - (1.0 - s).powi(2)),
            30.0 - 26.0 * s.powf(1.6),
        ));
    }
    let pale = Color::srgba(1.0, 0.97, 0.9, 0.92);
    let mut out = vec![
        Shape::Brass(t(22.0, 30.0), t(42.0, 57.0)),
        Shape::Rect(t(20.0, 54.0), t(44.0, 59.0), Color::srgb(0.55, 0.39, 0.13)),
        Shape::Polygon(ogive, tip),
        Shape::Line(
            t(26.0, 27.0),
            t(29.5, 12.0),
            2.0 * k,
            Color::srgba(1.0, 1.0, 1.0, 0.5),
        ),
    ];
    let c = |x: f32, y: f32| t(x, y);
    out.extend(match ammo {
        Ammo::Incendiary => vec![
            Shape::Circle(c(32.0, 48.0), 4.5 * k, tip),
            Shape::Polygon(vec![c(27.6, 47.0), c(32.0, 36.0), c(36.4, 47.0)], tip),
            Shape::Circle(c(32.0, 49.0), 2.0 * k, Color::srgb(1.0, 0.9, 0.5)),
        ],
        Ammo::Frost => (0..3)
            .map(|i| {
                let a = i as f32 * std::f32::consts::PI / 3.0;
                let d = Vec2::from_angle(a) * 7.0;
                Shape::Line(
                    c(32.0 - d.x, 44.0 - d.y),
                    c(32.0 + d.x, 44.0 + d.y),
                    2.2 * k,
                    pale,
                )
            })
            .collect(),
        Ammo::Shock => vec![
            Shape::Line(c(35.0, 35.0), c(29.0, 44.0), 2.6 * k, tip),
            Shape::Line(c(29.0, 44.0), c(35.0, 44.0), 2.6 * k, tip),
            Shape::Line(c(35.0, 44.0), c(29.0, 53.0), 2.6 * k, tip),
        ],
        Ammo::Piercing => vec![
            Shape::Line(c(32.0, 53.0), c(32.0, 36.0), 2.4 * k, pale),
            Shape::Line(c(27.0, 41.0), c(32.0, 36.0), 2.4 * k, pale),
            Shape::Line(c(37.0, 41.0), c(32.0, 36.0), 2.4 * k, pale),
        ],
        Ammo::Explosive => {
            let mut v: Vec<Shape> = (0..8)
                .map(|i| {
                    let d = Vec2::from_angle(i as f32 * std::f32::consts::FRAC_PI_4);
                    Shape::Line(
                        c(32.0, 45.0) + d * 4.0 * k,
                        c(32.0, 45.0) + d * 8.0 * k,
                        1.8 * k,
                        tip,
                    )
                })
                .collect();
            v.push(Shape::Circle(c(32.0, 45.0), 3.2 * k, tip));
            v
        }
        Ammo::Leech => vec![
            Shape::Circle(c(32.0, 48.0), 4.5 * k, tip),
            Shape::Polygon(vec![c(27.8, 46.5), c(32.0, 37.0), c(36.2, 46.5)], tip),
        ],
    });
    out
}

/// An olive ammo box with a yellow band, rounds showing at the top.
fn ammo_box() -> Vec<Shape> {
    let olive = Color::srgb(0.33, 0.38, 0.2);
    let mut out = vec![
        Shape::Rect(Vec2::new(10.0, 26.0), Vec2::new(54.0, 56.0), olive),
        Shape::Rect(
            Vec2::new(10.0, 26.0),
            Vec2::new(54.0, 31.0),
            Color::srgb(0.25, 0.29, 0.15),
        ),
        Shape::Rect(
            Vec2::new(10.0, 38.0),
            Vec2::new(54.0, 44.0),
            Color::srgb(0.88, 0.7, 0.2),
        ),
        Shape::Rect(
            Vec2::new(28.0, 47.0),
            Vec2::new(36.0, 51.0),
            Color::srgb(0.2, 0.22, 0.12),
        ),
    ];
    for i in 0..5 {
        let x = 15.0 + i as f32 * 8.5;
        out.push(Shape::Brass(Vec2::new(x, 14.0), Vec2::new(x + 5.0, 26.0)));
        out.push(Shape::Circle(
            Vec2::new(x + 2.5, 14.0),
            2.5,
            Color::srgb(0.7, 0.45, 0.2),
        ));
    }
    out
}

/// A white medkit with a red cross.
fn medkit() -> Vec<Shape> {
    let white = Color::srgb(0.93, 0.92, 0.89);
    let red = Color::srgb(0.85, 0.14, 0.12);
    vec![
        Shape::Rect(
            Vec2::new(24.0, 14.0),
            Vec2::new(40.0, 22.0),
            Color::srgb(0.6, 0.6, 0.58),
        ),
        Shape::Rect(Vec2::new(10.0, 20.0), Vec2::new(54.0, 54.0), white),
        Shape::Rect(Vec2::new(28.0, 26.0), Vec2::new(36.0, 48.0), red),
        Shape::Rect(Vec2::new(21.0, 33.0), Vec2::new(43.0, 41.0), red),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_round_is_drawn_inside_a_clear_square() {
        let image = paint(&round(Ammo::Shock, Vec2::ZERO, 1.0));
        let data = image.data.as_ref().unwrap();
        let alpha = |x: u32, y: u32| data[((y * SIZE + x) * 4 + 3) as usize];
        // Clear in the corners, solid in the middle of the case.
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(SIZE - 1, SIZE - 1), 0);
        assert_eq!(alpha(SIZE / 2, SIZE * 45 / 64), 255);
    }

    #[test]
    fn polygons_know_inside_from_out() {
        let square = [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y];
        assert!(inside(&square, Vec2::splat(0.5)));
        assert!(!inside(&square, Vec2::splat(1.5)));
    }
}
