use avian2d::prelude::Position;
use bevy::math::Vec2;
use bevy::prelude::{Entity, Message, MessageReader, Query, Transform};

#[derive(Copy, Clone, Message)]
pub struct MoveEvent {
    pub entity: Entity,
    pub pos: Vec2,
}

pub fn process_move(mut events: MessageReader<MoveEvent>, mut query: Query<&mut Position>) {
    for MoveEvent { entity, pos } in events.read().copied() {
        let mut transform = query.get_mut(entity).unwrap();
        transform.0 = pos;
    }
}

#[derive(Copy, Clone, Debug)]
pub struct MoveState {
    pub obj_delta: Vec2,
}
