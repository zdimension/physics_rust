use avian2d::prelude::{Position, Rotation};
use bevy::math::{Quat, Vec2};
use bevy::prelude::{
    ChildOf, Entity, GlobalTransform, Message, MessageReader, Query, Transform, With, Without,
};

use crate::InvTransformPoint;
use crate::tools::add_object::AttachmentKind;

#[derive(Clone, Message)]
pub struct RotateEvent {
    pub state: RotateState,
    pub click_pos: Vec2,
    pub mouse_pos: Vec2,
}

pub fn process_rotate(
    mut events: MessageReader<RotateEvent>,
    mut attachments: Query<
        (
            &mut Position,
            &mut Rotation,
            &mut Transform,
            Option<&ChildOf>,
        ),
        With<AttachmentKind>,
    >,
    parents: Query<&GlobalTransform, Without<AttachmentKind>>,
    mut bodies: Query<(&mut Position, &mut Rotation, &mut Transform), Without<AttachmentKind>>,
) {
    for RotateEvent {
        state,
        click_pos,
        mouse_pos,
    } in events.read().cloned()
    {
        let angle_delta = rotation_delta(&state, click_pos, mouse_pos);

        for target in state.targets {
            let rotated_pos = state.pivot
                + Vec2::from_angle(angle_delta).rotate(target.original_pos - state.pivot);
            let angle = target.original_angle + angle_delta;

            if let Ok((mut position, mut rotation, mut transform, parent)) =
                attachments.get_mut(target.entity)
            {
                let parent_transform = parent.and_then(|parent| parents.get(parent.parent()).ok());
                let (local_pos, local_rotation) =
                    attachment_local_pose(parent_transform, rotated_pos, angle);

                position.0 = rotated_pos;
                *rotation = Rotation::radians(angle);
                transform.translation.x = local_pos.x;
                transform.translation.y = local_pos.y;
                transform.rotation = local_rotation;
                continue;
            }

            let Ok((mut position, mut rotation, mut transform)) = bodies.get_mut(target.entity)
            else {
                continue;
            };
            position.0 = rotated_pos;
            transform.translation.x = rotated_pos.x;
            transform.translation.y = rotated_pos.y;
            *rotation = Rotation::radians(angle);
            transform.rotation = Quat::from_rotation_z(angle);
        }
    }
}

fn attachment_local_pose(
    parent: Option<&GlobalTransform>,
    world_pos: Vec2,
    world_angle: f32,
) -> (Vec2, Quat) {
    let local_pos = parent.map_or(world_pos, |parent| parent.to_local(world_pos));
    let parent_rotation = parent.map_or(Quat::IDENTITY, GlobalTransform::rotation);
    (
        local_pos,
        parent_rotation.inverse() * Quat::from_rotation_z(world_angle),
    )
}

pub(crate) fn rotation_delta(state: &RotateState, click_pos: Vec2, mouse_pos: Vec2) -> f32 {
    let start = click_pos - state.pivot;
    let current = mouse_pos - state.pivot;
    let mut angle_delta = start.perp_dot(current).atan2(start.dot(current));
    if current.length() <= ROTATE_HELPER_RADIUS * state.scale {
        angle_delta = (angle_delta / ROTATE_HELPER_ROUND_TO).round() * ROTATE_HELPER_ROUND_TO;
    }
    angle_delta
}

#[derive(Clone, Debug)]
pub struct RotateState {
    pub current_angle: f32,
    pub pivot: Vec2,
    pub targets: Vec<RotateTarget>,
    pub overlay_ent: Entity,
    pub scale: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct RotateTarget {
    pub entity: Entity,
    pub original_pos: Vec2,
    pub original_angle: f32,
}

pub const ROTATE_HELPER_RADIUS: f32 = 136.0;
const ROTATE_HELPER_ROUND_TO: f32 = 15.0f32 * std::f32::consts::PI / 180.0;

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::{EulerRot, Vec3};

    fn state(scale: f32) -> RotateState {
        RotateState {
            current_angle: 0.0,
            pivot: Vec2::ZERO,
            targets: Vec::new(),
            overlay_ent: Entity::PLACEHOLDER,
            scale,
        }
    }

    #[test]
    fn rotation_delta_tracks_the_mouse_angle() {
        let state = state(1.0);

        let delta = rotation_delta(&state, Vec2::new(200.0, 0.0), Vec2::new(0.0, 200.0));

        assert!((delta - std::f32::consts::FRAC_PI_2).abs() < 1.0e-6);
    }

    #[test]
    fn rotation_delta_uses_the_same_inner_ring_snapping_as_the_transform() {
        let state = state(1.0);
        let radius = 100.0;
        let mouse = Vec2::from_angle(20.0_f32.to_radians()) * radius;

        let delta = rotation_delta(&state, Vec2::X * radius, mouse);

        assert!((delta - 15.0_f32.to_radians()).abs() < 1.0e-6);
    }

    #[test]
    fn attachment_rotation_converts_world_pose_to_parent_space() {
        let parent = GlobalTransform::from(
            Transform::from_translation(Vec3::new(10.0, 0.0, 0.0))
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        );

        let (local_pos, local_rotation) = attachment_local_pose(
            Some(&parent),
            Vec2::new(10.0, 2.0),
            std::f32::consts::FRAC_PI_2,
        );

        assert!((local_pos - Vec2::new(2.0, 0.0)).length() < 1.0e-5);
        assert!(local_rotation.to_euler(EulerRot::XYZ).2.abs() < 1.0e-5);
    }
}
