use bevy::math::{Vec2, Vec3Swizzles};
use bevy::prelude::*;
use crate::mouse_tracking::{MainCamera, MousePosWorld};
use crate::lyon_compat::{Shape, ShapeBundle};
use crate::lyon_compat::GeometryBuilder;
use crate::lyon_compat::RectangleOrigin;
use crate::lyon_compat::shapes;
use std::f32::consts::TAU;

use crate::tools::rotate::ROTATE_HELPER_RADIUS;
use crate::FOREGROUND_Z;

#[derive(Copy, Clone, PartialEq)]
pub enum Overlay {
    Rectangle(Vec2),
    Circle(f32),
    Rotate(f32, f32, f32, Vec2),
}

#[derive(Resource, Default)]
pub struct OverlayState {
    pub draw_ent: Option<(Entity, Overlay, Vec2)>,
}

pub fn process_draw_overlay(
    cameras: Query<&Transform, With<MainCamera>>,
    mut overlay: ResMut<OverlayState>,
    mut commands: Commands,
    mouse: Res<MousePosWorld>,
    mut gizmos: Gizmos,
    mut root_shapes: Query<(&mut Shape, &mut Transform), (With<crate::DrawObject>, Without<MainCamera>)>,
    mut last_overlay: Local<Option<(Entity, Overlay, Vec2, f32)>>,
) {
    let Some((draw_ent, shape, pos)) = overlay.draw_ent else {
        *last_overlay = None;
        return;
    };

    let Ok(camera) = cameras.single() else {
        return;
    };
    let current = (draw_ent, shape, pos, camera.scale.x);
    if last_overlay.as_ref() == Some(&current) {
        return;
    }

    if commands.get_entity(draw_ent).is_err() {
        overlay.draw_ent = None;
        *last_overlay = None;
        return;
    }

    if let Overlay::Rotate(current_rot, scale, original_rot, click) = shape {
        draw_rotate_overlay(
            &mut gizmos,
            pos,
            current_rot,
            scale,
            original_rot,
            click,
            mouse.xy(),
        );
        *last_overlay = None;
        return;
    }

    let builder = GeometryBuilder::new();
    let (thickness, color, path) = match shape {
        Overlay::Rectangle(size) => (
            5.0,
            Color::WHITE,
            builder
                .add(&shapes::Rectangle {
                    extents: size,
                    origin: RectangleOrigin::BottomLeft,
                    radii: None,
                })
                .build(),
        ),
        Overlay::Circle(radius) => (
            5.0,
            Color::WHITE,
            builder
                .add(&shapes::Circle {
                    radius,
                    ..Default::default()
                })
                .build(),
        ),
        Overlay::Rotate(..) => unreachable!(),
    };

    match root_shapes.get_mut(draw_ent) {
        Ok((mut root_shape, mut root_transform)) => {
            root_shape.path = path;
            root_transform.translation = pos.extend(FOREGROUND_Z);
        }
        Err(_) => {
            commands.entity(draw_ent).insert((
                ShapeBundle::new(
                    path,
                    Transform::from_translation(pos.extend(FOREGROUND_Z)),
                    Visibility::Inherited,
                ),
                crate::make_stroke(color, thickness * camera.scale.x),
            ));
        }
    }

    *last_overlay = Some(current);
}

fn draw_rotate_overlay(
    gizmos: &mut Gizmos,
    center: Vec2,
    current_rot: f32,
    scale: f32,
    original_rot: f32,
    click: Vec2,
    mouse: Vec2,
) {
    const RESOLUTION: u32 = 96;
    let z = FOREGROUND_Z;
    let helper_radius = scale * ROTATE_HELPER_RADIUS;
    let helper_circle = (0..=RESOLUTION).map(|i| {
        let angle = i as f32 * TAU / RESOLUTION as f32;
        (center + Vec2::from_angle(angle) * helper_radius).extend(z)
    });
    gizmos.linestrip(helper_circle, Color::srgba(1.0, 1.0, 1.0, 0.4));

    let start = click - center;
    let radius = mouse.distance(center);
    if start.length_squared() <= f32::EPSILON || radius <= f32::EPSILON {
        return;
    }

    let start_angle = start.y.atan2(start.x);
    let delta_angle = (current_rot - original_rot).clamp(-TAU, TAU);
    if delta_angle.abs() <= f32::EPSILON {
        return;
    }

    let sector_color = Color::srgba_u8(0xff, 0x40, 0xff, 180);
    let fill_color = Color::srgba_u8(0xff, 0x40, 0xff, 48);
    let segments = ((delta_angle.abs() / TAU) * RESOLUTION as f32).ceil().max(1.0) as u32;
    let arc_points = (0..=segments).map(|i| {
        let progress = i as f32 / segments as f32;
        let angle = start_angle + delta_angle * progress;
        center + Vec2::from_angle(angle) * radius
    });

    let center_3d = center.extend(z);
    let arc_points = arc_points.collect::<Vec<_>>();
    gizmos.line(center_3d, arc_points[0].extend(z), sector_color);
    gizmos.line(center_3d, arc_points[arc_points.len() - 1].extend(z), sector_color);
    gizmos.linestrip(arc_points.iter().map(|point| point.extend(z)), sector_color);

    for point in arc_points.iter().step_by(4) {
        gizmos.line(center_3d, point.extend(z), fill_color);
    }
}
