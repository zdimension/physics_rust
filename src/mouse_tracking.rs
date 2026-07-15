use std::{fmt::Display, ops::Deref};

use bevy::{ecs::system::EntityCommand, prelude::*, window::PrimaryWindow};

pub mod prelude {
    pub use crate::mouse_tracking::{InitMouseTracking, InitWorldTracking, MousePosPlugin};
}

pub struct MousePosPlugin;

impl Plugin for MousePosPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MousePos::default())
            .insert_resource(MousePosWorld::default())
            .add_systems(
                Update,
                update_mouse_positions.after(crate::mouse::wheel::CameraZoomSet),
            );
    }
}

#[derive(Debug, Default, Resource, Clone, Copy, PartialEq)]
pub struct MousePos(Vec2);

impl Deref for MousePos {
    type Target = Vec2;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for MousePos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Default, Resource, Clone, Copy, PartialEq)]
pub struct MousePosWorld(Vec3);

impl Deref for MousePosWorld {
    type Target = Vec3;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for MousePosWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Component, Default)]
struct CameraMousePos(Vec2);

#[derive(Component, Default)]
struct CameraMousePosWorld(Vec3);

pub struct InitMouseTracking;

impl EntityCommand for InitMouseTracking {
    type Out = ();

    fn apply(self, mut entity: EntityWorldMut) -> Self::Out {
        entity.insert(CameraMousePos::default());
    }
}

pub struct InitWorldTracking;

impl EntityCommand for InitWorldTracking {
    type Out = ();

    fn apply(self, mut entity: EntityWorldMut) -> Self::Out {
        entity.insert((CameraMousePos::default(), CameraMousePosWorld::default()));
    }
}

#[derive(Component)]
pub struct MainCamera;

fn update_mouse_positions(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut mouse_positions: ParamSet<(
        (ResMut<MousePos>, ResMut<MousePosWorld>),
        Query<(
            &Camera,
            &GlobalTransform,
            &mut CameraMousePos,
            Option<&mut CameraMousePosWorld>,
            Option<&MainCamera>,
        )>,
    )>,
) {
    let Some(cursor_pos) = windows.iter().next().and_then(Window::cursor_position) else {
        return;
    };

    let mut main_screen = None;
    let mut main_world = None;

    for (camera, transform, mut screen, world, main) in mouse_positions.p1().iter_mut() {
        screen.0 = cursor_pos;
        let world_pos = camera
            .viewport_to_world_2d(transform, cursor_pos)
            .map(|pos| pos.extend(0.0))
            .unwrap_or_default();

        if let Some(mut world) = world {
            world.0 = world_pos;
        }

        if main.is_some() {
            main_screen = Some(cursor_pos);
            main_world = Some(world_pos);
        }
    }

    let (mut screen_res, mut world_res) = mouse_positions.p0();
    if let Some(pos) = main_screen {
        screen_res.0 = pos;
    }
    if let Some(pos) = main_world {
        world_res.0 = pos;
    }
}