use bevy::math::Vec2;
use bevy::prelude::Entity;
use bevy_egui::egui::ecolor::Hsva;

use crate::tools::rotate::ROTATE_HELPER_RADIUS;

const PLANE_ANGLE_STEP: f32 = 15.0_f32.to_radians();

#[derive(Clone, Debug)]
pub struct PlanePlacementState {
    pub overlay_ent: Entity,
    pub scale: f32,
    pub color: Hsva,
}

pub(crate) fn plane_outward_normal(center: Vec2, mouse: Vec2, scale: f32) -> Vec2 {
    let delta = mouse - center;
    if delta.length_squared() <= f32::EPSILON {
        return Vec2::Y;
    }

    let mut angle = delta.y.atan2(delta.x);
    if delta.length() <= ROTATE_HELPER_RADIUS * scale {
        angle = (angle / PLANE_ANGLE_STEP).round() * PLANE_ANGLE_STEP;
    }
    Vec2::from_angle(angle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inner_plane_handle_snaps_to_fifteen_degrees() {
        let mouse = Vec2::from_angle(20.0_f32.to_radians()) * 100.0;
        let normal = plane_outward_normal(Vec2::ZERO, mouse, 1.0);
        assert!((normal.to_angle() - 15.0_f32.to_radians()).abs() < 1.0e-6);
    }

    #[test]
    fn outer_plane_handle_is_free() {
        let mouse = Vec2::from_angle(20.0_f32.to_radians()) * 200.0;
        let normal = plane_outward_normal(Vec2::ZERO, mouse, 1.0);
        assert!((normal.to_angle() - 20.0_f32.to_radians()).abs() < 1.0e-6);
    }
}
