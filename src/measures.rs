use crate::objects::{
    body::{velocity_at_point, world_point},
    plane::PlaneObject,
    spring::SpringObject,
};
use avian2d::prelude::*;
use bevy::{ecs::query::QueryData, prelude::*};

#[derive(Default, Clone, Debug)]
pub struct AggregateMeasures {
    pub mass: Option<f32>,
    pub angular_inertia: Option<f32>,
    pub position: Option<Vec2>,
    pub velocity: Option<Vec2>,
    pub angular_velocity: Option<f32>,
    pub momentum: Option<MomentumValue>,
    pub kinetic_linear: Option<f32>,
    pub kinetic_angular: Option<f32>,
    pub gravity_energy: Option<f32>,
    pub spring_energy: Option<f32>,
}

impl AggregateMeasures {
    pub fn kinetic_total(&self) -> Option<f32> {
        sum_if_any([self.kinetic_linear, self.kinetic_angular])
    }

    pub fn potential_total(&self) -> Option<f32> {
        sum_if_any([self.gravity_energy, self.spring_energy])
    }

    pub fn energy_total(&self) -> Option<f32> {
        sum_if_any([
            self.kinetic_linear,
            self.kinetic_angular,
            self.gravity_energy,
            self.spring_energy,
        ])
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MomentumValue {
    pub linear: Vec2,
    pub angular: f32,
}

#[derive(QueryData)]
pub struct AggregateMeasureData {
    pub body: Option<&'static ColliderOf>,
    pub position: Option<&'static Position>,
    pub rotation: Option<&'static Rotation>,
    pub mass: Option<&'static ColliderMassProperties>,
    pub spring: Option<&'static SpringObject>,
    pub plane: Option<&'static PlaneObject>,
}

#[derive(Clone, Copy)]
struct MassSample {
    mass: f32,
    inertia: f32,
    center: Vec2,
    motion: Option<(Vec2, f32)>,
}

pub fn aggregate_measures(
    targets: impl IntoIterator<Item = Entity>,
    query: &Query<AggregateMeasureData>,
    body_positions: &Query<(&Position, &Rotation)>,
    bodies: &Query<(
        &Position,
        &Rotation,
        &ComputedCenterOfMass,
        &LinearVelocity,
        &AngularVelocity,
    )>,
    gravity: Vec2,
) -> AggregateMeasures {
    let mut total_mass = 0.0;
    let mut weighted_pos = Vec2::ZERO;
    let mut weighted_vel = Vec2::ZERO;
    let mut weighted_ang_vel = 0.0;
    let mut intrinsic_inertia = 0.0;
    let mut linear_momentum = Vec2::ZERO;
    let mut kinetic_linear = 0.0;
    let mut kinetic_angular = 0.0;
    let mut gravity_energy = 0.0;
    let mut spring_energy = 0.0;
    let mut has_mass = false;
    let mut has_velocity = false;
    let mut has_angular_velocity = false;
    let mut has_kinetic = false;
    let mut has_gravity = false;
    let mut has_spring = false;
    let mut plane_position_sum = Vec2::ZERO;
    let mut plane_count = 0usize;
    let mut samples = Vec::new();

    for entity in targets {
        let Ok(item) = query.get(entity) else {
            continue;
        };

        if item.plane.is_some() {
            if let Some(position) = item.position {
                plane_position_sum += position.0;
                plane_count += 1;
            }
        } else if let (Some(link), Some(mass), Some(position), Some(rotation)) =
            (item.body, item.mass, item.position, item.rotation)
            && mass.mass > 0.0
        {
            has_mass = true;
            total_mass += mass.mass;
            let center = world_point((position.0, *rotation), mass.center_of_mass);
            weighted_pos += center * mass.mass;
            gravity_energy += -mass.mass * gravity.dot(center);
            has_gravity = true;

            let motion = if let Ok((body_pos, body_rotation, body_center, linear, angular)) =
                bodies.get(link.body)
            {
                let body_center = world_point((body_pos.0, *body_rotation), body_center.0);
                let velocity = velocity_at_point(body_center, linear.0, angular.0, center);
                has_velocity = true;
                weighted_vel += velocity * mass.mass;
                linear_momentum += mass.mass * velocity;
                has_angular_velocity = true;
                weighted_ang_vel += angular.0 * mass.angular_inertia;
                intrinsic_inertia += mass.angular_inertia;
                has_kinetic = true;
                kinetic_linear += 0.5 * mass.mass * velocity.length_squared();
                kinetic_angular += 0.5 * mass.angular_inertia * angular.0.powi(2);
                Some((velocity, angular.0))
            } else {
                None
            };
            samples.push(MassSample {
                mass: mass.mass,
                inertia: mass.angular_inertia,
                center,
                motion,
            });
        }

        if let Some(spring) = item.spring {
            if let Some(energy) = spring.potential_energy(body_positions) {
                has_spring = true;
                spring_energy += energy;
            }
        }
    }

    let center = (has_mass && total_mass > 0.0).then(|| weighted_pos / total_mass);
    let total_inertia = center.map(|center| {
        samples
            .iter()
            .map(|sample| sample.inertia + sample.mass * sample.center.distance_squared(center))
            .sum()
    });
    let angular_momentum = center.map(|center| {
        samples
            .iter()
            .filter_map(|sample| {
                let (velocity, angular) = sample.motion?;
                Some(
                    sample.inertia * angular
                        + (sample.center - center).perp_dot(sample.mass * velocity),
                )
            })
            .sum()
    });

    AggregateMeasures {
        mass: has_mass.then_some(total_mass),
        angular_inertia: total_inertia,
        position: center
            .or_else(|| (plane_count > 0).then_some(plane_position_sum / plane_count as f32)),
        velocity: (has_velocity && total_mass > 0.0).then_some(weighted_vel / total_mass),
        angular_velocity: (has_angular_velocity && intrinsic_inertia > 0.0)
            .then_some(weighted_ang_vel / intrinsic_inertia),
        momentum: (has_velocity || has_angular_velocity).then_some(MomentumValue {
            linear: linear_momentum,
            angular: angular_momentum.unwrap_or_default(),
        }),
        kinetic_linear: has_kinetic.then_some(kinetic_linear),
        kinetic_angular: has_kinetic.then_some(kinetic_angular),
        gravity_energy: has_gravity.then_some(gravity_energy),
        spring_energy: has_spring.then_some(spring_energy),
    }
}

fn sum_if_any(items: impl IntoIterator<Item = Option<f32>>) -> Option<f32> {
    let mut sum = 0.0;
    let mut any = false;
    for item in items {
        if let Some(item) = item {
            sum += item;
            any = true;
        }
    }
    any.then_some(sum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::SystemState;

    fn aggregate(
        world: &mut World,
        targets: impl IntoIterator<Item = Entity>,
    ) -> AggregateMeasures {
        let mut state: SystemState<(
            Query<AggregateMeasureData>,
            Query<(&Position, &Rotation)>,
            Query<(
                &Position,
                &Rotation,
                &ComputedCenterOfMass,
                &LinearVelocity,
                &AngularVelocity,
            )>,
        )> = SystemState::new(world);
        let (measures, positions, bodies) = state.get(world).unwrap();
        aggregate_measures(targets, &measures, &positions, &bodies, Vec2::ZERO)
    }

    fn body(world: &mut World, position: Vec2, angular_velocity: f32) -> Entity {
        world
            .spawn((
                RigidBody::Dynamic,
                Position(position),
                Rotation::default(),
                ComputedCenterOfMass::default(),
                LinearVelocity::ZERO,
                AngularVelocity(angular_velocity),
            ))
            .id()
    }

    fn circle(world: &mut World, body: Entity, position: Vec2) -> Entity {
        world
            .spawn((
                ColliderOf { body },
                Position(position),
                Rotation::default(),
                ColliderMassProperties::from_shape(&Collider::circle(1.0), 1.0),
            ))
            .id()
    }

    #[test]
    fn plane_exposes_position_but_not_mass_or_energy() {
        let mut world = World::new();
        let position = Vec2::new(3.0, -2.0);
        let plane = world
            .spawn((
                PlaneObject,
                RigidBody::Static,
                Position(position),
                Rotation::default(),
                ColliderMassProperties::from_shape(&Collider::circle(2.0), 1.0),
                LinearVelocity(Vec2::X),
                AngularVelocity(2.0),
            ))
            .id();

        let aggregate = aggregate(&mut world, [plane]);

        assert_eq!(aggregate.position, Some(position));
        assert!(aggregate.mass.is_none());
        assert!(aggregate.angular_inertia.is_none());
        assert!(aggregate.velocity.is_none());
        assert!(aggregate.kinetic_total().is_none());
        assert!(aggregate.gravity_energy.is_none());
    }

    #[test]
    fn aggregate_angular_measures_match_algodoo() {
        let mut world = World::new();
        let shared = body(&mut world, Vec2::ZERO, 1.0);
        let welded = [
            circle(&mut world, shared, -Vec2::X * 2.0),
            circle(&mut world, shared, Vec2::X * 2.0),
        ];
        let welded = aggregate(&mut world, welded);
        let pi = std::f32::consts::PI;
        assert!((welded.angular_inertia.unwrap() - 9.0 * pi).abs() < 1.0e-4);
        assert_eq!(welded.angular_velocity, Some(1.0));
        assert!((welded.momentum.unwrap().angular - 9.0 * pi).abs() < 1.0e-4);
        assert!((welded.kinetic_linear.unwrap() - 4.0 * pi).abs() < 1.0e-4);
        assert!((welded.kinetic_angular.unwrap() - 0.5 * pi).abs() < 1.0e-4);

        let separate_bodies = [
            body(&mut world, -Vec2::X * 2.0, 1.0),
            body(&mut world, Vec2::X * 2.0, 1.0),
        ];
        let separate = [
            circle(&mut world, separate_bodies[0], -Vec2::X * 2.0),
            circle(&mut world, separate_bodies[1], Vec2::X * 2.0),
        ];
        let separate = aggregate(&mut world, separate);
        assert!((separate.angular_inertia.unwrap() - 9.0 * pi).abs() < 1.0e-4);
        assert_eq!(separate.angular_velocity, Some(1.0));
        assert!((separate.momentum.unwrap().angular - pi).abs() < 1.0e-4);
        assert_eq!(separate.kinetic_linear, Some(0.0));
        assert!((separate.kinetic_angular.unwrap() - 0.5 * pi).abs() < 1.0e-4);
    }
}
