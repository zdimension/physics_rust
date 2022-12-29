use bevy::prelude::*;
use bevy_rapier2d::prelude::*;

pub fn compute_measures() -> SystemSet {
    SystemSet::new()
        .with_system(KineticEnergy::compute)
        .with_system(GravityEnergy::compute)
        .with_system(Momentum::compute)
        .with_system(Forces::compute)
}

#[derive(Component)]
pub struct KineticEnergy {
    pub linear: f32,
    pub angular: f32,
}
// todo sometimes it crashes it we delete an entity during a frame because
// it tries to insert a component on a despawned entity
impl KineticEnergy {
    fn compute(bodies: Query<(Entity, &ReadMassProperties, &Velocity)>, mut commands: Commands) {
        for (id, ReadMassProperties(mass), vel) in bodies.iter() {
            let linear = mass.mass * vel.linvel.length_squared() / 2.0;
            let angular = mass.principal_inertia * vel.angvel * vel.angvel / 2.0;
            commands
                .entity(id)
                .insert(KineticEnergy { linear, angular });
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
    fn compute(
        bodies: Query<(Entity, &ReadMassProperties, &Transform)>,
        rapier_conf: Res<RapierConfiguration>,
        mut commands: Commands,
    ) {
        for (id, ReadMassProperties(mass), pos) in bodies.iter() {
            let energy = mass.mass * -rapier_conf.gravity.y * pos.translation.y;
            commands.entity(id).insert(GravityEnergy { energy });
        }
    }
}

#[derive(Component)]
pub struct Momentum {
    pub linear: Vec2,
    pub angular: f32,
}

impl Momentum {
    fn compute(bodies: Query<(Entity, &ReadMassProperties, &Velocity)>, mut commands: Commands) {
        for (id, ReadMassProperties(mass), vel) in bodies.iter() {
            let linear = mass.mass * vel.linvel;
            let angular = mass.principal_inertia * vel.angvel;
            commands.entity(id).insert(Momentum { linear, angular });
        }
    }
}

pub enum ForceKind {
    Gravity,
    Torque
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

pub struct Forces {
    forces: Vec<AppliedForce>
}

impl Forces {
    fn new() -> Self {
        Self {
            forces: Vec::new()
        }
    }

    fn compute(bodies: Query<(Entity, &ReadMassProperties, &Velocity)>, mut commands: Commands, rapier_conf: Res<RapierConfiguration>) {
        use ForceKind::*;

        for (id, ReadMassProperties(mass), vel) in bodies.iter() {
            let mut forces = vec![];

            forces.push(AppliedForce {
                kind: Gravity,
                at: Vec2::ZERO,
                value: Vec2::new(0.0, mass.mass * rapier_conf.gravity.y).into()
            });

            commands.entity(id).insert(Forces { forces });
        }
    }
}