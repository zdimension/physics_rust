use std::ops::Neg;

use bevy::math::{Quat, Vec2, Vec3Swizzles};
use bevy::prelude::*;
use crate::mouse_tracking::{MainCamera, MousePosWorld};
use crate::lyon_compat::{Shape, ShapeBundle};
use crate::lyon_compat::GeometryBuilder;
use crate::lyon_compat::RectangleOrigin;
use crate::lyon_compat::shapes;
use lyon_path::geom::euclid::{Transform2D, Vector2D};
use lyon_path::math::vector;
use lyon_path::path::Builder;
use lyon_path::traits::PathBuilder;
use num_traits::FloatConst;

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CircleSector {
    pub radius: f32,
    pub center: Vec2,
    pub end_angle: f32,
}

impl CircleSector {
    pub fn add_geometry(&self, builder: Builder) -> lyon_path::Path {
        let scale = self.radius;
        let mut total_angle = self.end_angle;
        if total_angle == 0.0 {
            return lyon_path::Path::new();
        }
        let mut xform = Transform2D::default()
            .then_translate(vector(self.center.x, self.center.y))
            .then_scale(scale, scale);
        if total_angle < 0.0 {
            total_angle = -total_angle;
            xform = xform.then_scale(1.0, -1.0);
        } else {
        }
        let mut builder = builder.transformed(xform);

        let total2 = total_angle;
        use lyon_path::math::point;
        let mut current = point(1.0, 0.0);
        const CONSTANT_FACTOR2: f32 = 0.55191505;
        builder.begin(point(0.0, 0.0));
        builder.line_to(current);
        if total_angle > f32::FRAC_PI_2() {
            current = point(0.0, 1.0);
            builder.cubic_bezier_to(
                point(1.0, CONSTANT_FACTOR2),
                point(CONSTANT_FACTOR2, 1.0),
                current,
            );
            total_angle -= f32::FRAC_PI_2();
        }
        if total_angle > f32::FRAC_PI_2() {
            current = point(-1.0, 0.0);
            builder.cubic_bezier_to(
                point(-CONSTANT_FACTOR2, 1.0),
                point(-1.0, CONSTANT_FACTOR2),
                current,
            );
            total_angle -= f32::FRAC_PI_2();
        }
        if total_angle > f32::FRAC_PI_2() {
            current = point(0.0, -1.0);
            builder.cubic_bezier_to(
                point(-1.0, -CONSTANT_FACTOR2),
                point(-CONSTANT_FACTOR2, -1.0),
                current,
            );
            total_angle -= f32::FRAC_PI_2();
        }

        let end = vector(total2.cos(), total2.sin());
        let q2 = 1.0 + current.to_vector().dot(end);
        let k = 4.0 / 3.0 * ((2.0 * q2).sqrt() - q2) / current.to_vector().cross(end);

        fn perp<T, U>(v: Vector2D<T, U>) -> Vector2D<T, U>
        where
            T: Copy + Neg<Output = T>,
            U: Copy,
        {
            Vector2D::new(-v.y, v.x)
        }

        builder.cubic_bezier_to(
            current + perp(current.to_vector()) * k,
            end.to_point() - perp(end) * k,
            end.to_point(),
        );

        builder.close();

        builder.build()
    }
}

pub fn process_draw_overlay(
    cameras: Query<&Transform, With<MainCamera>>,
    mut overlay: ResMut<OverlayState>,
    mut commands: Commands,
    mouse: Res<MousePosWorld>,
    mut root_shapes: Query<(&mut Shape, &mut Transform), (With<crate::DrawObject>, Without<MainCamera>)>,
    mut child_shapes: Query<(&ChildOf, &mut Shape, &mut Transform), (Without<crate::DrawObject>, Without<MainCamera>)>,
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
        Overlay::Rotate(_, scale, _, _) => (
            3.0,
            Color::srgba(1.0, 1.0, 1.0, 0.4),
            builder
                .add(&shapes::Circle {
                    radius: scale * ROTATE_HELPER_RADIUS,
                    ..Default::default()
                })
                .build(),
        ),
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

    if let Overlay::Rotate(current_rot, _, original_rot, click) = shape {
        let start = -((click - pos).perp_dot(Vec2::X).atan2((click - pos).dot(Vec2::X)));
        let sector = CircleSector {
            radius: mouse.xy().distance(pos),
            center: Vec2::ZERO,
            end_angle: current_rot - original_rot,
        };
        let mut updated = false;
        for (parent, mut child_shape, mut child_transform) in child_shapes.iter_mut() {
            if parent.parent() == draw_ent {
                child_shape.path = sector.add_geometry(Builder::new());
                *child_transform = Transform::from_rotation(Quat::from_rotation_z(start));
                updated = true;
                break;
            }
        }
        if !updated {
            commands.entity(draw_ent).with_children(|parent| {
                parent.spawn((
                    ShapeBundle::new(
                        sector.add_geometry(Builder::new()),
                        Transform::from_rotation(Quat::from_rotation_z(start)),
                        Visibility::Inherited,
                    ),
                    crate::make_fill(Color::srgba_u8(0xff, 0x40, 0xff, 128)),
                ));
            });
        }
    }

    *last_overlay = Some(current);
}