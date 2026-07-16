use crate::systems;
use bevy::{ecs::query::QueryData, prelude::*};
use avian2d::{math::*, prelude::*};

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
