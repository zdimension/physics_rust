use bevy::prelude::*;
use bevy_rapier2d::prelude::*;

pub fn compute_measures() -> SystemSet {
    SystemSet::new()
        .with_system(KineticEnergy::compute)
        .with_system(GravityEnergy::compute)
        .with_system(Momentum::compute)
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
