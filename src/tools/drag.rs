use bevy::math::{Vec2, Vec3Swizzles};
use bevy::prelude::*;
use crate::mouse_tracking::MainCamera;
use avian2d::{math::*, prelude::*};
use crate::{CustomForce, FOREGROUND_Z, InvTransformPoint};

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
    /// in N
    pub max_force: f32
}

impl Default for DragConfig {
    fn default() -> Self {
        Self {
            strength: 1e4f32,
            max_force: f32::INFINITY
        }
    }
}

#[derive(Component)]
pub struct DragObject;

pub fn init_drag(mut commands: Commands) {

}

pub fn process_drag(
    mut events: MessageReader<DragEvent>,
    mut drag_data: Query<&mut CustomForce, With<DragObject>>,
    mut drag_ent: Query<(&GlobalTransform, &Position, &LinearVelocity, Forces), Without<MainCamera>>,
    mut commands: Commands,
    mut gizmos: Gizmos,
    config: Res<DragConfig>,
    cameras: Query<&Transform, With<MainCamera>>
) {
    let cam_scale = cameras.single().unwrap().scale.x;
    for ev in events.read() {
        let Ok(mut drag_data) = drag_data.get_mut(ev.state.drag_entity) else { return };
        let (xform, pos, vel, mut forces) = drag_ent.get_mut(ev.state.entity).unwrap();
        let actual_pos = xform.to_global(ev.state.orig_obj_pos);
        let force = (ev.mouse_pos - actual_pos) * config.strength * cam_scale - vel.0 * 20.0;
        info!("drag force: {:?}", force);
        forces.apply_force_at_point(force, ev.mouse_pos);
        gizmos.line(ev.mouse_pos.extend(FOREGROUND_Z), actual_pos.extend(FOREGROUND_Z), Color::WHITE);
    }
}