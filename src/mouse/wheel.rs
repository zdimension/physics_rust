use bevy::input::mouse::MouseWheel;
use bevy::math::{Vec2, Vec3};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;

use crate::mouse_tracking::MainCamera;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CameraZoomSet;

#[derive(Resource, Default)]
pub struct SmoothZoom {
    target_scale: Option<f32>,
    focus: Vec2,
}

pub fn mouse_wheel(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut mouse_wheel_events: MessageReader<MouseWheel>,
    cameras: Query<&Transform, With<MainCamera>>,
    mut zoom: ResMut<SmoothZoom>,
    mut egui_ctx: EguiContexts,
) {
    if mouse_wheel_events.is_empty() {
        return;
    }
    let events = mouse_wheel_events.read().copied().collect::<Vec<_>>();
    if egui_caught_scroll(&mut egui_ctx) {
        return;
    }

    let prim = windows.single().unwrap();
    let Some(pos) = prim.cursor_position() else {
        return;
    };
    let current_scale = cameras.single().unwrap().scale.x;
    let mut target_scale = zoom.target_scale.unwrap_or(current_scale);

    for event in events {
        const FACTOR: f32 = 0.1;
        let factor = if event.y < 0.0 {
            1.0 + FACTOR
        } else {
            1.0 / (1.0 + FACTOR)
        };
        target_scale *= factor;
    }

    zoom.target_scale = Some(target_scale);
    zoom.focus = pos;
}

fn egui_caught_scroll(egui_ctx: &mut EguiContexts) -> bool {
    let Ok(ctx) = egui_ctx.ctx_mut() else {
        return false;
    };
    ctx.input(|input| input.smooth_scroll_delta.y.abs() <= f32::EPSILON)
}

pub fn smooth_zoom(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    mut zoom: ResMut<SmoothZoom>,
    time: Res<Time>,
) {
    let Some(target_scale) = zoom.target_scale else {
        return;
    };
    let prim = windows.single().unwrap();
    let mut transform = cameras.single_mut().unwrap();
    let current_scale = transform.scale.x;

    if current_scale == target_scale {
        zoom.target_scale = None;
        return;
    }

    const EASING_RATE: f32 = 12.0;
    let blend = 1.0 - (-EASING_RATE * time.delta_secs()).exp();
    let next_scale = current_scale + (target_scale - current_scale) * blend;
    let factor = next_scale / current_scale;
    let offset = zoom.focus - Vec2::new(prim.width(), prim.height()) * 0.5;
    let old = transform.transform_point(offset.extend(1.0));
    transform.scale *= Vec3::new(factor, factor, 1.0);
    let new = transform.transform_point(offset.extend(1.0));
    transform.translation -= (new - old) * Vec3::new(1.0, -1.0, 1.0);
}
