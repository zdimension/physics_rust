use bevy::math::Vec2;
use bevy::prelude::{Entity, EventReader, Query, Transform};

#[derive(Copy, Clone)]
pub struct MoveEvent {
    pub entity: Entity,
    pub pos: Vec2,
}

pub fn process_move(mut events: EventReader<MoveEvent>, mut query: Query<&mut Transform>) {
    for MoveEvent { entity, pos } in events.iter().copied() {
        let mut transform = query.get_mut(entity).unwrap();
        transform.translation = pos.extend(transform.translation.z);
    }
}

#[derive(Copy, Clone, Debug)]
pub struct MoveState {
    pub obj_delta: Vec2,
}
