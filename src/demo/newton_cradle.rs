use avian2d::prelude::*;
use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;

use crate::objects::phy_obj::PhysicalObject;

const BALL_COUNT: usize = 5;
const BALL_RADIUS: f32 = 0.4;
const PENDULUM_LENGTH: f32 = 2.0;
const PIVOT_Y: f32 = 4.0;

pub fn init(commands: &mut ChildSpawnerCommands) {
    let mut z = 1.0;
    let mut next_z = || {
        z += 0.1;
        z
    };

    commands
        .spawn(PhysicalObject::rect(
            Vec2::new(6.0, 0.2),
            Vec3::new(-3.0, PIVOT_Y, next_z()),
        ))
        .insert(RigidBody::Static);

    for index in 0..BALL_COUNT {
        let rest_x = (index as f32 - (BALL_COUNT as f32 - 1.0) * 0.5) * BALL_RADIUS * 2.0;
        let pivot = Vec2::new(rest_x, PIVOT_Y);
        let ball_position = if index == 0 {
            let release_angle = 0.55_f32;
            pivot
                + Vec2::new(
                    -release_angle.sin() * PENDULUM_LENGTH,
                    -release_angle.cos() * PENDULUM_LENGTH,
                )
        } else {
            pivot - Vec2::Y * PENDULUM_LENGTH
        };

        let anchor = commands
            .spawn((RigidBody::Static, Position(pivot), Rotation::default()))
            .id();
        let ball = commands
            .spawn(PhysicalObject::ball(
                BALL_RADIUS,
                ball_position.extend(next_z()),
            ))
            .insert(Restitution::new(1.0))
            .id();

        commands.spawn((
            JointCollisionDisabled,
            RevoluteJoint::new(anchor, ball)
                .with_local_anchor2(Vec2::Y * PENDULUM_LENGTH),
        ));
    }
}
