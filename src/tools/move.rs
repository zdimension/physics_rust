use avian2d::dynamics::rigid_body::{AngularVelocity, LinearVelocity};
use avian2d::prelude::Position;
use bevy::math::Vec2;
use bevy::prelude::{Entity, Message, MessageReader, Query, Transform};

#[derive(Copy, Clone, Message)]
pub struct MoveEvent {
    pub entity: Entity,
    pub pos: Vec2,
}

pub fn process_move(
    mut events: MessageReader<MoveEvent>,
    mut query: Query<(&mut Position, &mut Transform, &mut LinearVelocity, &mut AngularVelocity)>,
) {
    for MoveEvent { entity, pos } in events.read().copied() {
        let Ok((mut position, mut transform, mut vel, mut ang_vel)) = query.get_mut(entity) else {
            continue;
        };
        position.0 = pos;
        transform.translation.x = pos.x;
        transform.translation.y = pos.y;
        vel.0 = Vec2::ZERO;
        ang_vel.0 = 0.0;
    }
}

#[derive(Copy, Clone, Debug)]
pub struct MoveState {
    pub obj_delta: Vec2,
}
