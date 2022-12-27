use bevy::prelude::*;
use bevy_rapier2d::prelude::*;

pub fn compute_measures() -> SystemSet {
    SystemSet::new()
        .with_system(KineticEnergy::compute)
}

#[derive(Component)]
pub struct KineticEnergy {
    pub linear: f32,
    pub angular: f32,
}

impl KineticEnergy {
    fn compute(
        bodies: Query<(Entity, &ReadMassProperties, &Velocity)>,
        mut commands: Commands
    ) {
        for (id, ReadMassProperties(mass), vel) in bodies.iter() {
            let linear = mass.mass * vel.linvel.length_squared() / 2.0;
            let angular = mass.principal_inertia * vel.angvel * vel.angvel / 2.0;
            commands.entity(id).insert(KineticEnergy { linear, angular });
        }
    }
}