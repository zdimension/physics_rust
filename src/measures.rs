use crate::objects::spring::SpringObject;
use avian2d::prelude::*;
use bevy::{ecs::query::QueryData, prelude::*};

/*systems! {
    Forces::compute,
}*/

#[derive(Component)]
pub struct KineticEnergy {
    pub linear: f32,
    pub angular: f32,
}
// todo sometimes it crashes it we delete an entity during a frame because
// it tries to insert a component on a despawned entity
impl KineticEnergy {
    pub fn total(&self) -> f32 {
        self.linear + self.angular
    }
}

//pub type KineticData<'a> = (&'a ColliderMassProperties, &'a LinearVelocity, &'a AngularVelocity);

#[derive(QueryData)]
pub struct KineticData {
    pub mass: &'static ColliderMassProperties,
    pub linear: &'static LinearVelocity,
    pub angular: &'static AngularVelocity,
}

impl<'w, 's> KineticDataItem<'w, 's> {
    pub fn kinetic_energy(&self) -> KineticEnergy {
        KineticEnergy {
            linear: 0.5 * self.mass.mass * self.linear.0.length_squared(),
            angular: 0.5 * self.mass.angular_inertia * self.angular.0.powi(2),
        }
    }

    pub fn momentum(&self) -> Momentum {
        Momentum {
            linear: self.mass.mass * self.linear.0,
            angular: self.mass.angular_inertia * self.angular.0,
        }
    }
}

pub struct GravityEnergy {
    pub energy: f32,
}

#[derive(QueryData)]
pub struct GravityData {
    pub mass: &'static ColliderMassProperties,
    pub pos: &'static Position,
    pub gravity: &'static Gravity,
}

impl<'w, 's> GravityDataItem<'w, 's> {
    pub fn gravity_energy(&self) -> GravityEnergy {
        GravityEnergy {
            energy: -self.mass.mass * self.gravity.0.dot(self.pos.0),
        }
    }
}

#[derive(Component)]
pub struct Momentum {
    pub linear: Vec2,
    pub angular: f32,
}

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
    pub rigid_body: Option<&'static RigidBody>,
    pub position: Option<&'static Position>,
    pub rotation: Option<&'static Rotation>,
    pub mass: Option<&'static ColliderMassProperties>,
    pub linear: Option<&'static LinearVelocity>,
    pub angular: Option<&'static AngularVelocity>,
    pub spring: Option<&'static SpringObject>,
}

pub fn aggregate_measures(
    targets: impl IntoIterator<Item = Entity>,
    query: &Query<AggregateMeasureData>,
    body_positions: &Query<(&Position, &Rotation)>,
    gravity: Vec2,
) -> AggregateMeasures {
    let mut total_mass = 0.0;
    let mut total_inertia = 0.0;
    let mut weighted_pos = Vec2::ZERO;
    let mut weighted_vel = Vec2::ZERO;
    let mut weighted_ang_vel = 0.0;
    let mut linear_momentum = Vec2::ZERO;
    let mut angular_momentum = 0.0;
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

    for entity in targets {
        let Ok(item) = query.get(entity) else {
            continue;
        };

        if let (Some(_), Some(mass), Some(position), Some(rotation)) =
            (item.rigid_body, item.mass, item.position, item.rotation)
            && mass.mass > 0.0
        {
            has_mass = true;
            total_mass += mass.mass;
            total_inertia += mass.angular_inertia;
            let center_offset = *rotation * mass.center_of_mass;
            let center = position.0 + center_offset;
            weighted_pos += center * mass.mass;
            gravity_energy += -mass.mass * gravity.dot(center);
            has_gravity = true;

            if let Some(linear) = item.linear {
                let velocity = if let Some(angular) = item.angular {
                    linear.0 + Vec2::new(-center_offset.y, center_offset.x) * angular.0
                } else {
                    linear.0
                };
                has_velocity = true;
                weighted_vel += velocity * mass.mass;
                linear_momentum += mass.mass * velocity;
            }

            if let Some(angular) = item.angular {
                has_angular_velocity = true;
                weighted_ang_vel += angular.0 * mass.angular_inertia;
                angular_momentum += mass.angular_inertia * angular.0;
            }

            if let (Some(linear), Some(angular)) = (item.linear, item.angular) {
                has_kinetic = true;
                kinetic_linear += 0.5 * mass.mass * linear.0.length_squared();
                kinetic_angular += 0.5 * mass.angular_inertia * angular.0.powi(2);
            }
        }

        if let Some(spring) = item.spring {
            if let Some(energy) = spring.potential_energy(body_positions) {
                has_spring = true;
                spring_energy += energy;
            }
        }
    }

    AggregateMeasures {
        mass: has_mass.then_some(total_mass),
        angular_inertia: has_mass.then_some(total_inertia),
        position: (has_mass && total_mass > 0.0).then_some(weighted_pos / total_mass),
        velocity: (has_velocity && total_mass > 0.0).then_some(weighted_vel / total_mass),
        angular_velocity: (has_angular_velocity && total_inertia > 0.0)
            .then_some(weighted_ang_vel / total_inertia),
        momentum: (has_velocity || has_angular_velocity).then_some(MomentumValue {
            linear: linear_momentum,
            angular: angular_momentum,
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

pub enum ForceKind {
    Gravity,
    Torque,
}

pub enum ForceValue {
    Force(Vec2),
    Torque(f32),
}

impl From<Vec2> for ForceValue {
    fn from(f: Vec2) -> Self {
        ForceValue::Force(f)
    }
}

impl From<f32> for ForceValue {
    fn from(t: f32) -> Self {
        ForceValue::Torque(t)
    }
}

pub struct AppliedForce {
    pub kind: ForceKind,
    pub at: Vec2,
    pub value: ForceValue,
}

/*#[derive(Component)]
pub struct Forces {
    forces: Vec<AppliedForce>,
}

impl Forces {
    fn new() -> Self {
        Self { forces: Vec::new() }
    }

    pub(crate) fn compute(
        bodies: Query<(Entity, &Mass)>,
        changed_bodies: Query<(Entity, &Mass), Changed<Mass>>,
        mut commands: Commands,
        gravity: Res<Gravity>,
    ) {
        use ForceKind::*;

        let mut update = |id: Entity, mass: f32| {
            let Ok(mut cmds) = commands.get_entity(id) else {
                return;
            };
            cmds.insert(Forces {
                forces: vec![AppliedForce {
                    kind: Gravity,
                    at: Vec2::ZERO,
                    value: Vec2::new(0.0, mass * gravity.0.y).into(),
                }],
            });
        };

        if gravity.is_changed() {
            for (id, Mass(mass)) in bodies.iter() {
                update(id, *mass);
            }
        } else {
            for (id, Mass(mass)) in changed_bodies.iter() {
                update(id, *mass);
            }
        }
    }
}*/

/*pub fn forces(id: Entity, query: &Query<&Forces>) -> Option<&[AppliedForce]> {
    query.get(id).ok().map(|f| f.forces.as_slice())
}*/
