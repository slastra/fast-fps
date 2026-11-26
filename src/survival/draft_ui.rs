//! The draft as players see it: three cards in the Relic style, tall like a
//! tarot deck and framed in their rarity's metal (worn stone, steel, gold
//! that shimmers). Each has a picture (the gun's HUD icon, an ammo round, an
//! emblem), what kind of card it is, its name in Cinzel, what it does, and
//! at the foot how far a gun's levels go or which combo an ammo would make.
//! Keys 1-3 take one; the taken card lifts for a moment before they go.

use bevy::prelude::*;

use super::{
    Me,
    art::CardArt,
    cards::{Ammo, Build, CARDS, Card, Combo, Effect, Rarity},
    protocol::Phase,
};
use crate::{
    hud::{HudFont, TEXT},
    net::WeaponMods,
    weapon::{ICON_ASPECT, WeaponIcons, WeaponKind},
};

/// How long the taken card stays up, lifted, after it's taken.
const TAKEN_SECS: f32 = 0.45;
const WIDTH: f32 = 178.0;
const HEIGHT: f32 = 262.0;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Taken>()
        .add_systems(PreStartup, load_font)
        .add_systems(Startup, spawn)
        .add_systems(Update, (show, shimmer).chain());
}

/// The card face's typeface.
#[derive(Resource)]
pub struct CardFont(pub Handle<Font>);

fn load_font(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let bytes = include_bytes!("../../assets/fonts/Cinzel-VF.ttf");
    commands.insert_resource(CardFont(fonts.add(Font::from_bytes(bytes.to_vec()))));
}

/// The card this player just took (its place in the offer) and when, so it
/// can stay up a moment.
#[derive(Resource, Default)]
pub struct Taken(pub Option<(usize, f32)>);

// ---- Look -----------------------------------------------------------------------

/// A rarity's colors: its tone (text, key, pips) and the two ends of its
/// frame.
fn colors(r: Rarity) -> (Color, Color, Color) {
    match r {
        Rarity::Common => (
            Color::srgb(0.73, 0.70, 0.66),
            Color::srgb(0.42, 0.40, 0.36),
            Color::srgb(0.23, 0.22, 0.18),
        ),
        Rarity::Rare => (
            Color::srgb(0.42, 0.72, 1.0),
            Color::srgb(0.23, 0.49, 0.75),
            Color::srgb(0.11, 0.23, 0.39),
        ),
        Rarity::Legendary => (
            Color::srgb(1.0, 0.77, 0.34),
            Color::srgb(1.0, 0.82, 0.48),
            Color::srgb(0.54, 0.35, 0.08),
        ),
    }
}

const RARE_TEXT: Color = Color::srgb(0.42, 0.72, 1.0);

fn frame(r: Rarity) -> BackgroundGradient {
    let (_, a, b) = colors(r);
    BackgroundGradient::from(LinearGradient::new(
        160f32.to_radians(),
        vec![ColorStop::percent(a, 0.0), ColorStop::percent(b, 100.0)],
    ))
}

/// The gold frame with a bright band sweeping across it; `t` in seconds.
fn gold(t: f32) -> BackgroundGradient {
    let sweep = (t / 3.2).fract() / 0.6; // across in 60% of the cycle, then rest
    let at = -40.0 + 180.0 * sweep.min(1.0);
    let dark = Color::srgb(0.54, 0.35, 0.08);
    let gold = Color::srgb(1.0, 0.82, 0.48);
    let shine = Color::srgb(1.0, 0.95, 0.78);
    BackgroundGradient::from(LinearGradient::new(
        110f32.to_radians(),
        vec![
            ColorStop::percent(dark, 0.0),
            ColorStop::percent(gold, (at - 25.0).clamp(1.0, 98.0)),
            ColorStop::percent(shine, at.clamp(2.0, 99.0)),
            ColorStop::percent(gold, (at + 25.0).clamp(3.0, 99.5)),
            ColorStop::percent(dark, 100.0),
        ],
    ))
}

// ---- What a card shows ----------------------------------------------------------

enum Art {
    Gun(WeaponKind),
    Image(Handle<Image>),
    /// A round emblem with these letters.
    Emblem(String),
}

enum Foot {
    None,
    Note(String),
    Combo(String),
    Pips(u8),
}

struct Face {
    tag: String,
    art: Art,
    foot: Foot,
}

fn ammo_name(a: Ammo) -> &'static str {
    match a {
        Ammo::Incendiary => "Incendiary",
        Ammo::Frost => "Frost",
        Ammo::Shock => "Shock",
        Ammo::Piercing => "Piercing",
        Ammo::Explosive => "Explosive Tip",
        Ammo::Leech => "Leech",
    }
}

fn roman(n: u8) -> &'static str {
    ["", "I", "II", "III"][n.min(3) as usize]
}

fn face(card: &Card, build: &Build, art: &CardArt) -> Face {
    match card.effect {
        Effect::Unlock(w) => Face {
            tag: "WEAPON".into(),
            art: Art::Gun(w),
            foot: Foot::Note(if w == WeaponKind::Grenade {
                "Two now, two each wave".into()
            } else {
                format!("Comes with {} rounds", WeaponMods::mag_of(None, w) * 4)
            }),
        },
        Effect::Level(w, n) => Face {
            tag: format!("{} {}", w.def().name.to_uppercase(), roman(n)),
            art: Art::Gun(w),
            foot: Foot::Pips(n),
        },
        Effect::Load(a) => Face {
            tag: "AMMO".into(),
            art: Art::Image(art.ammo(a)),
            foot: match build.ammo.as_slice() {
                // A second slot with one ammo in it: what this would make.
                [have] if build.slots == 2 => match Combo::of(*have, a) {
                    Some(combo) => {
                        Foot::Combo(format!("With your {}: {}", ammo_name(*have), combo.name()))
                    }
                    None => Foot::Note(format!("No combo with {}", ammo_name(*have))),
                },
                _ => Foot::None,
            },
        },
        Effect::TwinBelt => Face {
            tag: "AMMO SLOT".into(),
            art: Art::Image(art.twin.clone()),
            foot: match build.ammo.as_slice() {
                [have] => Foot::Combo(format!("Pair your {} for a combo", ammo_name(*have))),
                _ => Foot::None,
            },
        },
        Effect::Perk(_) => Face {
            tag: "PERK".into(),
            art: Art::Emblem(
                card.name
                    .split([' ', '-'])
                    .filter_map(|w| w.chars().next())
                    .take(2)
                    .collect(),
            ),
            foot: Foot::None,
        },
        Effect::Supplies => Face {
            tag: "REFILL".into(),
            art: Art::Image(art.supplies.clone()),
            foot: Foot::None,
        },
        Effect::Patch => Face {
            tag: "REFILL".into(),
            art: Art::Image(art.patch.clone()),
            foot: Foot::None,
        },
    }
}

// ---- The nodes --------------------------------------------------------------------

#[derive(Component)]
struct Row;

/// Card `i`, and its parts.
#[derive(Component)]
struct Frame(usize);
#[derive(Component)]
struct Key(usize);
#[derive(Component)]
struct Glow(usize);
#[derive(Component)]
struct Tag(usize);
#[derive(Component)]
struct Gun(usize);
#[derive(Component)]
struct Picture(usize);
#[derive(Component)]
struct Emblem(usize);
#[derive(Component)]
struct Letters(usize);
#[derive(Component)]
struct CardName(usize);
#[derive(Component)]
struct Grade(usize);
#[derive(Component)]
struct Words(usize);
#[derive(Component)]
struct ComboBox(usize);
#[derive(Component)]
struct ComboText(usize);
#[derive(Component)]
struct Note(usize);
#[derive(Component)]
struct Pip(usize, u8);

fn centered(font: TextFont, color: Color) -> impl Bundle {
    (
        Text::new(""),
        font,
        TextColor(color),
        TextLayout::justify(Justify::Center),
        TextShadow {
            offset: Vec2::new(0.0, 1.0),
            color: Color::srgba(0.0, 0.0, 0.0, 0.6),
        },
    )
}

fn spawn(mut commands: Commands, hud: Res<HudFont>, card_font: Res<CardFont>) {
    let serif = |size: f32| TextFont {
        font: card_font.0.clone().into(),
        font_size: size.into(),
        weight: FontWeight(700),
        ..default()
    };
    commands
        .spawn((
            Row,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                top: px(262),
                justify_content: JustifyContent::Center,
                column_gap: px(22),
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .with_children(|row| {
            for i in 0..3 {
                row.spawn((
                    Frame(i),
                    Node {
                        width: px(WIDTH),
                        height: px(HEIGHT),
                        padding: UiRect::all(px(3)),
                        border_radius: BorderRadius::all(px(12)),
                        ..default()
                    },
                    frame(Rarity::Common),
                    BoxShadow(vec![ShadowStyle {
                        color: Color::srgba(0.0, 0.0, 0.0, 0.6),
                        x_offset: px(0),
                        y_offset: px(12),
                        spread_radius: px(-6),
                        blur_radius: px(24),
                    }]),
                ))
                .with_children(|card| {
                    // The key, on the top edge.
                    card.spawn((
                        Key(i),
                        Node {
                            position_type: PositionType::Absolute,
                            top: px(-13),
                            left: px(WIDTH / 2.0 - 14.0),
                            width: px(28),
                            height: px(28),
                            border_radius: BorderRadius::all(px(14)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(TEXT),
                        ZIndex(2),
                    ))
                    .with_child((
                        Text::new(format!("{}", i + 1)),
                        hud.at(14.0, 800),
                        TextColor(Color::srgb(0.11, 0.08, 0.06)),
                    ));
                    card.spawn((
                        Node {
                            width: percent(100),
                            height: percent(100),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            border_radius: BorderRadius::all(px(9)),
                            overflow: Overflow::clip(),
                            ..default()
                        },
                        BackgroundGradient::from(LinearGradient::new(
                            LinearGradient::TO_BOTTOM,
                            vec![
                                ColorStop::percent(Color::srgb(0.114, 0.094, 0.157), 0.0),
                                ColorStop::percent(Color::srgb(0.078, 0.067, 0.11), 100.0),
                            ],
                        )),
                    ))
                    .with_children(|inner| {
                        // The picture window.
                        inner
                            .spawn((
                                Glow(i),
                                Node {
                                    width: percent(100),
                                    height: px(104),
                                    flex_shrink: 0.0,
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    border: UiRect::bottom(px(1)),
                                    ..default()
                                },
                                BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.08)),
                                BackgroundColor(Color::srgb(0.13, 0.11, 0.18)),
                                BackgroundGradient::default(),
                            ))
                            .with_children(|art| {
                                art.spawn((
                                    Node {
                                        position_type: PositionType::Absolute,
                                        top: px(8),
                                        left: px(8),
                                        padding: UiRect::axes(px(6), px(3)),
                                        border_radius: BorderRadius::all(px(3)),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
                                ))
                                .with_child((
                                    Tag(i),
                                    Text::new(""),
                                    hud.at(8.5, 700),
                                    TextColor(TEXT),
                                ));
                                art.spawn((
                                    Gun(i),
                                    Node {
                                        width: px(160),
                                        height: px(160.0 / ICON_ASPECT),
                                        display: Display::None,
                                        ..default()
                                    },
                                    ImageNode::default().with_color(TEXT),
                                ));
                                art.spawn((
                                    Picture(i),
                                    Node {
                                        width: px(76),
                                        height: px(76),
                                        display: Display::None,
                                        ..default()
                                    },
                                    ImageNode::default(),
                                ));
                                art.spawn((
                                    Emblem(i),
                                    Node {
                                        width: px(66),
                                        height: px(66),
                                        border: UiRect::all(px(2)),
                                        border_radius: BorderRadius::all(px(33)),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        display: Display::None,
                                        ..default()
                                    },
                                    BorderColor::all(TEXT),
                                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.3)),
                                ))
                                .with_child((
                                    Letters(i),
                                    Text::new(""),
                                    serif(26.0),
                                    TextColor(TEXT),
                                ));
                            });
                        inner.spawn((
                            CardName(i),
                            centered(serif(18.0), TEXT),
                            Node {
                                margin: UiRect::new(px(8), px(8), px(12), px(3)),
                                ..default()
                            },
                        ));
                        inner.spawn((Grade(i), centered(hud.at(8.5, 700), TEXT)));
                        inner.spawn((
                            Words(i),
                            centered(hud.at(12.0, 500), TEXT.with_alpha(0.85)),
                            Node {
                                margin: UiRect::new(px(12), px(12), px(9), px(0)),
                                ..default()
                            },
                        ));
                        // The foot: a combo, level pips, or a note.
                        inner
                            .spawn(Node {
                                width: percent(100),
                                padding: UiRect::axes(px(10), px(0)),
                                margin: UiRect::new(px(0), px(0), Val::Auto, px(11)),
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::Center,
                                ..default()
                            })
                            .with_children(|foot| {
                                foot.spawn((
                                    ComboBox(i),
                                    Node {
                                        width: percent(100),
                                        justify_content: JustifyContent::Center,
                                        padding: UiRect::axes(px(7), px(5)),
                                        border_radius: BorderRadius::all(px(5)),
                                        display: Display::None,
                                        ..default()
                                    },
                                    BackgroundColor(RARE_TEXT.with_alpha(0.12)),
                                ))
                                .with_child((ComboText(i), centered(hud.at(10.5, 600), RARE_TEXT)));
                                foot.spawn((
                                    Note(i),
                                    centered(hud.at(10.5, 600), TEXT.with_alpha(0.6)),
                                ));
                                foot.spawn(Node {
                                    column_gap: px(5),
                                    ..default()
                                })
                                .with_children(|pips| {
                                    for n in 1..=3 {
                                        pips.spawn((
                                            Pip(i, n),
                                            Node {
                                                width: px(10),
                                                height: px(10),
                                                border_radius: BorderRadius::all(px(2)),
                                                display: Display::None,
                                                ..default()
                                            },
                                            BackgroundColor(TEXT),
                                        ));
                                    }
                                });
                            });
                    });
                });
            }
        });
}

fn set(text: &mut Text, to: &str) {
    if text.0 != to {
        text.0 = to.to_string();
    }
}

fn shown(node: &mut Node, on: bool) {
    let want = if on { Display::Flex } else { Display::None };
    if node.display != want {
        node.display = want;
    }
}

/// Fill the cards from the offer, and keep a taken card up for a moment.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn show(
    time: Res<Time>,
    me: Res<Me>,
    taken: Res<Taken>,
    icons: Option<Res<WeaponIcons>>,
    art: Option<Res<CardArt>>,
    mut last: Local<Vec<u8>>,
    mut row: Single<&mut Visibility, With<Row>>,
    mut frames: Query<(&Frame, &mut Node, &mut BackgroundGradient, &mut Visibility), Without<Row>>,
    mut parts: ParamSet<(
        Query<(&Key, &mut BackgroundColor)>,
        Query<(&Glow, &mut BackgroundGradient), Without<Frame>>,
        Query<(
            AnyOf<(&Tag, &CardName, &Grade, &Words, &ComboText, &Note, &Letters)>,
            &mut Text,
            &mut TextColor,
        )>,
        Query<
            (
                AnyOf<(&Gun, &Picture, &Emblem, &ComboBox)>,
                &mut Node,
                Option<&mut ImageNode>,
                Option<&mut BorderColor>,
            ),
            Without<Frame>,
        >,
        Query<(&Pip, &mut Node, &mut BackgroundColor), Without<Frame>>,
    )>,
) {
    let (Some(icons), Some(art)) = (icons, art) else {
        return;
    };
    let now = time.elapsed_secs();
    let choosing =
        me.run.as_ref().is_some_and(|r| r.phase == Phase::Breather) && !me.offer.is_empty();
    let lifted = taken
        .0
        .filter(|(_, at)| now - at < TAKEN_SECS && !last.is_empty());
    if choosing {
        if *last != me.offer {
            *last = me.offer.clone();
        }
    } else if lifted.is_none() {
        last.clear();
    }
    row.set_if_neq(if last.is_empty() {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    });
    if last.is_empty() {
        return;
    }
    let cards: Vec<&Card> = last.iter().map(|&id| &CARDS[id as usize]).collect();
    let faces: Vec<Face> = cards.iter().map(|c| face(c, &me.build, &art)).collect();

    for (f, mut node, mut gradient, mut visibility) in &mut frames {
        let i = f.0;
        let Some(card) = cards.get(i) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        // While the taken card is up, the others are gone and it lifts.
        let up = lifted.is_some_and(|(n, _)| n == i);
        let gone = lifted.is_some() && !up;
        visibility.set_if_neq(if gone {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
        let top = if up { px(-16) } else { px(0) };
        if node.top != top {
            node.top = top;
        }
        if card.rarity != Rarity::Legendary {
            let want = frame(card.rarity);
            if *gradient != want {
                *gradient = want;
            }
        }
    }
    for (key, mut bg) in &mut parts.p0() {
        if let Some(card) = cards.get(key.0) {
            bg.set_if_neq(BackgroundColor(colors(card.rarity).0));
        }
    }
    for (glow, mut gradient) in &mut parts.p1() {
        let Some(card) = cards.get(glow.0) else {
            continue;
        };
        let tone = colors(card.rarity).0;
        let want = BackgroundGradient::from(RadialGradient::new(
            UiPosition::CENTER,
            RadialGradientShape::FarthestSide,
            vec![
                ColorStop::percent(tone.with_alpha(0.4), 0.0),
                ColorStop::percent(tone.with_alpha(0.12), 55.0),
                ColorStop::percent(tone.with_alpha(0.0), 100.0),
            ],
        ));
        if *gradient != want {
            *gradient = want;
        }
    }
    for (which, mut text, mut color) in &mut parts.p2() {
        let (tag, name, grade, words, combo, note, letters) = which;
        let i = [
            tag.map(|t| t.0),
            name.map(|t| t.0),
            grade.map(|t| t.0),
            words.map(|t| t.0),
            combo.map(|t| t.0),
            note.map(|t| t.0),
            letters.map(|t| t.0),
        ]
        .into_iter()
        .flatten()
        .next()
        .unwrap_or(0);
        let (Some(card), Some(f)) = (cards.get(i), faces.get(i)) else {
            continue;
        };
        let tone = colors(card.rarity).0;
        if tag.is_some() {
            set(&mut text, &f.tag);
            color.set_if_neq(TextColor(tone));
        } else if name.is_some() {
            set(&mut text, card.name);
        } else if grade.is_some() {
            let grade = match card.rarity {
                Rarity::Common => "COMMON",
                Rarity::Rare => "RARE",
                Rarity::Legendary => "LEGENDARY",
            };
            set(&mut text, grade);
            color.set_if_neq(TextColor(tone));
        } else if words.is_some() {
            set(&mut text, card.text);
        } else if combo.is_some() {
            set(&mut text, if let Foot::Combo(s) = &f.foot { s } else { "" });
        } else if note.is_some() {
            set(&mut text, if let Foot::Note(s) = &f.foot { s } else { "" });
        } else if letters.is_some() {
            set(&mut text, if let Art::Emblem(s) = &f.art { s } else { "" });
            color.set_if_neq(TextColor(tone));
        }
    }
    for (which, mut node, image, border) in &mut parts.p3() {
        let (gun, picture, emblem, combo) = which;
        let i = gun
            .map(|g| g.0)
            .or(picture.map(|p| p.0))
            .or(emblem.map(|e| e.0))
            .or(combo.map(|c| c.0))
            .unwrap_or(0);
        let (Some(card), Some(f)) = (cards.get(i), faces.get(i)) else {
            continue;
        };
        if gun.is_some() {
            let on = matches!(f.art, Art::Gun(_));
            shown(&mut node, on);
            if let (Art::Gun(w), Some(mut image)) = (&f.art, image) {
                let handle = icons.get(*w);
                if image.image != handle {
                    image.image = handle;
                }
            }
        } else if picture.is_some() {
            shown(&mut node, matches!(f.art, Art::Image(_)));
            if let (Art::Image(handle), Some(mut image)) = (&f.art, image)
                && image.image != *handle
            {
                image.image = handle.clone();
            }
        } else if emblem.is_some() {
            shown(&mut node, matches!(f.art, Art::Emblem(_)));
            if let Some(mut border) = border {
                let want = BorderColor::all(colors(card.rarity).0);
                if *border != want {
                    *border = want;
                }
            }
        } else if combo.is_some() {
            shown(&mut node, matches!(f.foot, Foot::Combo(_)));
        }
    }
    for (pip, mut node, mut bg) in &mut parts.p4() {
        let (Some(card), Some(f)) = (cards.get(pip.0), faces.get(pip.0)) else {
            continue;
        };
        let level = if let Foot::Pips(n) = f.foot { n } else { 0 };
        shown(&mut node, level > 0);
        let tone = colors(card.rarity).0;
        let color = if pip.1 <= level {
            tone
        } else {
            Color::srgba(1.0, 1.0, 1.0, 0.18)
        };
        bg.set_if_neq(BackgroundColor(color));
    }
}

/// A legendary card's gold frame shimmers.
fn shimmer(time: Res<Time>, me: Res<Me>, mut frames: Query<(&Frame, &mut BackgroundGradient)>) {
    let t = time.elapsed_secs();
    for (f, mut gradient) in &mut frames {
        let legendary = me
            .offer
            .get(f.0)
            .is_some_and(|&id| CARDS[id as usize].rarity == Rarity::Legendary);
        if legendary {
            *gradient = gold(t);
        }
    }
}
