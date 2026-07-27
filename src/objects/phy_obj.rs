use bevy::math::{Vec2, Vec3, Vec3Swizzles};
use bevy::prelude::*;
use bevy_egui::egui::ecolor::Hsva;

use crate::lyon_compat::GeometryBuilder;
use crate::lyon_compat::RectangleOrigin;
use crate::lyon_compat::ShapeBundle;
use crate::lyon_compat::shapes;
use avian2d::prelude::*;

use crate::objects::{CircleAngleMarker, ColorComponent};
use crate::update_from::UpdateFrom;
use crate::{BORDER_THICKNESS, FillStroke};

#[derive(Component)]
pub struct CircleVisual(pub f32);

#[derive(Bundle)]
pub struct PhysicalObject {
    rigid_body: RigidBody,
    //velocity: Velocity,
    collider: Collider,
    friction: Friction,
    restitution: Restitution,
    mass_props: ColliderMassProperties,
    shape: ShapeBundle,
    //read_props: ReadMassProperties,
    groups: CollisionLayers,
    refractive_index: RefractiveIndex,
    color: ColorComponent,
    color_upd: UpdateFrom<ColorComponent>,
    fill_stroke: FillStroke,
    sleeping: SleepingDisabled,
    pos: Position,
    circle_visual: CircleVisual,
}

impl PhysicalObject {
    pub fn make(collider: Collider, shape: ShapeBundle, pos: Position) -> Self {
        Self {
            rigid_body: RigidBody::Dynamic,
            //velocity: Velocity::default(),
            mass_props: ColliderMassProperties::from_shape(&collider, 2.0),
            collider,
            friction: Friction::default(),
            restitution: Restitution::new(0.7),
            shape,
            //read_props: ReadMassProperties::default(),
            groups: CollisionLayers::from_bits(1, 1),
            refractive_index: RefractiveIndex::default(),
            color: ColorComponent(Hsva::new(0.0, 1.0, 1.0, 1.0)),
            color_upd: UpdateFrom::This,
            fill_stroke: FillStroke::default(),
            sleeping: SleepingDisabled, // todo: better
            pos,
            circle_visual: CircleVisual(0.0),
        }
    }

    pub fn ball(radius: f32, pos: Vec3) -> Self {
        let radius = radius.abs();
        let mut object = Self::make(
            Collider::circle(radius),
            ShapeBundle::new(
                GeometryBuilder::build_as(&shapes::Circle {
                    radius: (radius - BORDER_THICKNESS * 0.5).max(radius * 0.5),
                    ..Default::default()
                }),
                Transform::from_translation(Vec3::new(0.0, 0.0, pos.z)),
                Visibility::Inherited,
            ),
            Position(pos.xy()),
        );
        object.circle_visual = CircleVisual(radius);
        object
    }

    pub fn rect(mut size: Vec2, mut pos: Vec3) -> Self {
        if size.x < 0.0 {
            pos.x += size.x;
            size.x = -size.x;
        }
        if size.y < 0.0 {
            pos.y += size.y;
            size.y = -size.y;
        }
        Self::make(
            Collider::rectangle(size.x, size.y),
            ShapeBundle::new(
                GeometryBuilder::build_as(&shapes::Rectangle {
                    extents: (size - Vec2::splat(BORDER_THICKNESS)).max(Vec2::splat(f32::EPSILON)),
                    origin: RectangleOrigin::Center,
                    radii: None,
                }),
                Transform::from_translation((size / 2.0).extend(pos.z)),
                Visibility::Inherited,
            ),
            Position(pos.xy() + size / 2.0),
        )
    }

    pub fn poly(points: Vec<Vec2>, pos: Vec3) -> Self {
        Self::make(
            Collider::convex_hull(points.clone()).unwrap(),
            ShapeBundle::new(
                GeometryBuilder::build_as(&shapes::Polygon {
                    points,
                    closed: true,
                }),
                Transform::from_translation(Vec3::new(0.0, 0.0, pos.z)), // todo: center of mass
                Visibility::Inherited,
            ),
            Position(pos.xy()),
        )
    }
}

#[derive(Component, Copy, Clone)]
pub struct RefractiveIndex(pub(crate) f32);

impl Default for RefractiveIndex {
    fn default() -> Self {
        RefractiveIndex(1.5)
    }
}
pub fn spawn_circle_angle_markers(
    circles: Query<(Entity, &CircleVisual), Added<CircleVisual>>,
    mut commands: Commands,
) {
    for (entity, circle) in circles.iter() {
        if circle.0 <= 0.0 {
            continue;
        }
        commands.entity(entity).with_children(|parent| {
            const SEGMENTS: usize = 6;
            let marker_radius = (circle.0 - BORDER_THICKNESS).max(circle.0 * 0.5);
            let mut points = Vec::with_capacity(SEGMENTS + 2);
            points.push(Vec2::ZERO);
            for step in 0..=SEGMENTS {
                let angle = (-5.0 + 10.0 * step as f32 / SEGMENTS as f32).to_radians();
                points.push(Vec2::from_angle(angle) * marker_radius);
            }
            parent.spawn((
                ShapeBundle::new(
                    GeometryBuilder::build_as(&shapes::Polygon {
                        points,
                        closed: true,
                    }),
                    Transform::from_translation(Vec3::new(0.0, 0.0, 0.25)),
                    Visibility::Inherited,
                ),
                crate::make_fill(Color::WHITE),
                UpdateFrom::<ColorComponent>::entity(entity),
                CircleAngleMarker,
            ));
        });
    }
}
