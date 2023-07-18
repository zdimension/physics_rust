use bevy::math::{Vec2, Vec3};
use bevy::prelude::*;
use bevy_egui::egui::ecolor::Hsva;

use bevy_prototype_lyon::entity::ShapeBundle;
use bevy_prototype_lyon::geometry::GeometryBuilder;
use bevy_prototype_lyon::prelude::RectangleOrigin;
use bevy_prototype_lyon::shapes;
use bevy_rapier2d::dynamics::{ReadMassProperties, RigidBody, Velocity};
use bevy_rapier2d::geometry::{
    Collider, ColliderMassProperties, CollisionGroups, Friction, Group, Restitution,
};
use bevy_rapier2d::prelude::{ExternalForce, Sleeping};

use crate::objects::ColorComponent;
use crate::update_from::UpdateFrom;
use crate::FillStroke;

#[derive(Bundle)]
pub struct PhysicalObject {
    rigid_body: RigidBody,
    velocity: Velocity,
    collider: Collider,
    friction: Friction,
    restitution: Restitution,
    mass_props: ColliderMassProperties,
    shape: ShapeBundle,
    read_props: ReadMassProperties,
    groups: CollisionGroups,
    refractive_index: RefractiveIndex,
    color: ColorComponent,
    color_upd: UpdateFrom<ColorComponent>,
    fill_stroke: FillStroke,
    sleeping: Sleeping,
    ext_forces: ExternalForce,
}

impl PhysicalObject {
    pub fn make(collider: Collider, shape: ShapeBundle) -> Self {
        Self {
            rigid_body: RigidBody::Dynamic,
            velocity: Velocity::default(),
            collider,
            friction: Friction::default(),
            restitution: Restitution::coefficient(0.7),
            mass_props: ColliderMassProperties::Density(1.0),
            shape,
            read_props: ReadMassProperties::default(),
            groups: CollisionGroups::new(Group::GROUP_1, Group::GROUP_1),
            refractive_index: RefractiveIndex::default(),
            color: ColorComponent(Hsva::new(0.0, 1.0, 1.0, 1.0)),
            color_upd: UpdateFrom::This,
            fill_stroke: FillStroke::default(),
            sleeping: Sleeping::disabled(), // todo: better
            ext_forces: ExternalForce::default(),
        }
    }

    pub fn ball(radius: f32, pos: Vec3) -> Self {
        let radius = radius.abs();
        Self::make(
            Collider::ball(radius),
            ShapeBundle {
                path: GeometryBuilder::build_as(&shapes::Circle {
                    radius,
                    ..Default::default()
                }),
                transform: Transform::from_translation(pos),
                ..Default::default()
            },
        )
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
            Collider::cuboid(size.x / 2.0, size.y / 2.0),
            ShapeBundle {
                path: GeometryBuilder::build_as(&shapes::Rectangle {
                    extents: size,
                    origin: RectangleOrigin::Center,
                }),
                transform: Transform::from_translation(pos + (size / 2.0).extend(0.0)),
                ..Default::default()
            },
        )
    }

    pub fn poly(points: Vec<Vec2>, pos: Vec3) -> Self {
        Self::make(
            Collider::convex_hull(&points).unwrap(),
            ShapeBundle {
                path: GeometryBuilder::build_as(&shapes::Polygon {
                    points,
                    closed: true,
                }),
                transform: Transform::from_translation(pos),
                ..Default::default()
            },
        )
    }
}

#[derive(Component)]
pub struct RefractiveIndex(pub(crate) f32);

impl Default for RefractiveIndex {
    fn default() -> Self {
        RefractiveIndex(1.5)
    }
}
