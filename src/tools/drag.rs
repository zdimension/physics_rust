use bevy::math::Vec2;
use bevy::prelude::*;
use crate::mouse_tracking::MainCamera;
use avian2d::prelude::*;
use crate::{FOREGROUND_Z, InvTransformPoint};

#[derive(Copy, Clone, Debug)]
pub struct DragState {
    pub entity: Entity,
    pub orig_obj_pos: Vec2,
    pub drag_entity: Entity
}

#[derive(Copy, Clone, Message)]
pub struct DragEvent {
    pub state: DragState,
    pub mouse_pos: Vec2,
}

#[derive(Resource)]
pub struct DragConfig {
    /// technically in N*px
    pub strength: f32,
    /// Force removed per metre per second at the grabbed point.
    pub damping: f32,
    /// Maximum pulling force in N.
    pub max_force: f32
}

impl Default for DragConfig {
    fn default() -> Self {
        Self {
            strength: 1e4f32,
            damping: 40.0,
            max_force: 2_000.0
        }
    }
}

#[derive(Component)]
pub struct DragObject;

pub fn init_drag() {}

pub fn process_drag(
    mut events: MessageReader<DragEvent>,
    drag_data: Query<(), With<DragObject>>,
    mut drag_ent: Query<(&GlobalTransform, Forces), Without<MainCamera>>,
    mut gizmos: Gizmos,
    config: Res<DragConfig>,
    cameras: Query<&Transform, With<MainCamera>>
) {
    let cam_scale = cameras.single().unwrap().scale.x;
    for ev in events.read() {
        let Ok(()) = drag_data.get(ev.state.drag_entity) else { continue };
        let Ok((xform, mut forces)) = drag_ent.get_mut(ev.state.entity) else { continue };
        let actual_pos = xform.to_global(ev.state.orig_obj_pos);
        let stiffness = config.strength * cam_scale;
        let damping = config.damping;
        let force = ((ev.mouse_pos - actual_pos) * stiffness
            - forces.velocity_at_point(actual_pos) * damping)
            .clamp_length_max(config.max_force);

        // Pull at the point that was clicked, so both linear and angular motion are damped there.
        forces.apply_force_at_point(force, actual_pos);
        gizmos.line(ev.mouse_pos.extend(FOREGROUND_Z), actual_pos.extend(FOREGROUND_Z), Color::WHITE);
    }
}