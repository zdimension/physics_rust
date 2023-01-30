use bevy::math::{Quat, Vec2, Vec3Swizzles};
use bevy::prelude::{Entity, EventReader, Query, Transform};

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
        let Ok(mut transform) = query.get_mut(entity) else { continue };
        let start = click_pos - transform.translation.xy();
        let current = mouse_pos - transform.translation.xy();
        let mut angle = (2.0 * orig_obj_rot.z.asin()) + start.angle_between(current);
        if current.length() <= crate::ROTATE_HELPER_RADIUS {
            let count = angle / crate::ROTATE_HELPER_ROUND_TO;
            let rounded = count.round();
            angle = rounded * crate::ROTATE_HELPER_ROUND_TO;
        }
        transform.rotation = Quat::from_rotation_z(angle);
    }
}
