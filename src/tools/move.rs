use avian2d::dynamics::rigid_body::{AngularVelocity, LinearVelocity};
use avian2d::prelude::Position;
use bevy::math::Vec2;
use bevy::prelude::{
    ChildOf, Entity, GlobalTransform, Message, MessageReader, Query, Transform, With, Without,
};

use crate::InvTransformPoint;
use crate::tools::add_object::AttachmentKind;

#[derive(Copy, Clone, Message)]
pub struct MoveEvent {
    pub entity: Entity,
    pub pos: Vec2,
}

pub fn process_move(
    mut events: MessageReader<MoveEvent>,
    mut attachments: Query<(&mut Transform, Option<&ChildOf>), With<AttachmentKind>>,
    parents: Query<&GlobalTransform>,
    mut query: Query<
        (
            &mut Position,
            &mut Transform,
            &mut LinearVelocity,
            &mut AngularVelocity,
        ),
        Without<AttachmentKind>,
    >,
) {
    for MoveEvent { entity, pos } in events.read().copied() {
        if let Ok((mut transform, parent)) = attachments.get_mut(entity) {
            let local_pos = parent
                .and_then(|parent| parents.get(parent.parent()).ok())
                .map_or(pos, |parent_transform| parent_transform.to_local(pos));
            transform.translation.x = local_pos.x;
            transform.translation.y = local_pos.y;
            continue;
        }

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

#[derive(Clone, Debug)]
pub struct MoveState {
    pub primary_delta: Vec2,
    pub pointer_start: Vec2,
    pub targets: Vec<(Entity, Vec2)>,
}

impl MoveState {
    pub fn pointer_delta(&self, pointer: Vec2) -> Vec2 {
        pointer - self.pointer_start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_is_relative_to_the_pointer_when_the_move_gesture_starts() {
        let pointer_start = Vec2::new(4.25, -1.5);
        let state = MoveState {
            primary_delta: Vec2::new(2.0, 3.0),
            pointer_start,
            targets: Vec::new(),
        };

        assert_eq!(state.pointer_delta(pointer_start), Vec2::ZERO);
        assert_eq!(
            state.pointer_delta(pointer_start + Vec2::new(0.5, -0.25)),
            Vec2::new(0.5, -0.25)
        );
    }
}
