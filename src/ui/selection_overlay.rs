use std::ops::Neg;

use bevy::math::{Quat, Vec2, Vec3Swizzles};
use bevy::prelude::{
    BuildChildren, Color, Commands, DespawnRecursiveExt, Entity, Query, Res, ResMut, Resource,
    Transform, With,
};
use bevy_mouse_tracking_plugin::{MainCamera, MousePosWorld};
use bevy_prototype_lyon::entity::ShapeBundle;
use bevy_prototype_lyon::geometry::GeometryBuilder;
use bevy_prototype_lyon::prelude::{Geometry, Path, RectangleOrigin};
use bevy_prototype_lyon::shapes;
use lyon_path::geom::euclid::{Transform2D, Vector2D};
use lyon_path::math::vector;
use lyon_path::path::Builder;
use lyon_path::traits::PathBuilder;
use num_traits::FloatConst;

use crate::tools::rotate::ROTATE_HELPER_RADIUS;
use crate::FOREGROUND_Z;

#[derive(Copy, Clone)]
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
    cameras: Query<&mut Transform, With<MainCamera>>,
    mut overlay: ResMut<OverlayState>,
    mut commands: Commands,
    mouse: Res<MousePosWorld>,
) {
    if let Some((draw_ent, shape, pos)) = overlay.draw_ent {
        let Some(mut cmds) = commands.get_entity(draw_ent) else {
            overlay.draw_ent = None;
            return
        };
        cmds.despawn_descendants();
        let camera = cameras.single();
        let builder = GeometryBuilder::new();
        let (thickness, color, builder) = match shape {
            Overlay::Rectangle(size) => (
                5.0,
                Color::WHITE,
                builder.add(&shapes::Rectangle {
                    extents: size,
                    origin: RectangleOrigin::BottomLeft,
                }),
            ),
            Overlay::Circle(radius) => (
                5.0,
                Color::WHITE,
                builder.add(&shapes::Circle {
                    radius,
                    ..Default::default()
                }),
            ),
            Overlay::Rotate(_rot_value, scale, rot, click) => {
                let start = -(click - pos).angle_between(Vec2::X);
                let end = _rot_value - rot;
                cmds.with_children(|builder| {
                    builder.spawn((
                        ShapeBundle {
                            path: Path(
                                CircleSector {
                                    radius: mouse.xy().distance(pos),
                                    center: Vec2::ZERO,
                                    end_angle: end,
                                }
                                .add_geometry(Builder::new()),
                            ),
                            transform: Transform::from_rotation(Quat::from_rotation_z(start)),
                            ..Default::default()
                        },
                        crate::make_fill(Color::rgba_u8(0xff, 0x40, 0xff, 128)),
                    ));
                });

                (
                    3.0,
                    Color::rgba(1.0, 1.0, 1.0, 0.4),
                    builder.add(&shapes::Circle {
                        radius: scale * ROTATE_HELPER_RADIUS,
                        ..Default::default()
                    }),
                )
            }
        };
        // todo: rotate helper 2
        cmds.insert((
            ShapeBundle {
                path: builder.build(),
                transform: Transform::from_translation(pos.extend(FOREGROUND_Z)),
                ..Default::default()
            },
            crate::make_stroke(color, thickness * camera.scale.x),
        ));
    }
}
