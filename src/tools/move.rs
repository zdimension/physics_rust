use bevy::prelude::{Entity, EventReader, Query, Transform};
use bevy::math::Vec2;

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
