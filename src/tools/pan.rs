use crate::CAMERA_Z;
use bevy::math::{Vec2, Vec3Swizzles};
use bevy::prelude::{EventReader, Query, Transform, With};
use bevy_mouse_tracking_plugin::MainCamera;

#[derive(Copy, Clone)]
pub struct PanEvent {
    pub orig_camera_pos: Vec2,
    pub delta: Vec2,
}

pub fn process_pan(
    mut events: EventReader<PanEvent>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
) {
    for PanEvent {
        orig_camera_pos,
        delta,
    } in events.iter().copied()
    {
        let mut camera = cameras.single_mut();
        camera.translation = (orig_camera_pos + delta * camera.scale.xy()).extend(CAMERA_Z);
    }
}
