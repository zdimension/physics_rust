use bevy::math::{Vec2, Vec3, Vec3Swizzles};
use bevy::prelude::*;
use bevy_egui::egui::ecolor::Hsva;

use bevy_prototype_lyon::entity::ShapeBundle;
use bevy_prototype_lyon::geometry::GeometryBuilder;
use bevy_prototype_lyon::prelude::RectangleOrigin;
use bevy_prototype_lyon::shapes;
use bevy_xpbd_2d::{math::*, prelude::*, components::*};

use crate::objects::ColorComponent;
use crate::update_from::UpdateFrom;
use crate::FillStroke;

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
    ext_forces: ExternalForce,
    pos: Position,
}

impl PhysicalObject {
    pub fn make(collider: Collider, shape: ShapeBundle, pos: Position) -> Self {
        Self {
            rigid_body: RigidBody::Dynamic,
            //velocity: Velocity::default(),
            mass_props: ColliderMassProperties::new_computed(&collider, 2.0),
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
            ext_forces: ExternalForce::default().with_persistence(false),
            pos,
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
                global_transform: GlobalTransform::from_translation(Vec3::new(0.0, 0.0, pos.z)),
                ..Default::default()
            },
            Position(pos.xy()),
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
            Collider::cuboid(size.x, size.y),
            ShapeBundle {
                path: GeometryBuilder::build_as(&shapes::Rectangle {
                    extents: size,
                    origin: RectangleOrigin::Center,
                }),
                transform: Transform::from_translation((size / 2.0).extend(0.0)),
                global_transform: GlobalTransform::from_translation(Vec3::new(0.0, 0.0, pos.z)),
                ..Default::default()
            },
            Position(pos.xy() + size / 2.0),
        )
    }

    pub fn poly(points: Vec<Vec2>, pos: Vec3) -> Self {
        Self::make(
            Collider::convex_hull(points.clone()).unwrap(),
            ShapeBundle {
                path: GeometryBuilder::build_as(&shapes::Polygon {
                    points,
                    closed: true,
                }),
                global_transform: GlobalTransform::from_translation(Vec3::new(0.0, 0.0, pos.z)), // todo: center of mass
                ..Default::default()
            },
            Position(pos.xy()),
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
