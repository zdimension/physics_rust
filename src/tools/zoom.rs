use bevy::math::{Vec2, Vec3};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::mouse_tracking::MainCamera;

#[derive(Copy, Clone, Debug)]
pub struct ZoomState {
    pub orig_camera_pos: Vec2,
    pub orig_camera_scale: f32,
    pub click_pos_screen: Vec2,
}

#[derive(Copy, Clone, Message)]
pub struct ZoomEvent {
    pub state: ZoomState,
    pub mouse_pos_screen: Vec2,
}

pub fn process_zoom(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut events: MessageReader<ZoomEvent>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok(mut camera) = cameras.single_mut() else {
        return;
    };

    for ZoomEvent {
        state,
        mouse_pos_screen,
    } in events.read().copied()
    {
        let delta = mouse_pos_screen - state.click_pos_screen;
        let axis = Vec2::new(1.0, -1.0).normalize();
        let projected = delta.dot(axis);
        const PIXELS_PER_ZOOM_STEP: f32 = 240.0;
        const STEP_FACTOR: f32 = 1.5;
        let factor = STEP_FACTOR.powf(-projected / PIXELS_PER_ZOOM_STEP);
        let next_scale = (state.orig_camera_scale * factor).max(f32::EPSILON);
        zoom_about_screen_pos(
            &mut camera,
            state.orig_camera_pos,
            state.orig_camera_scale,
            next_scale,
            state.click_pos_screen,
            window.size(),
        );
    }
}

fn zoom_about_screen_pos(
    camera: &mut Transform,
    orig_camera_pos: Vec2,
    orig_camera_scale: f32,
    next_scale: f32,
    focus: Vec2,
    window_size: Vec2,
) {
    let offset = focus - window_size * 0.5;
    let scale_delta = next_scale - orig_camera_scale;
    camera.scale = Vec3::new(next_scale, next_scale, camera.scale.z);
    camera.translation = (orig_camera_pos - offset * Vec2::new(1.0, -1.0) * scale_delta)
        .extend(camera.translation.z);
}
