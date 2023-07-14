use crate::CAMERA_Z;
use bevy::math::{Vec2, Vec3Swizzles};
use bevy::prelude::*;
use bevy_mouse_tracking_plugin::MainCamera;
use crate::config::AppConfig;

#[derive(Copy, Clone, Event)]
pub struct PanEvent {
    pub orig_camera_pos: Vec2,
    pub delta: Vec2,
}

pub fn process_pan(
    mut events: EventReader<PanEvent>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    mut pan_speed: Local<Option<Vec2>>,
    mut speed_stat: Local<(Vec2, f32)>,
    mut last_delta: Local<Option<Vec2>>,
    app_config: Res<AppConfig>,
    time: Res<Time>,
) {
    if app_config.kinetic_panning && events.is_empty() {
        if let Some(ref mut v) = *pan_speed {
            if v.length() < 0.1 {
                *pan_speed = None;
            } else {
                let mut camera = cameras.single_mut();
                camera.translation += (*v * time.delta_seconds()).extend(0.0);
                *v *= 0.97;
            }
        }
        return;
    }
    for PanEvent {
        orig_camera_pos,
        delta,
    } in events.iter().copied()
    {
        let mut camera = cameras.single_mut();
        let delta_scaled = delta * camera.scale.xy() * Vec2::new(1.0, -1.0);
        if let Some(v) = *last_delta {
            let old_total = *speed_stat;
            let (mut new_total, mut new_time) = (old_total.0 + (delta_scaled - v), old_total.1 + time.delta_seconds());
            if new_time > 0.01 {
                *pan_speed = Some(new_total / new_time);
                new_total = Vec2::ZERO;
                new_time = 0.0;
            }
            *speed_stat = (new_total, new_time);
        }
        *last_delta = Some(delta_scaled);
        camera.translation =
            (orig_camera_pos + delta_scaled).extend(CAMERA_Z);
    }
}

#[derive(Copy, Clone, Debug)]
pub struct PanState {
    pub orig_camera_pos: Vec2,
}
