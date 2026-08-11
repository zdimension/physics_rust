use avian2d::prelude::*;
use bevy::math::{EulerRot, Quat, Vec2};
use bevy::prelude::*;

use crate::objects::{body::PhysicsBody, phy_obj::PhysicalGeometry};
use crate::tools::add_object::AttachmentKind;

#[derive(Component, Copy, Clone, Debug)]
pub struct ThrusterSettings {
    pub force: f32,
    pub follow_geometry_rotation: bool,
    pub(crate) fixed_angle: f32,
}

impl Default for ThrusterSettings {
    fn default() -> Self {
        Self {
            force: 5.0,
            follow_geometry_rotation: true,
            fixed_angle: 0.0,
        }
    }
}

#[derive(Component)]
pub(crate) struct ThrusterInner;

pub fn add_systems(app: &mut App) {
    app.add_systems(Update, sync_independent_rotation)
        .add_systems(
            PhysicsSchedule,
            apply_thruster_forces
                .in_set(PhysicsStepSystems::BroadPhase)
                .after(crate::objects::spring::apply_spring_forces),
        );
}

fn sync_independent_rotation(
    mut thrusters: Query<(&ThrusterSettings, &ChildOf, &mut Transform), With<AttachmentKind>>,
    bodies: Query<&Rotation, With<PhysicalGeometry>>,
) {
    for (settings, parent, mut transform) in &mut thrusters {
        if settings.follow_geometry_rotation {
            continue;
        }
        let Ok(parent_rotation) = bodies.get(parent.parent()) else {
            continue;
        };
        transform.rotation =
            Quat::from_rotation_z(settings.fixed_angle - parent_rotation.as_radians());
    }
}

pub(crate) fn apply_thruster_forces(
    thrusters: Query<(&ThrusterSettings, &Transform, &ChildOf)>,
    geometries: Query<
        (&Position, &Rotation, &ColliderOf),
        (With<PhysicalGeometry>, Without<PhysicsBody>),
    >,
    mut bodies: Query<
        Forces,
        (
            With<PhysicsBody>,
            Without<PhysicalGeometry>,
            Without<RigidBodyDisabled>,
        ),
    >,
) {
    for (settings, transform, parent) in &thrusters {
        let Ok((position, geometry_rotation, link)) = geometries.get(parent.parent()) else {
            continue;
        };
        let Ok(mut forces) = bodies.get_mut(link.body) else {
            continue;
        };

        let local_point = transform.translation.truncate();
        let point = application_point(position.0, *geometry_rotation, local_point);
        let force = force_vector(
            settings,
            geometry_rotation.as_radians(),
            transform.rotation.to_euler(EulerRot::XYZ).2,
        );
        forces.apply_force_at_point(force, point);
    }
}

fn application_point(body_position: Vec2, body_rotation: Rotation, local_point: Vec2) -> Vec2 {
    body_position + body_rotation * local_point
}

fn force_vector(settings: &ThrusterSettings, body_angle: f32, local_angle: f32) -> Vec2 {
    let angle = if settings.follow_geometry_rotation {
        body_angle + local_angle
    } else {
        settings.fixed_angle
    };
    Vec2::from_angle(angle) * settings.force.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_five_newtons_and_follows_geometry() {
        let settings = ThrusterSettings::default();

        assert_eq!(settings.force, 5.0);
        assert!(settings.follow_geometry_rotation);
    }

    #[test]
    fn following_force_rotates_with_body_and_uses_attachment_point() {
        let settings = ThrusterSettings::default();

        let force = force_vector(&settings, std::f32::consts::FRAC_PI_2, 0.0);
        let point = application_point(
            Vec2::new(1.0, 2.0),
            Rotation::radians(std::f32::consts::FRAC_PI_2),
            Vec2::new(2.0, 0.0),
        );

        assert!((force - Vec2::new(0.0, 5.0)).length() < 1.0e-5);
        assert!((point - Vec2::new(1.0, 4.0)).length() < 1.0e-5);
    }

    #[test]
    fn independent_force_uses_fixed_world_angle() {
        let settings = ThrusterSettings {
            follow_geometry_rotation: false,
            fixed_angle: 0.0,
            ..Default::default()
        };

        let force = force_vector(&settings, std::f32::consts::FRAC_PI_2, 1.0);

        assert!((force - Vec2::new(5.0, 0.0)).length() < 1.0e-5);
    }
}
