use bevy::math::Vec2;
use bevy::prelude::{Entity, Event, EventReader, Query, Transform};
use bevy_xpbd_2d::components::Position;

#[derive(Copy, Clone, Event)]
pub struct MoveEvent {
    pub entity: Entity,
    pub pos: Vec2,
}

pub fn process_move(mut events: EventReader<MoveEvent>, mut query: Query<&mut Position>) {
    for MoveEvent { entity, pos } in events.iter().copied() {
        let mut transform = query.get_mut(entity).unwrap();
        transform.0 = pos;
    }
}

#[derive(Copy, Clone, Debug)]
pub struct MoveState {
    pub obj_delta: Vec2,
}
