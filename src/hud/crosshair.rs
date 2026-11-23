//! The crosshair, whose gap shows your real accuracy, and the scope overlay.

use bevy::prelude::*;

use crate::{
    hud::{CrossBar, CrosshairRoot, ScopeOverlay, TEXT},
    render::WorldCamera,
    weapon::{Arsenal, HitMarker},
};

/// The gap is the inaccuracy cone projected onto the screen: a bullet will
/// land somewhere between the bars (plus the spray pattern while spraying).
pub fn crosshair(
    time: Res<Time>,
    mut marker: ResMut<HitMarker>,
    arsenal: Res<Arsenal>,
    window: Single<&Window>,
    camera: Single<&Projection, With<WorldCamera>>,
    mut root: Single<&mut Visibility, With<CrosshairRoot>>,
    mut bars: Query<(&CrossBar, &mut Node, &mut BackgroundColor)>,
) {
    marker.time = (marker.time - time.delta_secs()).max(0.0);
    root.set_if_neq(if arsenal.scoped() {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    });
    let fov = match *camera {
        Projection::Perspective(p) => p.fov,
        _ => 1.4,
    };
    let half_height = window.height() / 2.0;
    let cone = arsenal.inaccuracy.to_radians().tan() / (fov / 2.0).tan() * half_height;
    let gap = cone.max(3.0);

    let hit = marker.time > 0.0;
    let color = match (hit, marker.kill) {
        (true, true) => Color::srgb(1.0, 0.25, 0.15),
        (true, false) => Color::WHITE,
        _ if arsenal.reloading > 0.0 || arsenal.draw > 0.0 => TEXT.with_alpha(0.35),
        _ => TEXT.with_alpha(0.9),
    };
    for (bar, mut node, mut bg) in &mut bars {
        let (len, thick) = (8.0, 2.0);
        let (w, h) = if bar.0.x != 0.0 {
            (len, thick)
        } else {
            (thick, len)
        };
        node.width = px(w);
        node.height = px(h);
        node.margin = UiRect {
            left: px(bar.0.x * (gap + len / 2.0) * 2.0),
            top: px(-bar.0.y * (gap + len / 2.0) * 2.0),
            ..default()
        };
        bg.0 = color;
    }
}

pub fn scope(arsenal: Res<Arsenal>, mut overlay: Single<&mut Visibility, With<ScopeOverlay>>) {
    overlay.set_if_neq(if arsenal.scoped() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
}
