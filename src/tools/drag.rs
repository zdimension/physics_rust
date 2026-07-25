use crate::mouse_tracking::MainCamera;
use crate::{FOREGROUND_Z, InvTransformPoint};
use avian2d::prelude::*;
use bevy::math::Vec2;
use bevy::prelude::*;

#[derive(Copy, Clone, Debug)]
pub struct DragState {
    pub entity: Entity,
    pub grab_local_point: Vec2,
    pub drag_entity: Entity,
}

#[derive(Copy, Clone, Message)]
pub struct DragEvent {
    pub state: DragState,
    pub mouse_pos: Vec2,
}

#[derive(Resource)]
pub struct DragConfig {
    /// Algodoo drag-tool strength.
    pub strength: f32,
    /// Maximum pulling force in N.
    pub max_force: f32,
    /// Apply the drag force at the body's center of mass instead of the grabbed point.
    pub drag_center_of_mass: bool,
}

impl Default for DragConfig {
    fn default() -> Self {
        Self {
            strength: 1e7,
            max_force: f32::INFINITY,
            drag_center_of_mass: true,
        }
    }
}

#[derive(Component)]
pub struct DragObject;

#[derive(Component)]
pub struct DragTarget {
    pub entity: Entity,
    pub grab_local_point: Vec2,
    pub mouse_pos: Vec2,
}

pub fn init_drag() {}

fn effective_stiffness(zoom: f32, strength: f32, mass: f32) -> f32 {
    (zoom / (0.77 * strength) + 1.0 / (845.0 * mass)).recip()
}

fn critical_damping(stiffness: f32, mass: f32) -> f32 {
    2.0 * (stiffness * mass).sqrt()
}

pub fn update_drag_target(
    mut events: MessageReader<DragEvent>,
    mut drag_targets: Query<&mut DragTarget, With<DragObject>>,
) {
    for event in events.read() {
        let Ok(mut target) = drag_targets.get_mut(event.state.drag_entity) else {
            continue;
        };
        target.mouse_pos = event.mouse_pos;
    }
}

pub fn apply_drag_force(
    drag_targets: Query<&DragTarget, With<DragObject>>,
    mut drag_ent: Query<
        (&Position, &Rotation, &ColliderMassProperties, Forces),
        Without<MainCamera>,
    >,
    mut gizmos: Gizmos,
    config: Res<DragConfig>,
    cameras: Query<&Transform, With<MainCamera>>,
) {
    let cam_scale = cameras.single().unwrap().scale.x;
    for target in drag_targets.iter() {
        let Ok((position, rotation, mass, mut forces)) = drag_ent.get_mut(target.entity) else {
            continue;
        };
        let center_of_mass = position.0 + *rotation * mass.center_of_mass;
        let attachment_point = if config.drag_center_of_mass {
            center_of_mass
        } else {
            position.0 + *rotation * target.grab_local_point
        };
        let stiffness = effective_stiffness(cam_scale.recip(), config.strength, mass.mass);
        let damping = critical_damping(stiffness, mass.mass);
        let force = ((target.mouse_pos - attachment_point) * stiffness
            - forces.velocity_at_point(attachment_point) * damping)
            .clamp_length_max(config.max_force);

        if config.drag_center_of_mass {
            forces.apply_force(force);
        } else {
            forces.apply_force_at_point(force, attachment_point);
        }
        gizmos.line(
            target.mouse_pos.extend(FOREGROUND_Z),
            attachment_point.extend(FOREGROUND_Z),
            Color::WHITE,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_stiffness_matches_drag_law() {
        let zoom = 100.0_f32;
        let strength = 1e7_f32;
        let mass = 2.0_f32;
        let expected = (zoom / (0.77_f32 * strength) + 1.0_f32 / (845.0_f32 * mass)).recip();

        assert_eq!(effective_stiffness(zoom, strength, mass), expected);
    }

    #[test]
    fn critical_damping_matches_mass_and_stiffness() {
        assert_eq!(critical_damping(845.0, 2.0), 2.0 * 1690.0_f32.sqrt());
    }

    #[test]
    fn force_cap_limits_vector_magnitude() {
        let force = (Vec2::new(3.0, 4.0) * 100.0).clamp_length_max(250.0);

        assert_eq!(force.length(), 250.0);
        assert_eq!(force.normalize(), Vec2::new(0.6, 0.8));
    }
}
