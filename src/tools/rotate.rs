use crate::ToRot;
use bevy::math::{Quat, Vec2, Vec3Swizzles};
use bevy::prelude::{Entity, Event, EventReader, Query, Transform};
use bevy_xpbd_2d::components::{Position, Rotation};

#[derive(Copy, Clone, Event)]
pub struct RotateEvent {
    pub entity: Entity,
    pub orig_obj_rot: f32,
    pub click_pos: Vec2,
    pub mouse_pos: Vec2,
    pub scale: f32,
}

pub fn process_rotate(mut events: EventReader<RotateEvent>, mut query: Query<(&Position, &mut Rotation)>) {
    for RotateEvent {
        entity,
        orig_obj_rot,
        click_pos,
        mouse_pos,
        scale,
    } in events.iter().copied()
    {
        let Ok((position, mut transform)) = query.get_mut(entity) else { continue };
        let start = click_pos - position.0;
        let current = mouse_pos - position.0;
        let mut angle = orig_obj_rot + start.angle_between(current);
        if current.length() <= ROTATE_HELPER_RADIUS * scale {
            let count = angle / ROTATE_HELPER_ROUND_TO;
            let rounded = count.round();
            angle = rounded * ROTATE_HELPER_ROUND_TO;
        }
        *transform = Rotation::from_radians(angle);
    }
}

#[derive(Copy, Clone, Debug)]
pub struct RotateState {
    pub orig_obj_rot: f32,
    pub overlay_ent: Entity,
    pub scale: f32,
}

pub const ROTATE_HELPER_RADIUS: f32 = 136.0;
const ROTATE_HELPER_ROUND_TO: f32 = 15.0f32 * std::f32::consts::PI / 180.0;
