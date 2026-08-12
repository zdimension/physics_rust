use bevy::math::{Vec2, Vec3, Vec3Swizzles};
use bevy::prelude::*;
use bevy_egui::egui::ecolor::Hsva;

use crate::lyon_compat::GeometryBuilder;
use crate::lyon_compat::RectangleOrigin;
use crate::lyon_compat::shapes;
use crate::lyon_compat::{Shape, ShapeBundle};
use avian2d::prelude::*;

use crate::FillStroke;
use crate::objects::attraction::Attraction;
use crate::objects::{CircleAngleMarker, ColorComponent};
use crate::tools::{
    add_object::DepthSorter,
    polygon::{polygon_path, tessellate_path},
};
use crate::update_from::UpdateFrom;
use bevy_prototype_lyon::prelude::tess::path::Path;

#[derive(Component)]
pub struct CircleVisual(pub f32);

#[derive(Component)]
pub struct FreeformObject;

#[derive(Component, Copy, Clone, Debug)]
pub struct PhysicalGeometry;

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
struct PhysicalGeometryBundle {
    marker: PhysicalGeometry,
    collider: Collider,
    density: ColliderDensity,
    properties: PhysicalProperties,
    shape: ShapeBundle,
    color: ColorComponent,
    color_upd: UpdateFrom<ColorComponent>,
    fill_stroke: FillStroke,
    circle_visual: CircleVisual,
    attraction: Attraction,
}

pub struct PhysicalObject {
    geometry: PhysicalGeometryBundle,
    position: Vec2,
    rotation: Rotation,
    z: f32,
}

impl PhysicalObject {
    pub fn make(collider: Collider, shape: ShapeBundle, pos: Position, z: f32) -> Self {
        Self {
            geometry: PhysicalGeometryBundle {
                marker: PhysicalGeometry,
                collider,
                density: ColliderDensity(2.0),
                properties: PhysicalProperties::default(),
                shape,
                color: ColorComponent(Hsva::new(0.0, 1.0, 1.0, 1.0)),
                color_upd: UpdateFrom::This,
                fill_stroke: FillStroke::default(),
                circle_visual: CircleVisual(0.0),
                attraction: Attraction::default(),
            },
            position: pos.0,
            rotation: Rotation::default(),
            z,
        }
    }

    pub fn ball(radius: f32, pos: Vec3) -> Self {
        let radius = radius.abs();
        let mut object = Self::make(
            Collider::circle(radius),
            ShapeBundle::new(
                GeometryBuilder::build_as(&shapes::Circle {
                    radius,
                    ..Default::default()
                }),
                Transform::from_translation(Vec3::new(0.0, 0.0, pos.z)),
                Visibility::Inherited,
            ),
            Position(pos.xy()),
            pos.z,
        );
        object.geometry.circle_visual = CircleVisual(radius);
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
                    extents: size.max(Vec2::splat(f32::EPSILON)),
                    origin: RectangleOrigin::Center,
                    radii: None,
                }),
                Transform::from_translation((size / 2.0).extend(pos.z)),
                Visibility::Inherited,
            ),
            Position(pos.xy() + size / 2.0),
            pos.z,
        )
    }

    pub fn freeform(points: &[Vec2], pos: Vec3) -> Option<Self> {
        let path = polygon_path(points, true);
        Self::freeform_path(path, pos, 0.0)
    }

    pub fn freeform_path(
        path: bevy_prototype_lyon::prelude::tess::path::Path,
        pos: Vec3,
        angle: f32,
    ) -> Option<Self> {
        let geometry = tessellate_path(&path)?;
        let mut object = Self::make(
            geometry.collider(),
            ShapeBundle::new(
                path,
                Transform::from_translation(pos).with_rotation(Quat::from_rotation_z(angle)),
                Visibility::Inherited,
            ),
            Position(pos.xy()),
            pos.z,
        );
        object.rotation = Rotation::radians(angle);
        Some(object)
    }

    pub fn spawn(self, commands: &mut Commands, scene: Entity) -> Entity {
        self.spawn_with_body(commands, scene).1
    }

    pub fn spawn_with_body(self, commands: &mut Commands, scene: Entity) -> (Entity, Entity) {
        let body = super::body::spawn(
            commands,
            scene,
            self.position,
            self.rotation,
            RigidBody::Dynamic,
        );
        let geometry = commands.spawn(self.geometry).id();
        super::body::attach(
            commands,
            geometry,
            body,
            scene,
            self.position,
            self.rotation,
            self.z,
        );
        (body, geometry)
    }

    pub(crate) fn fragment(world: &mut World, source: Entity, path: Path) -> Option<Entity> {
        let (pos, rotation) = super::body::pose(world, source)?;
        let scene = world.get::<ChildOf>(source)?.parent();
        let source_z = world.get::<Transform>(source)?.translation.z;
        let z = {
            let mut depth = world.resource_mut::<DepthSorter>();
            depth.include(source_z);
            depth.next()
        };
        let velocity = super::body::point_velocity(world, source)?;
        let angular = world
            .get::<ColliderOf>(source)
            .and_then(|link| world.get::<AngularVelocity>(link.body))?
            .0;
        let mut object = Self::freeform_path(path, pos.extend(z), rotation.as_radians())?;

        macro_rules! copy {
            ($field:ident: $ty:ty) => {
                if let Some(value) = world.get::<$ty>(source) {
                    object.geometry.$field = *value;
                }
            };
        }
        copy!(density: ColliderDensity);
        copy!(attraction: Attraction);
        if let Some(color) = world.get::<ColorComponent>(source) {
            object.geometry.color = ColorComponent(color.0);
        }
        macro_rules! property {
            ($field:ident: $ty:ty) => {
                if let Some(value) = world.get::<$ty>(source) {
                    object.geometry.properties.$field = *value;
                }
            };
        }
        property!(friction: Friction);
        property!(friction_model: FrictionModel);
        property!(restitution: Restitution);
        property!(groups: CollisionLayers);
        property!(refractive_index: RefractiveIndex);

        let center = object
            .geometry
            .collider
            .shape()
            .mass_properties(1.0)
            .local_com;
        let linear =
            super::body::velocity_at_point(pos, velocity, angular, pos + rotation * center);
        let mut commands = world.commands();
        let (body, geometry) = object.spawn_with_body(&mut commands, scene);
        commands.entity(geometry).insert(FreeformObject);
        commands
            .entity(body)
            .insert((LinearVelocity(linear), AngularVelocity(angular)));
        Some(geometry)
    }
}

pub(crate) fn set_box_geometry(collider: &mut Collider, shape: &mut Shape, size: Vec2) {
    let size = size.abs().max(Vec2::splat(f32::EPSILON));
    *collider = Collider::rectangle(size.x, size.y);
    shape.path = GeometryBuilder::build_as(&shapes::Rectangle {
        extents: size,
        origin: RectangleOrigin::Center,
        radii: None,
    });
}

pub(crate) fn set_circle_geometry(
    collider: &mut Collider,
    shape: &mut Shape,
    circle: &mut CircleVisual,
    radius: f32,
) {
    let radius = radius.abs().max(f32::EPSILON);
    *collider = Collider::circle(radius);
    shape.path = GeometryBuilder::build_as(&shapes::Circle {
        radius,
        ..Default::default()
    });
    circle.0 = radius;
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
    use crate::lyon_compat::StrokeAlignment;
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

        assert_eq!(object.geometry.density, ColliderDensity(2.0));
        assert_eq!(object.geometry.fill_stroke.stroke.width_px, 1.0);
        assert_eq!(
            object.geometry.fill_stroke.stroke.alignment,
            StrokeAlignment::Inward
        );
    }

    #[test]
    fn freeform_path_keeps_its_placement_orientation() {
        let path = polygon_path(&[Vec2::ZERO, Vec2::X, Vec2::Y], true);
        let object = PhysicalObject::freeform_path(path, Vec3::ZERO, 0.75).unwrap();

        assert!((object.rotation.as_radians() - 0.75).abs() < 1.0e-6);
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
            let marker_radius = circle.0;
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
