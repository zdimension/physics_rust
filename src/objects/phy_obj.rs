use crate::objects::ColorComponent;
use crate::update_from::UpdateFrom;
use crate::BORDER_THICKNESS;
use bevy::math::{Vec2, Vec3};
use bevy::prelude::{Bundle, Color, Component, Transform};
use bevy_egui::egui::ecolor::Hsva;
use bevy_prototype_lyon::draw::DrawMode;
use bevy_prototype_lyon::entity::ShapeBundle;
use bevy_prototype_lyon::geometry::GeometryBuilder;
use bevy_prototype_lyon::prelude::RectangleOrigin;
use bevy_prototype_lyon::shapes;
use bevy_rapier2d::dynamics::{ReadMassProperties, RigidBody, Velocity};
use bevy_rapier2d::geometry::{
    Collider, ColliderMassProperties, CollisionGroups, Friction, Group, Restitution,
};

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
        }
    }

    pub fn ball(radius: f32, pos: Vec3) -> Self {
        let radius = radius.abs();
        Self::make(
            Collider::ball(radius),
            GeometryBuilder::build_as(
                &shapes::Circle {
                    radius,
                    ..Default::default()
                },
                DrawMode::Outlined {
                    fill_mode: crate::make_fill(Color::CYAN),
                    outline_mode: crate::make_stroke(Color::BLACK, BORDER_THICKNESS),
                },
                Transform::from_translation(pos),
            ),
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
            GeometryBuilder::build_as(
                &shapes::Rectangle {
                    extents: size,
                    origin: RectangleOrigin::Center,
                },
                DrawMode::Outlined {
                    fill_mode: crate::make_fill(Color::CYAN),
                    outline_mode: crate::make_stroke(Color::BLACK, BORDER_THICKNESS),
                },
                Transform::from_translation(pos + (size / 2.0).extend(0.0)),
            ),
        )
    }

    pub fn poly(points: Vec<Vec2>, pos: Vec3) -> Self {
        Self::make(
            Collider::convex_hull(&points).unwrap(),
            GeometryBuilder::build_as(
                &shapes::Polygon {
                    points,
                    closed: true,
                },
                DrawMode::Outlined {
                    fill_mode: crate::make_fill(Color::CYAN),
                    outline_mode: crate::make_stroke(Color::BLACK, BORDER_THICKNESS),
                },
                Transform::from_translation(pos),
            ),
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