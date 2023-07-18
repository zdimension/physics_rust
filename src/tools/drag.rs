use bevy::math::{Vec2, Vec3Swizzles};
use bevy::prelude::*;
use bevy_mouse_tracking_plugin::MainCamera;
use bevy_rapier2d::prelude::*;
use crate::{CustomForce, FOREGROUND_Z};

#[derive(Copy, Clone, Debug)]
pub struct DragState {
    pub entity: Entity,
    pub orig_obj_pos: Vec2,
    pub drag_entity: Entity
}

#[derive(Copy, Clone, Event)]
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
            strength: 1e3f32,
            max_force: f32::INFINITY
        }
    }
}

#[derive(Component)]
pub struct DragObject;

pub fn init_drag(mut commands: Commands) {

}

pub fn process_drag(
    mut events: EventReader<DragEvent>,
    mut drag_data: Query<&mut CustomForce, With<DragObject>>,
    drag_ent: Query<(&Transform, &Velocity), Without<MainCamera>>,
    mut commands: Commands,
    mut gizmos: Gizmos,
    config: Res<DragConfig>,
    cameras: Query<&Transform, With<MainCamera>>
) {
    let cam_scale = cameras.single().scale.x;
    for ev in events.iter() {
        let Ok(mut drag_data) = drag_data.get_mut(ev.state.drag_entity) else { return };
        let (xform, vel) = drag_ent.get(ev.state.entity).unwrap();
        let actual_pos = xform.transform_point(ev.state.orig_obj_pos.extend(1.0)).xy();
        let force = (ev.mouse_pos - actual_pos) * config.strength * cam_scale - vel.linvel * 20.0;
        info!("drag force: {:?}", force);
        drag_data.0 = ExternalForce::at_point(force, ev.mouse_pos, actual_pos);
        gizmos.line(ev.mouse_pos.extend(FOREGROUND_Z), actual_pos.extend(FOREGROUND_Z), Color::WHITE);
    }
}