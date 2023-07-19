use crate::systems;
use bevy::prelude::*;
use bevy_xpbd_2d::{math::*, prelude::*};

systems! {
    KineticEnergy::compute,
    GravityEnergy::compute,
    Momentum::compute,
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
    pub(crate) fn compute(
        bodies: Query<(Entity, &ColliderMassProperties, &LinearVelocity, &AngularVelocity)>,
        mut commands: Commands,
    ) {
        for (id, mass, lin, ang) in bodies.iter() {
            let Some(mut cmds) = commands.get_entity(id) else { continue; };
            let linear = mass.mass.0 * lin.0.length_squared() / 2.0;
            let angular = mass.inertia.0 * ang.0 * ang.0 / 2.0;
            cmds.insert(KineticEnergy { linear, angular });
        }
    }

    pub fn total(&self) -> f32 {
        self.linear + self.angular
    }
}

#[derive(Component)]
pub struct GravityEnergy {
    pub energy: f32,
}

impl GravityEnergy {
    pub(crate) fn compute(
        bodies: Query<(Entity, &Mass, &Position)>,
        gravity: Res<Gravity>,
        mut commands: Commands,
    ) {
        for (id, Mass(mass), pos) in bodies.iter() {
            let Some(mut cmds) = commands.get_entity(id) else { continue; };
            let energy = mass * -gravity.0.y * pos.0.y;
            cmds.insert(GravityEnergy { energy });
        }
    }
}

#[derive(Component)]
pub struct Momentum {
    pub linear: Vec2,
    pub angular: f32,
}

impl Momentum {
    pub(crate) fn compute(
        bodies: Query<(Entity, &ColliderMassProperties, &LinearVelocity, &AngularVelocity)>,
        mut commands: Commands,
    ) {
        for (id, props, lin, ang) in bodies.iter() {
            let Some(mut cmds) = commands.get_entity(id) else { continue; };
            let linear = props.mass.0 * lin.0;
            let angular = props.inertia.0 * ang.0;
            cmds.insert(Momentum { linear, angular });
        }
    }
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
        mut commands: Commands,
        gravity: Res<Gravity>,
    ) {
        use ForceKind::*;

        for (id, Mass(mass)) in bodies.iter() {
            let Some(mut cmds) = commands.get_entity(id) else { continue; };
            let mut forces = vec![];

            forces.push(AppliedForce {
                kind: Gravity,
                at: Vec2::ZERO,
                value: Vec2::new(0.0, mass * gravity.0.y).into(),
            });

            cmds.insert(Forces { forces });
        }
    }
}
