use bevy::math::{Vec2, Vec3Swizzles};
use bevy::prelude::*;
use bevy_mouse_tracking_plugin::MainCamera;
use bevy_rapier2d::prelude::*;

#[derive(Copy, Clone, Debug)]
pub struct DragState {
    pub(crate) entity: Entity,
    pub(crate) orig_obj_pos: Vec2,
}

#[derive(Copy, Clone, Event)]
pub struct DragEvent {
    pub entity: Entity,
    pub orig_obj_pos: Vec2,
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
            strength: 1.0f32,
            max_force: f32::INFINITY
        }
    }
}

#[derive(Component)]
pub struct DragObject;

pub fn init_drag(mut commands: Commands) {
    commands.spawn((
                       DragObject,
                       RigidBody::Dynamic));
}

pub fn process_drag(
    mut events: EventReader<DragEvent>,
    drag: Query<(Entity), With<DragObject>>,
    xform: Query<&Transform, Without<MainCamera>>,
    mut commands: Commands,
    mut gizmos: Gizmos,
    config: Res<DragConfig>,
    cameras: Query<&Transform, With<MainCamera>>
) {
    let (drag_entity) = drag.single();
    let cam_scale = cameras.single().scale.x;
    for ev in events.iter() {
        /*let rope_joint = RopeJointBuilder::new()
            .local_anchor1(ev.orig_obj_pos)
            .local_anchor2(ev.mouse_pos)
            .motor_position(0.0, 10.0, 0.0);
        commands.entity(drag_entity).insert((
            ImpulseJoint::new(ev.entity, rope_joint)
        ));*/
        let actual_pos = xform.get(ev.entity).unwrap().transform_point(ev.orig_obj_pos.extend(1.0)).xy();
        let force = (ev.mouse_pos - actual_pos) * config.strength * cam_scale;
        info!("drag force: {:?}", force);
        commands.entity(ev.entity).insert(ExternalForce {
            torque: 0.0,
            force: force
        });
        gizmos.line_2d(ev.mouse_pos, actual_pos, Color::WHITE);
    }
}