use bevy::math::{Vec2, Vec3, Vec3Swizzles};
use bevy::prelude::*;
use bevy_egui::egui::ecolor::Hsva;

use crate::lyon_compat::GeometryBuilder;
use crate::lyon_compat::RectangleOrigin;
use crate::lyon_compat::ShapeBundle;
use crate::lyon_compat::shapes;
use avian2d::prelude::*;

use crate::objects::attraction::Attraction;
use crate::objects::{CircleAngleMarker, ColorComponent};
use crate::tools::polygon::{polygon_path, tessellate_polygon};
use crate::update_from::UpdateFrom;
use crate::{BORDER_THICKNESS, FillStroke};

#[derive(Component)]
pub struct CircleVisual(pub f32);

#[derive(Component)]
pub struct FreeformObject;

#[derive(Component, Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum FrictionModel {
    #[default]
    Simple,
    Advanced,
}

/// Material and collision properties shared by every physical scene object.
#[derive(Bundle)]
pub struct PhysicalProperties {
    friction: Friction,
    friction_model: FrictionModel,
    restitution: Restitution,
    groups: CollisionLayers,
    refractive_index: RefractiveIndex,
}

impl PhysicalProperties {
    pub(crate) fn with_collision_layers(mut self, groups: CollisionLayers) -> Self {
        self.groups = groups;
        self
    }
}

impl Default for PhysicalProperties {
    fn default() -> Self {
        Self {
            friction: Friction::default().with_combine_rule(CoefficientCombine::Multiply),
            friction_model: FrictionModel::default(),
            restitution: Restitution::new(0.7),
            groups: CollisionLayers::from_bits(1, 1),
            refractive_index: RefractiveIndex::default(),
        }
    }
}

#[derive(Bundle)]
pub struct PhysicalObject {
    rigid_body: RigidBody,
    //velocity: Velocity,
    collider: Collider,
    density: ColliderDensity,
    properties: PhysicalProperties,
    shape: ShapeBundle,
    //read_props: ReadMassProperties,
    color: ColorComponent,
    color_upd: UpdateFrom<ColorComponent>,
    fill_stroke: FillStroke,
    sleeping: SleepingDisabled,
    pos: Position,
    circle_visual: CircleVisual,
    attraction: Attraction,
}

impl PhysicalObject {
    pub fn make(collider: Collider, shape: ShapeBundle, pos: Position) -> Self {
        Self {
            rigid_body: RigidBody::Dynamic,
            //velocity: Velocity::default(),
            collider,
            density: ColliderDensity(2.0),
            properties: PhysicalProperties::default(),
            shape,
            //read_props: ReadMassProperties::default(),
            color: ColorComponent(Hsva::new(0.0, 1.0, 1.0, 1.0)),
            color_upd: UpdateFrom::This,
            fill_stroke: FillStroke::default(),
            sleeping: SleepingDisabled, // todo: better
            pos,
            circle_visual: CircleVisual(0.0),
            attraction: Attraction::default(),
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

    pub fn freeform(points: &[Vec2], pos: Vec3) -> Option<Self> {
        let geometry = tessellate_polygon(points)?;
        Some(Self::make(
            geometry.collider(),
            ShapeBundle::new(
                polygon_path(points, true),
                Transform::from_translation(pos),
                Visibility::Inherited,
            ),
            Position(pos.xy()),
        ))
    }
}

#[derive(Component, Copy, Clone)]
pub struct RefractiveIndex(pub(crate) f32);

impl Default for RefractiveIndex {
    fn default() -> Self {
        RefractiveIndex(1.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn shared_physical_properties_use_multiply_friction() {
        let properties = PhysicalProperties::default();

        assert_eq!(
            properties.friction.combine_rule,
            CoefficientCombine::Multiply
        );
        assert_eq!(properties.friction_model, FrictionModel::Simple);
        assert_eq!(properties.restitution.coefficient, 0.7);
        assert_eq!(properties.refractive_index.0, 1.5);

        let mut world = World::new();
        let entity = world.spawn(properties).id();
        assert!(world.get::<Friction>(entity).is_some());
        assert!(world.get::<FrictionModel>(entity).is_some());
        assert!(world.get::<Restitution>(entity).is_some());
        assert!(world.get::<CollisionLayers>(entity).is_some());
        assert!(world.get::<RefractiveIndex>(entity).is_some());
    }

    #[test]
    fn shared_physical_properties_can_override_collision_layers() {
        let properties = PhysicalProperties::default().with_collision_layers(CollisionLayers::ALL);

        assert_eq!(properties.groups, CollisionLayers::ALL);
    }

    #[test]
    fn physical_objects_default_to_two_kilograms_per_square_meter() {
        let object = PhysicalObject::ball(1.0, Vec3::ZERO);

        assert_eq!(object.density, ColliderDensity(2.0));
    }

    #[test]
    fn angle_marker_tracks_circle_geometry_changes() {
        let mut world = World::new();
        let object = world.spawn(CircleVisual(2.0)).id();

        world.run_system_once(spawn_circle_angle_markers).unwrap();
        let marker = world
            .query_filtered::<(Entity, &ChildOf), With<CircleAngleMarker>>()
            .iter(&world)
            .find_map(|(marker, parent)| (parent.parent() == object).then_some(marker))
            .expect("circle marker should be created");

        world.entity_mut(object).insert(CircleVisual(0.0));
        world.run_system_once(spawn_circle_angle_markers).unwrap();
        assert!(world.get_entity(marker).is_err());
    }
}

pub fn spawn_circle_angle_markers(
    circles: Query<(Entity, &CircleVisual), Changed<CircleVisual>>,
    markers: Query<(Entity, &ChildOf), With<CircleAngleMarker>>,
    mut commands: Commands,
) {
    for (entity, circle) in circles.iter() {
        let existing_markers = markers
            .iter()
            .filter_map(|(marker, parent)| (parent.parent() == entity).then_some(marker))
            .collect::<Vec<_>>();
        if circle.0 <= 0.0 {
            for marker in existing_markers {
                commands.entity(marker).despawn();
            }
            continue;
        }
        if !existing_markers.is_empty() {
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
