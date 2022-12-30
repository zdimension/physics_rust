use bevy::prelude::{Entity, EventReader, Query, Transform};
use bevy::math::{Quat, Vec2, Vec3Swizzles};

#[derive(Copy, Clone)]
pub struct RotateEvent {
    pub entity: Entity,
    pub orig_obj_rot: Quat,
    pub click_pos: Vec2,
    pub mouse_pos: Vec2,
}

pub fn process_rotate(mut events: EventReader<RotateEvent>, mut query: Query<&mut Transform>) {
    for RotateEvent {
        entity,
        orig_obj_rot,
        click_pos,
        mouse_pos,
    } in events.iter().copied()
    {
        let mut transform = query.get_mut(entity).unwrap();
        let start = click_pos - transform.translation.xy();
        let current = mouse_pos - transform.translation.xy();
        let angle = start.angle_between(current);
        transform.rotation = orig_obj_rot * Quat::from_rotation_z(angle);
    }
}
