use bevy::math::Vec2;
use bevy::prelude::Entity;

#[derive(Copy, Clone, Debug)]
pub struct DragState {
    pub(crate) entity: Entity,
    pub(crate) orig_obj_pos: Vec2,
}
