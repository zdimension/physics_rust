use avian2d::prelude::*;
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use super::{
    axle::{FixObject, JointGeometry},
    phy_obj::PhysicalGeometry,
    plane::PlaneObject,
    spring::{SpringEnd, SpringObject},
};
use crate::ui::SceneState;

pub(crate) fn world_point(pose: (Vec2, Rotation), local: Vec2) -> Vec2 {
    pose.0 + pose.1 * local
}

pub(crate) fn local_point(pose: (Vec2, Rotation), world: Vec2) -> Vec2 {
    pose.1.inverse() * (world - pose.0)
}

pub(crate) fn velocity_at_point(center: Vec2, linear: Vec2, angular: f32, point: Vec2) -> Vec2 {
    linear + (point - center).perp() * angular
}

#[derive(Component, Copy, Clone, Debug)]
pub struct PhysicsBody;

#[derive(Component, Copy, Clone, Debug, Deref, DerefMut)]
pub struct BodyTransform(pub ColliderTransform);

impl BodyTransform {
    pub fn transform_point(&self, point: Vec2) -> Vec2 {
        world_point((self.translation, self.rotation), point * self.scale)
    }

    pub fn set_world_pose(
        &mut self,
        collider: &mut ColliderTransform,
        body: (Vec2, Rotation),
        pose: (Vec2, Rotation),
    ) {
        self.translation = local_point(body, pose.0);
        self.rotation = body.1.inverse() * pose.1;
        *collider = self.0;
    }
}

#[derive(Resource, Default)]
pub struct WeldTopology {
    edges: Vec<(Entity, [Option<Entity>; 2])>,
}

fn bundle(pos: Vec2, rotation: Rotation, kind: RigidBody) -> impl Bundle {
    let transform = Transform::from_translation(pos.extend(0.0))
        .with_rotation(Quat::from_rotation_z(rotation.as_radians()));
    (
        PhysicsBody,
        kind,
        Position(pos),
        rotation,
        LinearVelocity::ZERO,
        AngularVelocity::ZERO,
        Mass(1.0),
        AngularInertia(1.0),
        CenterOfMass::ZERO,
        NoAutoMass,
        NoAutoAngularInertia,
        NoAutoCenterOfMass,
        SleepingDisabled,
        transform,
        GlobalTransform::from(transform),
    )
}

pub fn spawn(
    commands: &mut Commands,
    scene: Entity,
    pos: Vec2,
    rotation: Rotation,
    kind: RigidBody,
) -> Entity {
    commands
        .spawn((bundle(pos, rotation, kind), ChildOf(scene)))
        .id()
}

pub fn attach(
    commands: &mut Commands,
    geometry: Entity,
    body: Entity,
    scene: Entity,
    pos: Vec2,
    rotation: Rotation,
    z: f32,
) {
    let transform = Transform::from_translation(pos.extend(z))
        .with_rotation(Quat::from_rotation_z(rotation.as_radians()));
    commands.entity(geometry).insert((
        ColliderOf { body },
        BodyTransform(ColliderTransform::default()),
        ColliderTransform::default(),
        Position(pos),
        rotation,
        transform,
        GlobalTransform::from(transform),
        ChildOf(scene),
    ));
}

pub fn entity(world: &World, geometry: Entity) -> Option<Entity> {
    world.get::<ColliderOf>(geometry).map(|link| link.body)
}

pub fn pose(world: &World, geometry: Entity) -> Option<(Vec2, Rotation)> {
    let link = world.get::<ColliderOf>(geometry)?;
    let local = world.get::<BodyTransform>(geometry)?;
    let pos = world.get::<Position>(link.body)?;
    let rotation = world.get::<Rotation>(link.body)?;
    Some((
        world_point((pos.0, *rotation), local.translation),
        *rotation * local.rotation,
    ))
}

pub fn point_velocity(world: &World, geometry: Entity) -> Option<Vec2> {
    let body = entity(world, geometry)?;
    let (point, _) = pose(world, geometry)?;
    let center = world_point(
        (
            world.get::<Position>(body)?.0,
            *world.get::<Rotation>(body)?,
        ),
        world
            .get::<ComputedCenterOfMass>(body)
            .map_or(Vec2::ZERO, |center| center.0),
    );
    let linear = world.get::<LinearVelocity>(body)?.0;
    let angular = world.get::<AngularVelocity>(body)?.0;
    Some(velocity_at_point(center, linear, angular, point))
}

pub fn set_point_velocity(world: &mut World, geometry: Entity, velocity: Vec2) -> bool {
    let Some(body) = entity(world, geometry) else {
        return false;
    };
    let Some((point, _)) = pose(world, geometry) else {
        return false;
    };
    let body_pos = world.get::<Position>(body).map_or(Vec2::ZERO, |pos| pos.0);
    let body_rotation = world.get::<Rotation>(body).copied().unwrap_or_default();
    let center = world_point(
        (body_pos, body_rotation),
        world
            .get::<ComputedCenterOfMass>(body)
            .map_or(Vec2::ZERO, |center| center.0),
    );
    let angular = world
        .get::<AngularVelocity>(body)
        .map_or(0.0, |velocity| velocity.0);
    let Some(mut linear) = world.get_mut::<LinearVelocity>(body) else {
        return false;
    };
    linear.0 = velocity_at_point(center, velocity, -angular, point);
    true
}

pub fn set_pose(world: &mut World, geometry: Entity, pos: Vec2, rotation: Rotation) -> bool {
    let Some(body) = entity(world, geometry) else {
        return false;
    };
    let Some(body_pos) = world.get::<Position>(body).copied() else {
        return false;
    };
    let Some(body_rotation) = world.get::<Rotation>(body).copied() else {
        return false;
    };
    let mut local = BodyTransform(ColliderTransform {
        scale: world
            .get::<BodyTransform>(geometry)
            .map_or(Vec2::ONE, |transform| transform.scale),
        ..default()
    });
    let mut collider = local.0;
    local.set_world_pose(&mut collider, (body_pos.0, body_rotation), (pos, rotation));
    let z = world
        .get::<Transform>(geometry)
        .map_or(0.0, |transform| transform.translation.z);
    world
        .entity_mut(geometry)
        .insert((local, collider, Position(pos), rotation));
    let Some(mut transform) = world.get_mut::<Transform>(geometry) else {
        return false;
    };
    transform.translation = pos.extend(z);
    transform.rotation = Quat::from_rotation_z(rotation.as_radians());
    true
}

#[derive(Clone, Copy)]
struct GeometryState {
    entity: Entity,
    body: Entity,
    pos: Vec2,
    rotation: Rotation,
    mass: f32,
    inertia: f32,
    center: Vec2,
    velocity: Vec2,
    angular_velocity: f32,
}

fn root(parents: &mut HashMap<Entity, Entity>, entity: Entity) -> Entity {
    let parent = parents[&entity];
    if parent == entity {
        entity
    } else {
        let root = root(parents, parent);
        parents.insert(entity, root);
        root
    }
}

fn join(parents: &mut HashMap<Entity, Entity>, a: Entity, b: Entity) {
    let (a, b) = (root(parents, a), root(parents, b));
    if a != b {
        parents.insert(b, a);
    }
}

fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

fn despawn_unused_bodies(world: &mut World, used: &HashSet<Entity>) {
    let sky = world.resource::<SceneState>().sky;
    let unused = world
        .query_filtered::<Entity, With<PhysicsBody>>()
        .iter(world)
        .filter(|entity| *entity != sky && !used.contains(entity))
        .collect::<Vec<_>>();
    for body in unused {
        world.despawn(body);
    }
}

/// Rebuilds compound bodies only when the logical fixjoint graph changes.
pub fn rebuild_welds(world: &mut World) {
    let geometries = world
        .query_filtered::<Entity, With<PhysicalGeometry>>()
        .iter(world)
        .collect::<Vec<_>>();
    let geometry_set = geometries.iter().copied().collect::<HashSet<_>>();
    let stale_joints = world
        .query::<(
            Entity,
            &JointGeometry,
            Option<&crate::tools::add_object::AttachmentLinks>,
        )>()
        .iter(world)
        .filter(|(_, joint, _)| {
            joint.geoms == [None, None]
                || joint
                    .geoms
                    .iter()
                    .flatten()
                    .any(|entity| !geometry_set.contains(entity))
        })
        .map(|(entity, _, links)| (entity, links.and_then(|links| links.joint)))
        .collect::<Vec<_>>();
    for (visual, physical) in stale_joints {
        if let Some(physical) = physical {
            world.despawn(physical);
        }
        world.despawn(visual);
    }
    let stale_springs = world
        .query::<(Entity, &SpringObject)>()
        .iter(world)
        .filter(|(_, spring)| {
            [spring.end_a, spring.end_b].iter().any(|end| {
                matches!(end, SpringEnd::Body { entity, .. } if !geometry_set.contains(entity))
            })
        })
        .map(|(entity, _)| entity)
        .collect::<Vec<_>>();
    for entity in stale_springs {
        world.despawn(entity);
    }
    let orphaned_joints = world
        .query::<(Entity, &crate::tools::add_object::AttachmentJoint)>()
        .iter(world)
        .filter(|(_, joint)| world.get_entity(joint.visual).is_err())
        .map(|(entity, _)| entity)
        .collect::<Vec<_>>();
    for entity in orphaned_joints {
        world.despawn(entity);
    }
    let mut edges = world
        .query_filtered::<(Entity, &JointGeometry), With<FixObject>>()
        .iter(world)
        .map(|(entity, geometry)| (entity, geometry.geoms))
        .collect::<Vec<_>>();
    edges.sort_by_key(|(entity, _)| entity.index());
    if world.resource::<WeldTopology>().edges == edges {
        let used = geometries
            .iter()
            .filter_map(|entity| world.get::<ColliderOf>(*entity).map(|link| link.body))
            .collect();
        despawn_unused_bodies(world, &used);
        return;
    }
    world.resource_mut::<WeldTopology>().edges = edges.clone();

    let sky = world.resource::<SceneState>().sky;
    let scene = world.resource::<SceneState>().scene;
    let mut parents = geometries
        .iter()
        .copied()
        .map(|entity| (entity, entity))
        .collect::<HashMap<_, _>>();
    let mut sky_geometries = HashSet::new();
    for (_, edge) in &edges {
        match *edge {
            [Some(a), Some(b)] if parents.contains_key(&a) && parents.contains_key(&b) => {
                join(&mut parents, a, b);
            }
            [Some(a), None] | [None, Some(a)] if parents.contains_key(&a) => {
                sky_geometries.insert(a);
            }
            _ => {}
        }
    }

    let mut states = HashMap::new();
    for entity in geometries {
        let Some(link) = world.get::<ColliderOf>(entity) else {
            continue;
        };
        let Some(pos) = world.get::<Position>(entity).copied() else {
            continue;
        };
        let rotation = world.get::<Rotation>(entity).copied().unwrap_or_default();
        let mass = ColliderMassProperties::from_shape(
            world.get::<Collider>(entity).unwrap(),
            world.get::<ColliderDensity>(entity).map_or(1.0, |d| d.0),
        );
        let center = world_point((pos.0, rotation), mass.center_of_mass);
        states.insert(
            entity,
            GeometryState {
                entity,
                body: link.body,
                pos: pos.0,
                rotation,
                mass: mass.mass,
                inertia: mass.angular_inertia,
                center,
                velocity: Vec2::ZERO,
                angular_velocity: 0.0,
            },
        );
    }

    let mut old_centers: HashMap<Entity, (f32, Vec2)> = HashMap::new();
    for state in states.values() {
        let center = old_centers.entry(state.body).or_default();
        center.0 += state.mass;
        center.1 += state.center * state.mass;
    }
    for state in states.values_mut() {
        let (mass, weighted_center) = old_centers[&state.body];
        let body_center = if mass > 0.0 {
            weighted_center / mass
        } else {
            world
                .get::<Position>(state.body)
                .map_or(state.pos, |pos| pos.0)
        };
        state.angular_velocity = world
            .get::<AngularVelocity>(state.body)
            .map_or(0.0, |velocity| velocity.0);
        state.velocity = velocity_at_point(
            body_center,
            world
                .get::<LinearVelocity>(state.body)
                .map_or(Vec2::ZERO, |velocity| velocity.0),
            state.angular_velocity,
            state.center,
        );
    }

    let mut components: HashMap<Entity, Vec<GeometryState>> = HashMap::new();
    for (&entity, state) in &states {
        components
            .entry(root(&mut parents, entity))
            .or_default()
            .push(*state);
    }
    let sky_roots = sky_geometries
        .into_iter()
        .map(|entity| root(&mut parents, entity))
        .collect::<HashSet<_>>();
    let mut used = HashSet::new();
    let mut components = components.into_iter().collect::<Vec<_>>();
    components.sort_by_key(|(root, _)| root.index());

    for (root, mut members) in components {
        members.sort_by_key(|member| member.entity.index());
        let static_body = sky_roots.contains(&root)
            || members
                .iter()
                .any(|member| world.get::<PlaneObject>(member.entity).is_some());
        let total_mass = members.iter().map(|m| m.mass).sum::<f32>();
        let center = if total_mass > 0.0 {
            members.iter().map(|m| m.center * m.mass).sum::<Vec2>() / total_mass
        } else {
            members[0].pos
        };
        let momentum = members.iter().map(|m| m.velocity * m.mass).sum::<Vec2>();
        let inertia = members
            .iter()
            .map(|m| m.inertia + m.mass * m.center.distance_squared(center))
            .sum::<f32>();
        let angular_momentum = members
            .iter()
            .map(|m| m.inertia * m.angular_velocity + cross(m.center - center, m.velocity * m.mass))
            .sum::<f32>();
        let velocity = if total_mass > 0.0 {
            momentum / total_mass
        } else {
            Vec2::ZERO
        };
        let angular_velocity = if inertia > 0.0 {
            angular_momentum / inertia
        } else {
            0.0
        };

        let body = if static_body {
            sky
        } else if let Some(body) = members
            .iter()
            .map(|member| member.body)
            .find(|body| *body != sky && !used.contains(body))
        {
            body
        } else {
            world
                .spawn((
                    bundle(center, Rotation::default(), RigidBody::Dynamic),
                    ChildOf(scene),
                ))
                .id()
        };
        used.insert(body);
        if body != sky {
            let transform = Transform::from_translation(center.extend(0.0));
            world.entity_mut(body).insert((
                RigidBody::Dynamic,
                Position(center),
                Rotation::default(),
                LinearVelocity(velocity),
                AngularVelocity(angular_velocity),
                transform,
                GlobalTransform::from(transform),
            ));
        }

        let (body_pos, body_rotation) = if body == sky {
            (
                world.get::<Position>(sky).unwrap().0,
                *world.get::<Rotation>(sky).unwrap(),
            )
        } else {
            (center, Rotation::default())
        };
        for member in members {
            let local = ColliderTransform {
                translation: local_point((body_pos, body_rotation), member.pos),
                rotation: body_rotation.inverse() * member.rotation,
                scale: Vec2::ONE,
            };
            world.entity_mut(member.entity).insert((
                ColliderOf { body },
                BodyTransform(local),
                local,
            ));
        }
    }

    despawn_unused_bodies(world, &used);
    crate::tools::add_object::rebuild_hinges(world);
}

pub fn sync_mass_properties(
    changed: Query<
        &ColliderOf,
        (
            With<PhysicalGeometry>,
            Or<(
                Changed<Collider>,
                Changed<ColliderDensity>,
                Changed<BodyTransform>,
                Changed<ColliderOf>,
            )>,
        ),
    >,
    colliders: Query<(&Collider, &ColliderDensity, &BodyTransform)>,
    mut bodies: Query<(
        Entity,
        Ref<RigidBodyColliders>,
        &mut Mass,
        &mut AngularInertia,
        &mut CenterOfMass,
    )>,
) {
    let dirty = changed.iter().map(|link| link.body).collect::<HashSet<_>>();
    for (entity, children, mut mass, mut inertia, mut center) in &mut bodies {
        if !children.is_changed() && !dirty.contains(&entity) {
            continue;
        }
        let parts = children
            .iter()
            .filter_map(|entity| {
                let (collider, density, local) = colliders.get(entity).ok()?;
                let props = ColliderMassProperties::from_shape(collider, density.0);
                (props.mass > 0.0 && props.mass.is_finite()).then_some((
                    props.mass,
                    world_point((local.translation, local.rotation), props.center_of_mass),
                    props.angular_inertia,
                ))
            })
            .collect::<Vec<_>>();
        let total = parts.iter().map(|part| part.0).sum::<f32>();
        if total <= 0.0 {
            continue;
        }
        let com = parts
            .iter()
            .map(|(mass, center, _)| *mass * *center)
            .sum::<Vec2>()
            / total;
        let angular = parts
            .iter()
            .map(|(mass, center, inertia)| inertia + mass * center.distance_squared(com))
            .sum();
        mass.0 = total;
        inertia.0 = angular;
        center.0 = com;
    }
}

pub fn sync_transforms(
    bodies: Query<(&Position, &Rotation), With<PhysicsBody>>,
    mut geometries: Query<
        (
            &ColliderOf,
            &BodyTransform,
            &mut ColliderTransform,
            &mut Position,
            &mut Rotation,
            &mut Transform,
        ),
        Without<PhysicsBody>,
    >,
) {
    for (link, local, mut collider_transform, mut position, mut geometry_rotation, mut transform) in
        &mut geometries
    {
        let Ok((pos, rotation)) = bodies.get(link.body) else {
            continue;
        };
        let world_pos = world_point((pos.0, *rotation), local.translation);
        let world_rotation = *rotation * local.rotation;
        *collider_transform = local.0;
        position.0 = world_pos;
        *geometry_rotation = world_rotation;
        transform.translation.x = world_pos.x;
        transform.translation.y = world_pos.y;
        transform.rotation = Quat::from_rotation_z(world_rotation.as_radians());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::phy_obj::PhysicalObject;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::ecs::world::CommandQueue;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<SceneState>();
        world.init_resource::<WeldTopology>();
        world
    }

    fn circle(world: &mut World, pos: Vec2) -> Entity {
        let scene = world.resource::<SceneState>().scene;
        let mut queue = CommandQueue::default();
        let entity = PhysicalObject::ball(1.0, pos.extend(0.0))
            .spawn(&mut Commands::new(&mut queue, world), scene);
        queue.apply(world);
        entity
    }

    fn weld(world: &mut World, geoms: [Option<Entity>; 2]) -> Entity {
        world
            .spawn((
                FixObject,
                JointGeometry {
                    geoms,
                    positions: [Vec2::ZERO; 2],
                },
            ))
            .id()
    }

    #[test]
    fn weld_merges_bodies_and_conserves_linear_and_angular_momentum() {
        let mut world = world();
        let (a, b) = (circle(&mut world, -Vec2::X), circle(&mut world, Vec2::X));
        world
            .get_mut::<LinearVelocity>(entity(&world, a).unwrap())
            .unwrap()
            .0 = Vec2::Y;
        world
            .get_mut::<LinearVelocity>(entity(&world, b).unwrap())
            .unwrap()
            .0 = -Vec2::Y;
        weld(&mut world, [Some(a), Some(b)]);

        rebuild_welds(&mut world);

        let body = entity(&world, a).unwrap();
        assert_eq!(entity(&world, b), Some(body));
        assert_eq!(
            world.get::<Transform>(body).unwrap().translation.truncate(),
            world.get::<Position>(body).unwrap().0
        );
        assert_eq!(world.get::<LinearVelocity>(body).unwrap().0, Vec2::ZERO);
        let angular = world.get::<AngularVelocity>(body).unwrap().0;
        assert!((angular + 2.0 / 3.0).abs() < 1e-5, "{angular}");
        assert_eq!(pose(&world, a).unwrap().0, -Vec2::X);
        assert_eq!(pose(&world, b).unwrap().0, Vec2::X);
        assert_eq!(world.query::<&FixedJoint>().iter(&world).count(), 0);
    }

    #[test]
    fn weld_uses_current_collider_centers_when_mass_cache_is_stale() {
        let mut world = world();
        let (a, b) = (
            circle(&mut world, Vec2::ZERO),
            circle(&mut world, Vec2::ZERO),
        );
        assert!(set_pose(&mut world, a, -Vec2::X * 2.0, Rotation::default()));
        assert!(set_pose(&mut world, b, Vec2::X * 2.0, Rotation::default()));
        let old_bodies = [entity(&world, a).unwrap(), entity(&world, b).unwrap()];
        for body in old_bodies {
            world.get_mut::<AngularVelocity>(body).unwrap().0 = 1.0;
        }
        weld(&mut world, [Some(a), Some(b)]);

        rebuild_welds(&mut world);

        let body = entity(&world, a).unwrap();
        assert_eq!(entity(&world, b), Some(body));
        assert!((world.get::<AngularVelocity>(body).unwrap().0 - 1.0 / 9.0).abs() < 1.0e-5);
        assert!(world.get::<LinearVelocity>(body).unwrap().0.length() < 1.0e-5);
    }

    #[test]
    fn removing_a_weld_splits_bodies_without_changing_point_velocity() {
        let mut world = world();
        let (a, b) = (circle(&mut world, -Vec2::X), circle(&mut world, Vec2::X));
        let edge = weld(&mut world, [Some(a), Some(b)]);
        rebuild_welds(&mut world);
        let velocities = [point_velocity(&world, a), point_velocity(&world, b)];

        world.despawn(edge);
        rebuild_welds(&mut world);

        assert_ne!(entity(&world, a), entity(&world, b));
        assert_eq!(point_velocity(&world, a), velocities[0]);
        assert_eq!(point_velocity(&world, b), velocities[1]);
    }

    #[test]
    fn standalone_changes_do_not_rebuild_compounds() {
        let mut world = world();
        let (a, b) = (circle(&mut world, -Vec2::X), circle(&mut world, Vec2::X));
        weld(&mut world, [Some(a), Some(b)]);
        rebuild_welds(&mut world);
        let compound = entity(&world, a).unwrap();
        *world.get_mut::<Rotation>(compound).unwrap() = Rotation::radians(0.7);

        let standalone = circle(&mut world, Vec2::splat(4.0));
        let standalone_body = entity(&world, standalone).unwrap();
        rebuild_welds(&mut world);
        assert_eq!(world.get::<Rotation>(compound).unwrap().as_radians(), 0.7);

        world.despawn(standalone);
        rebuild_welds(&mut world);
        assert!(world.get_entity(standalone_body).is_err());
        assert_eq!(world.get::<Rotation>(compound).unwrap().as_radians(), 0.7);
    }

    #[test]
    fn removing_a_sky_weld_restores_a_dynamic_body() {
        let mut world = world();
        let geometry = circle(&mut world, Vec2::new(3.0, 4.0));
        let edge = weld(&mut world, [Some(geometry), None]);
        let sky = world.resource::<SceneState>().sky;
        rebuild_welds(&mut world);
        assert_eq!(entity(&world, geometry), Some(sky));

        world.despawn(edge);
        rebuild_welds(&mut world);

        let body = entity(&world, geometry).unwrap();
        assert_ne!(body, sky);
        assert_eq!(world.get::<RigidBody>(body), Some(&RigidBody::Dynamic));
        assert_eq!(pose(&world, geometry).unwrap().0, Vec2::new(3.0, 4.0));
        assert_eq!(
            world.get::<Transform>(body).unwrap().translation.truncate(),
            world.get::<Position>(body).unwrap().0
        );
    }

    #[test]
    fn hidden_bodies_aggregate_their_linked_colliders() {
        let mut world = world();
        let geometry = circle(&mut world, Vec2::ZERO);
        world.run_system_once(sync_mass_properties).unwrap();

        let body = entity(&world, geometry).unwrap();
        assert!((world.get::<Mass>(body).unwrap().0 - 2.0 * std::f32::consts::PI).abs() < 1e-5);
        assert!(world.get::<NoAutoMass>(body).is_some());
    }

    #[test]
    fn cycles_only_split_when_connectivity_breaks() {
        let mut world = world();
        let geoms = [
            circle(&mut world, Vec2::ZERO),
            circle(&mut world, Vec2::X),
            circle(&mut world, Vec2::Y),
        ];
        let edges = [
            weld(&mut world, [Some(geoms[0]), Some(geoms[1])]),
            weld(&mut world, [Some(geoms[1]), Some(geoms[2])]),
            weld(&mut world, [Some(geoms[2]), Some(geoms[0])]),
        ];
        rebuild_welds(&mut world);
        world.despawn(edges[0]);
        rebuild_welds(&mut world);
        assert_eq!(entity(&world, geoms[0]), entity(&world, geoms[1]));
        world.despawn(edges[1]);
        rebuild_welds(&mut world);
        assert_ne!(entity(&world, geoms[0]), entity(&world, geoms[1]));
    }

    #[test]
    fn sync_restores_the_body_local_collider_transform() {
        let mut world = world();
        let geometry = circle(&mut world, Vec2::X);
        world
            .get_mut::<ColliderTransform>(geometry)
            .unwrap()
            .translation = Vec2::splat(99.0);

        world.run_system_once(sync_transforms).unwrap();

        assert_eq!(
            *world.get::<ColliderTransform>(geometry).unwrap(),
            world.get::<BodyTransform>(geometry).unwrap().0
        );
    }

    #[test]
    fn setting_geometry_pose_updates_both_local_transforms_immediately() {
        let mut world = world();
        let geometry = circle(&mut world, Vec2::ZERO);
        let rotation = Rotation::radians(0.7);

        assert!(set_pose(
            &mut world,
            geometry,
            Vec2::new(3.0, 4.0),
            rotation
        ));

        assert_eq!(
            world.get::<BodyTransform>(geometry).unwrap().0,
            *world.get::<ColliderTransform>(geometry).unwrap()
        );
        assert_eq!(
            world.get::<Position>(geometry).unwrap().0,
            Vec2::new(3.0, 4.0)
        );
        assert_eq!(*world.get::<Rotation>(geometry).unwrap(), rotation);
    }

    #[test]
    fn pose_system_queries_are_disjoint() {
        let mut app = App::new();
        app.add_message::<crate::tools::r#move::MoveEvent>()
            .add_message::<crate::tools::rotate::RotateEvent>()
            .add_systems(
                Update,
                (
                    sync_transforms,
                    crate::tools::r#move::process_move,
                    crate::tools::rotate::process_rotate,
                    crate::objects::spring::apply_spring_forces,
                ),
            );
        app.update();
    }
}
