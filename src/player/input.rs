//! Key bindings, mouse capture, and whether gameplay input is live
//! (it isn't while the console is open or the mouse is released).

use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};
use bevy_ahoy::prelude::*;
use bevy_enhanced_input::prelude::{Press, *};

use crate::{console::Console, platform, player::LocalPlayer};

pub(super) fn plugin(app: &mut App) {
    app.add_input_context::<PlayerInput>()
        .init_resource::<GameInput>()
        .add_systems(
            PreUpdate,
            (update_game_input, grab_cursor)
                .chain()
                .after(bevy::input::InputSystems),
        );
}

/// Whether gameplay input should reach the character this frame.
#[derive(Resource, Default)]
pub struct GameInput {
    pub enabled: bool,
}

/// Left Ctrl crouches (C does too), except in a browser: there Ctrl+W, with
/// W held to move, closes the tab, and a page can't stop that. So C alone.
const CROUCH_KEY: KeyCode = if cfg!(target_arch = "wasm32") {
    KeyCode::KeyC
} else {
    KeyCode::ControlLeft
};

#[derive(Component, Default)]
#[component(on_add = PlayerInput::on_add)]
pub struct PlayerInput;

impl PlayerInput {
    fn on_add(mut world: DeferredWorld, ctx: HookContext) {
        world.commands().entity(ctx.entity).apply_scene(bsn! {
            Actions<PlayerInput> [
                Action<Movement>::new()
                DeadZone
                Bindings [
                    {preset::cardinal::wasd_keys()}
                    --
                    {preset::axial::left_stick()}
                ]
                --
                Action<Jump>::new()
                Press
                Bindings [
                    Binding(KeyCode::Space)
                    --
                    Binding(GamepadButton::South)
                ]
                --
                Action<Crane>::new()
                Press
                Bindings [
                    Binding(KeyCode::Space)
                    --
                    Binding(GamepadButton::South)
                ]
                --
                Action<Mantle>::new()
                ~Hold::new(0.2)
                Bindings [
                    Binding(KeyCode::Space)
                    --
                    Binding(GamepadButton::South)
                ]
                --
                Action<Crouch>::new()
                Bindings [
                    Binding(CROUCH_KEY)
                    --
                    Binding(KeyCode::KeyC)
                    --
                    Binding(GamepadButton::LeftTrigger2)
                ]
                --
                Action<RotateCamera>::new()
                Bindings [
                    Binding::mouse_motion()
                    ~Scale::splat(0.07)
                    --
                    {preset::axial::new(
                        (GamepadAxis::RightStickY, Scale::splat(4.0), DeadZone::default()),
                        (GamepadAxis::RightStickX, Scale::splat(4.0), DeadZone::default())
                    )}
                ]
            ]
        });
    }
}

pub fn update_game_input(
    console: Res<Console>,
    cursor: Single<&CursorOptions>,
    mut input: ResMut<GameInput>,
    mut commands: Commands,
    player: Single<Entity, With<LocalPlayer>>,
) {
    let enabled = !console.open && cursor.grab_mode == CursorGrabMode::Locked;
    if enabled != input.enabled {
        input.enabled = enabled;
        commands
            .entity(*player)
            .insert(ContextActivity::<PlayerInput>::new(enabled));
    }
}

/// Click to grab the mouse, Esc or the console releases it.
pub fn grab_cursor(
    console: Res<Console>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: Single<&mut CursorOptions>,
    window: Single<&Window>,
    mut page_had_it: Local<bool>,
) {
    // A browser lets go of the mouse on Esc by itself, without telling the
    // game. Catch up: once the page has held the mouse and no longer does,
    // it's released. (Right after a click the page doesn't hold it yet.)
    if let Some(held) = platform::pointer_locked() {
        let had = std::mem::replace(&mut *page_had_it, held);
        if had && !held && cursor.grab_mode == CursorGrabMode::Locked {
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
            return;
        }
    }
    let locked = cursor.grab_mode == CursorGrabMode::Locked;
    let release = console.open || keys.just_pressed(KeyCode::Escape) || !window.focused;
    if locked && release {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    } else if !locked
        && !console.open
        // Closing the console takes the mouse back too, except in a browser,
        // which only lets a page take the mouse right after a click.
        && (mouse.just_pressed(MouseButton::Left)
            || (console.is_changed() && !cfg!(target_arch = "wasm32")))
    {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}
