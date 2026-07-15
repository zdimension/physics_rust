use crate::systems;
use bevy::prelude::*;
use avian2d::{math::*, prelude::*};

systems! {
    compute_motion_measures,
    GravityEnergy::compute,
    Forces::compute,
}

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

fn compute_motion_measures(
    bodies: Query<
        (Entity, &ColliderMassProperties, &LinearVelocity, &AngularVelocity),
        Or<(
            Changed<ColliderMassProperties>,
            Changed<LinearVelocity>,
            Changed<AngularVelocity>,
        )>,
    >,
    mut commands: Commands,
) {
    for (id, mass, lin, ang) in bodies.iter() {
        let Ok(mut cmds) = commands.get_entity(id) else {
            continue;
        };
        cmds.insert((
            KineticEnergy {
                linear: mass.mass * lin.0.length_squared() / 2.0,
                angular: mass.angular_inertia * ang.0 * ang.0 / 2.0,
            },
            Momentum {
                linear: mass.mass * lin.0,
                angular: mass.angular_inertia * ang.0,
            },
        ));
    }
}

#[derive(Component)]
pub struct GravityEnergy {
    pub energy: f32,
}

impl GravityEnergy {
    pub(crate) fn compute(
        bodies: Query<(Entity, &Mass, &Position)>,
        changed_bodies: Query<(Entity, &Mass, &Position), Or<(Changed<Mass>, Changed<Position>)>>,
        gravity: Res<Gravity>,
        mut commands: Commands,
    ) {
        let mut update = |id: Entity, mass: f32, pos: Vec2| {
            let Ok(mut cmds) = commands.get_entity(id) else {
                return;
            };
            cmds.insert(GravityEnergy {
                energy: mass * -gravity.0.y * pos.y,
            });
        };

        if gravity.is_changed() {
            for (id, Mass(mass), pos) in bodies.iter() {
                update(id, *mass, pos.0);
            }
        } else {
            for (id, Mass(mass), pos) in changed_bodies.iter() {
                update(id, *mass, pos.0);
            }
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

#[derive(Component)]
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
}
