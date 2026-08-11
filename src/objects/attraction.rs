use avian2d::prelude::*;
use bevy::prelude::*;

use super::phy_obj::PhysicalGeometry;

const MIN_ATTRACTION_DISTANCE: f32 = 1.0e-4;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum AttractionFalloff {
    Linear,
    #[default]
    Quadratic,
}

#[derive(Component, Copy, Clone, Debug)]
pub struct Attraction {
    pub strength: f32,
    pub falloff: AttractionFalloff,
}

impl Default for Attraction {
    fn default() -> Self {
        Self {
            strength: 0.0,
            falloff: AttractionFalloff::Quadratic,
        }
    }
}

#[derive(Component)]
struct ActiveAttractor;

#[derive(Copy, Clone)]
struct BodySample {
    entity: Entity,
    body: Entity,
    center: Vec2,
    mass: f32,
}

#[derive(Default)]
struct AttractionWorkspace {
    attractors: Vec<(Entity, Attraction)>,
    samples: Vec<BodySample>,
    accumulated: Vec<Vec2>,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AttractionForcesSet;

pub(crate) fn add_systems(app: &mut App) {
    app.add_systems(Update, sync_active_attractors).add_systems(
        PhysicsSchedule,
        apply_attraction_forces
            .in_set(PhysicsStepSystems::BroadPhase)
            .in_set(AttractionForcesSet)
            .after(crate::objects::thruster::apply_thruster_forces),
    );
}

fn sync_active_attractors(
    changed: Query<(Entity, &Attraction), Changed<Attraction>>,
    mut commands: Commands,
) {
    for (entity, attraction) in &changed {
        if attraction.strength != 0.0 && attraction.strength.is_finite() {
            commands.entity(entity).insert(ActiveAttractor);
        } else {
            commands.entity(entity).remove::<ActiveAttractor>();
        }
    }
}

fn apply_attraction_forces(
    attractors: Query<(Entity, &Attraction), (With<ActiveAttractor>, Without<RigidBodyDisabled>)>,
    geometries: Query<
        (
            Entity,
            &Position,
            &Rotation,
            &ColliderMassProperties,
            &ColliderOf,
        ),
        (With<PhysicalGeometry>, Without<RigidBodyDisabled>),
    >,
    mut forces: Query<Forces, (Without<RigidBodyDisabled>, Without<PhysicalGeometry>)>,
    mut workspace: Local<AttractionWorkspace>,
) {
    if attractors.is_empty() {
        return;
    }

    let AttractionWorkspace {
        attractors: active,
        samples,
        accumulated,
    } = &mut *workspace;
    active.clear();
    active.extend(
        attractors
            .iter()
            .filter(|(_, attraction)| attraction.strength != 0.0 && attraction.strength.is_finite())
            .map(|(entity, attraction)| (entity, *attraction)),
    );
    if active.is_empty() {
        return;
    }

    samples.clear();
    samples.extend(
        geometries
            .iter()
            .filter_map(|(entity, position, rotation, mass, link)| {
                (mass.mass > 0.0 && mass.mass.is_finite()).then_some(BodySample {
                    entity,
                    body: link.body,
                    center: position.0 + *rotation * mass.center_of_mass,
                    mass: mass.mass,
                })
            }),
    );
    accumulated.clear();
    accumulated.resize(samples.len(), Vec2::ZERO);

    for (source_entity, attraction) in active.iter().copied() {
        let Some(source_index) = samples
            .iter()
            .position(|sample| sample.entity == source_entity)
        else {
            continue;
        };
        let source = samples[source_index];

        for (target_index, target) in samples.iter().copied().enumerate() {
            if target.body == source.body {
                continue;
            }
            let force_on_target = attraction_force(source, target, attraction);
            accumulated[target_index] += force_on_target;
            accumulated[source_index] -= force_on_target;
        }
    }

    for (sample, force) in samples.iter().copied().zip(accumulated.iter().copied()) {
        if force != Vec2::ZERO
            && let Ok(mut body_forces) = forces.get_mut(sample.body)
        {
            body_forces.apply_force_at_point(force, sample.center);
        }
    }
}

fn attraction_force(source: BodySample, target: BodySample, attraction: Attraction) -> Vec2 {
    let delta = source.center - target.center;
    let distance_squared = delta.length_squared();
    if distance_squared <= f32::EPSILON || !distance_squared.is_finite() {
        return Vec2::ZERO;
    }

    let distance = distance_squared.sqrt();
    let softened_distance = distance.max(MIN_ATTRACTION_DISTANCE);
    let denominator = match attraction.falloff {
        AttractionFalloff::Linear => softened_distance,
        AttractionFalloff::Quadratic => softened_distance * softened_distance,
    };
    delta / distance * (attraction.strength * source.mass * target.mass / denominator)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(entity: u32, x: f32, mass: f32) -> BodySample {
        BodySample {
            entity: Entity::from_raw_u32(entity).unwrap(),
            body: Entity::from_raw_u32(entity).unwrap(),
            center: Vec2::new(x, 0.0),
            mass,
        }
    }

    fn magnitude(
        source_mass: f32,
        target_mass: f32,
        distance: f32,
        falloff: AttractionFalloff,
    ) -> f32 {
        attraction_force(
            sample(1, 0.0, source_mass),
            sample(2, distance, target_mass),
            Attraction {
                strength: 8.0,
                falloff,
            },
        )
        .length()
    }

    #[test]
    fn quadratic_attraction_matches_algodoo_measurements() {
        assert!((magnitude(1.0, 1.0, 2.0, AttractionFalloff::Quadratic) - 2.0).abs() < 1.0e-6);
        assert!((magnitude(2.0, 1.0, 2.0, AttractionFalloff::Quadratic) - 4.0).abs() < 1.0e-6);
        assert!((magnitude(1.0, 3.0, 2.0, AttractionFalloff::Quadratic) - 6.0).abs() < 1.0e-6);
        assert!((magnitude(1.0, 1.0, 4.0, AttractionFalloff::Quadratic) - 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn linear_attraction_matches_algodoo_measurement() {
        assert!((magnitude(1.0, 1.0, 2.0, AttractionFalloff::Linear) - 4.0).abs() < 1.0e-6);
    }

    #[test]
    fn two_attractors_add_their_mutual_contributions() {
        let a = sample(1, 0.0, 1.0);
        let b = sample(2, 2.0, 1.0);
        let force_from_a = attraction_force(
            a,
            b,
            Attraction {
                strength: 8.0,
                falloff: AttractionFalloff::Quadratic,
            },
        );
        let force_from_b = attraction_force(
            b,
            a,
            Attraction {
                strength: 4.0,
                falloff: AttractionFalloff::Quadratic,
            },
        );

        assert!((force_from_a.length() + force_from_b.length() - 3.0).abs() < 1.0e-6);
    }
}
