use crate::lyon_compat::GeometryBuilder;
use crate::lyon_compat::RectangleOrigin;
use crate::lyon_compat::shapes;
use crate::lyon_compat::{Fill, Shape, ShapeBundle, Stroke};
use crate::mouse_tracking::{MainCamera, MousePosWorld};
use bevy::math::{Vec2, Vec3Swizzles};
use bevy::prelude::*;
use std::f32::consts::{PI, TAU};

use crate::FOREGROUND_Z;
use crate::tools::rotate::ROTATE_HELPER_RADIUS;

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
    mut root_shapes: Query<
        (
            &mut Shape,
            &mut Transform,
            Option<&mut Fill>,
            Option<&mut Stroke>,
        ),
        (With<crate::DrawObject>, Without<MainCamera>),
    >,
    mut last_overlay: Local<Option<(Entity, Overlay, Vec2, f32)>>,
    mut active_overlay: Local<Option<Entity>>,
) {
    let Some((draw_ent, shape, pos)) = overlay.draw_ent else {
        *last_overlay = None;
        if let Some(active_overlay) = active_overlay.take() {
            clear_overlay_shape(active_overlay, &mut root_shapes);
        }
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
        if *active_overlay == Some(draw_ent) {
            *active_overlay = None;
        }
        return;
    }

    if active_overlay.is_some_and(|active_overlay| active_overlay != draw_ent) {
        clear_overlay_shape(active_overlay.unwrap(), &mut root_shapes);
    }
    *active_overlay = Some(draw_ent);

    if let Overlay::Rotate(current_rot, scale, original_rot, click) = shape {
        let path = draw_rotate_overlay(
            &mut gizmos,
            pos,
            current_rot,
            scale,
            original_rot,
            click,
            mouse.xy(),
        );
        let pink = Color::srgba_u8(255, 64, 255, 127);
        upsert_overlay_shape(
            draw_ent,
            pos,
            path,
            crate::make_fill(pink),
            crate::make_stroke(pink, 0.0),
            &mut commands,
            &mut root_shapes,
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

    upsert_overlay_shape(
        draw_ent,
        pos,
        path,
        crate::make_fill(Color::srgba(0.0, 0.0, 0.0, 0.0)),
        crate::make_stroke(color, thickness * camera.scale.x),
        &mut commands,
        &mut root_shapes,
    );

    *last_overlay = Some(current);
}

fn clear_overlay_shape(
    draw_ent: Entity,
    root_shapes: &mut Query<
        (
            &mut Shape,
            &mut Transform,
            Option<&mut Fill>,
            Option<&mut Stroke>,
        ),
        (With<crate::DrawObject>, Without<MainCamera>),
    >,
) {
    let Ok((mut root_shape, _, root_fill, root_stroke)) = root_shapes.get_mut(draw_ent) else {
        return;
    };
    root_shape.path = GeometryBuilder::new().build();
    if let Some(mut root_fill) = root_fill {
        root_fill.color = Color::srgba(0.0, 0.0, 0.0, 0.0);
    }
    if let Some(mut root_stroke) = root_stroke {
        root_stroke.color = Color::srgba(0.0, 0.0, 0.0, 0.0);
    }
}

fn upsert_overlay_shape(
    draw_ent: Entity,
    pos: Vec2,
    path: bevy_prototype_lyon::prelude::tess::path::Path,
    fill: Fill,
    stroke: Stroke,
    commands: &mut Commands,
    root_shapes: &mut Query<
        (
            &mut Shape,
            &mut Transform,
            Option<&mut Fill>,
            Option<&mut Stroke>,
        ),
        (With<crate::DrawObject>, Without<MainCamera>),
    >,
) {
    match root_shapes.get_mut(draw_ent) {
        Ok((mut root_shape, mut root_transform, root_fill, root_stroke)) => {
            root_shape.path = path;
            root_transform.translation = pos.extend(FOREGROUND_Z);
            if let Some(mut root_fill) = root_fill {
                *root_fill = fill;
            } else {
                commands.entity(draw_ent).insert(fill);
            }
            if let Some(mut root_stroke) = root_stroke {
                *root_stroke = stroke;
            } else {
                commands.entity(draw_ent).insert(stroke);
            }
        }
        Err(_) => {
            commands.entity(draw_ent).insert((
                ShapeBundle::new(
                    path,
                    Transform::from_translation(pos.extend(FOREGROUND_Z)),
                    Visibility::Inherited,
                ),
                fill,
                stroke,
            ));
        }
    }
}

fn draw_rotate_overlay(
    gizmos: &mut Gizmos,
    center: Vec2,
    current_rot: f32,
    scale: f32,
    original_rot: f32,
    click: Vec2,
    mouse: Vec2,
) -> bevy_prototype_lyon::prelude::tess::path::Path {
    const RESOLUTION: u32 = 96;
    let helper_radius = scale * ROTATE_HELPER_RADIUS;

    gizmos
        .circle_2d(
            Isometry2d::new(center, Rot2::IDENTITY),
            helper_radius,
            Color::srgba(1.0, 1.0, 1.0, 0.68),
        )
        .resolution(64);
    draw_absolute_rotation_arc(gizmos, center, helper_radius, scale, current_rot);

    let start = click - center;
    let radius = mouse.distance(center);
    if start.length_squared() <= f32::EPSILON || radius <= f32::EPSILON {
        return GeometryBuilder::new().build();
    }

    let start_angle = start.y.atan2(start.x);
    let delta_angle = (current_rot - original_rot).clamp(-TAU, TAU);
    if delta_angle.abs() <= f32::EPSILON {
        return GeometryBuilder::new().build();
    }

    let segments = ((delta_angle.abs() / TAU) * RESOLUTION as f32)
        .ceil()
        .max(1.0) as u32;
    let arc_points = (0..=segments).map(|i| {
        let progress = i as f32 / segments as f32;
        let angle = start_angle + delta_angle * progress;
        Vec2::from_angle(angle) * radius
    });

    arc_points
        .fold(
            GeometryBuilder::new().begin(Vec2::ZERO),
            |builder, point| builder.line_to(point),
        )
        .close()
        .build()
}

fn draw_absolute_rotation_arc(
    gizmos: &mut Gizmos,
    center: Vec2,
    helper_radius: f32,
    camera_scale: f32,
    current_rot: f32,
) {
    const RESOLUTION: u32 = 96;
    let angle = {
        let normalized = current_rot.rem_euclid(TAU);
        if normalized > PI {
            normalized - TAU
        } else {
            normalized
        }
    };
    if angle.abs() <= f32::EPSILON {
        return;
    }

    let segments = ((angle.abs() / TAU) * RESOLUTION as f32).ceil().max(1.0) as u32;
    let color = Color::srgba(1.0, 1.0, 1.0, 0.82);

    for radius in [
        helper_radius + 2.0 * camera_scale,
        helper_radius + 4.0 * camera_scale,
    ] {
        let points = (0..=segments).map(|i| {
            let progress = i as f32 / segments as f32;
            let angle = angle * progress;
            (center + Vec2::from_angle(angle) * radius).extend(FOREGROUND_Z)
        });
        gizmos.linestrip(points, color);
    }
}
