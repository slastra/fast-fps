//! The crosshair, whose gap shows your real accuracy, the reload bar under
//! it, and the scope overlay.

use bevy::prelude::*;

use crate::{
    hud::{ACCENT, CrossBar, CrosshairRoot, DANGER, ReloadFill, ReloadTrack, ScopeOverlay, TEXT},
    render::WorldCamera,
    weapon::{Arsenal, GRENADE, HitMarker, MenuKeys},
};

/// The gap is the inaccuracy cone projected onto the screen: a bullet will
/// land somewhere between the bars (plus the spray pattern while spraying).
pub fn crosshair(
    time: Res<Time>,
    mut marker: ResMut<HitMarker>,
    arsenal: Res<Arsenal>,
    menu: Res<MenuKeys>,
    window: Single<&Window>,
    scale: Res<UiScale>,
    camera: Single<&Projection, With<WorldCamera>>,
    mut root: Single<(&mut Visibility, &mut Node), (With<CrosshairRoot>, Without<CrossBar>)>,
    mut bars: Query<(&CrossBar, &mut Node, &mut BackgroundColor)>,
) {
    marker.time = (marker.time - time.delta_secs()).max(0.0);
    let (visibility, root_node) = &mut *root;
    // Hidden while scoped, holstered, or a menu is up (it'd sit on it).
    visibility.set_if_neq(if arsenal.scoped() || arsenal.holstered() || menu.0 {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    });
    let fov = match *camera {
        Projection::Perspective(p) => p.fov,
        _ => 1.4,
    };
    // In window pixels, then into the HUD's scaled units.
    let half_height = window.height() / 2.0;
    let to_screen = |deg: f32| deg.to_radians().tan() / (fov / 2.0).tan() * half_height / scale.0;
    let gap = to_screen(arsenal.inaccuracy).max(3.0);
    // A duelist's pistol coming out of the holster: the crosshair rises from
    // where a shot would land, fading in as it comes up.
    let low = to_screen(arsenal.low_deg());
    if root_node.top != px(low) {
        root_node.top = px(low);
    }
    let fade = arsenal.raise();

    let hit = marker.time > 0.0;
    let color = match (hit, marker.kill) {
        (true, true) => DANGER,
        (true, false) => Color::WHITE,
        _ if arsenal.reloading > 0.0 || arsenal.draw > 0.0 => TEXT.with_alpha(0.35),
        _ => TEXT.with_alpha(0.92),
    };
    let color = color.with_alpha(color.alpha() * fade);
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

/// A thin bar under the crosshair that fills while you reload, or while you
/// charge a grenade throw.
pub fn reload_bar(
    arsenal: Res<Arsenal>,
    mut track: Single<&mut Visibility, With<ReloadTrack>>,
    mut fill: Single<(&mut Node, &mut BackgroundColor), With<ReloadFill>>,
) {
    if let Some((held, _)) = arsenal.charge {
        track.set_if_neq(Visibility::Inherited);
        let strength = GRENADE.strength(held);
        fill.0.width = percent(strength * 100.0);
        fill.1.0 = if strength >= 1.0 { TEXT } else { ACCENT };
        return;
    }
    let reloading = arsenal.reloading > 0.0;
    track.set_if_neq(if reloading {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    if reloading {
        let done = 1.0 - arsenal.reloading / arsenal.reload_secs(arsenal.current);
        fill.0.width = percent(done.clamp(0.0, 1.0) * 100.0);
        fill.1.0 = ACCENT;
    }
}

pub fn scope(arsenal: Res<Arsenal>, mut overlay: Single<&mut Visibility, With<ScopeOverlay>>) {
    overlay.set_if_neq(if arsenal.scoped() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
}
