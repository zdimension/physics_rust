use avian2d::prelude::{Position, Rotation};
use bevy::math::{Quat, Vec2};
use bevy::prelude::{Entity, Message, MessageReader, Query, Transform};

#[derive(Clone, Message)]
pub struct RotateEvent {
    pub state: RotateState,
    pub click_pos: Vec2,
    pub mouse_pos: Vec2,
}

pub fn process_rotate(
    mut events: MessageReader<RotateEvent>,
    mut query: Query<(&mut Position, &mut Rotation, &mut Transform)>,
) {
    for RotateEvent {
        state,
        click_pos,
        mouse_pos,
    } in events.read().cloned()
    {
        let angle_delta = rotation_delta(&state, click_pos, mouse_pos);

        for target in state.targets {
            let Ok((mut position, mut rotation, mut transform)) = query.get_mut(target.entity)
            else {
                continue;
            };
            let rotated_pos = state.pivot
                + Vec2::from_angle(angle_delta).rotate(target.original_pos - state.pivot);
            let angle = target.original_angle + angle_delta;
            position.0 = rotated_pos;
            transform.translation.x = rotated_pos.x;
            transform.translation.y = rotated_pos.y;
            *rotation = Rotation::radians(angle);
            transform.rotation = Quat::from_rotation_z(angle);
        }
    }
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
}
