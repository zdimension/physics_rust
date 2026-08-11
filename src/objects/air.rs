use avian2d::prelude::*;
use bevy::prelude::*;

use super::{
    body::{velocity_at_point, world_point},
    phy_obj::PhysicalGeometry,
};

#[derive(Resource, Copy, Clone, Debug)]
pub struct AirSettings {
    pub enabled: bool,
    pub multiplier: f32,
    pub linear_term: f32,
    pub quadratic_term: f32,
    pub wind_speed: f32,
    pub wind_direction: f32,
}

impl Default for AirSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            multiplier: 1.0,
            linear_term: 0.01,
            quadratic_term: 0.001,
            wind_speed: 0.0,
            wind_direction: 0.0,
        }
    }
}

pub(crate) fn add_systems(app: &mut App) {
    app.add_systems(
        PhysicsSchedule,
        apply_air_friction
            .in_set(PhysicsStepSystems::BroadPhase)
            .after(crate::objects::attraction::AttractionForcesSet),
    );
}

fn apply_air_friction(
    settings: Res<AirSettings>,
    geometries: Query<
        (Entity, &Collider, &Position, &Rotation, &ColliderOf),
        (With<PhysicalGeometry>, Without<RigidBodyDisabled>),
    >,
    mut bodies: ParamSet<(
        Query<
            (
                &RigidBody,
                &Position,
                &Rotation,
                &LinearVelocity,
                &AngularVelocity,
                Option<&ComputedCenterOfMass>,
            ),
            Without<PhysicalGeometry>,
        >,
        Query<Forces, (Without<RigidBodyDisabled>, Without<PhysicalGeometry>)>,
    )>,
    mut pending_forces: Local<Vec<(Entity, Vec2, Vec2)>>,
) {
    if !settings.enabled || settings.multiplier == 0.0 {
        return;
    }

    let wind_velocity = Vec2::from_angle(settings.wind_direction) * settings.wind_speed;
    pending_forces.clear();
    {
        let body_query = bodies.p0();
        for (_, collider, position, rotation, link) in &geometries {
            let Ok((rigid_body, body_pos, body_rotation, velocity, angular, center)) =
                body_query.get(link.body)
            else {
                continue;
            };
            if !rigid_body.is_dynamic() {
                continue;
            }

            let center = world_point(
                (body_pos.0, *body_rotation),
                center.map_or(Vec2::ZERO, |center| center.0),
            );
            let point_velocity = velocity_at_point(center, velocity.0, angular.0, position.0);
            let relative_velocity = point_velocity - wind_velocity;
            let speed = relative_velocity.length();
            if speed <= f32::EPSILON || !speed.is_finite() {
                continue;
            }

            let diameter = projected_width(collider, *rotation, relative_velocity);
            let magnitude = diameter
                * settings.multiplier
                * (settings.linear_term * speed + settings.quadratic_term * speed * speed);
            let force = -relative_velocity / speed * magnitude;
            if force.is_finite() {
                pending_forces.push((link.body, force, position.0));
            }
        }
    }

    let mut forces = bodies.p1();
    for (entity, force, point) in pending_forces.iter().copied() {
        if let Ok(mut forces) = forces.get_mut(entity) {
            forces.apply_force_at_point(force, point);
        }
    }
}

fn projected_width(collider: &Collider, rotation: Rotation, movement: Vec2) -> f32 {
    let direction = movement.normalize_or_zero();
    if direction == Vec2::ZERO {
        return 0.0;
    }

    // Rotate an AABB query so its X axis is perpendicular to movement. Its width
    // is then the collider's exact projection across the direction of movement.
    let crosswind = Vec2::new(-direction.y, direction.x);
    let local_crosswind_angle = crosswind.to_angle() - rotation.as_radians();
    let projected = collider.aabb(Vec2::ZERO, Rotation::radians(-local_crosswind_angle));
    (projected.max.x - projected.min.x).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projected_width_uses_direction_and_body_rotation() {
        let collider = Collider::rectangle(4.0, 2.0);

        assert!((projected_width(&collider, Rotation::default(), Vec2::X) - 2.0).abs() < 1.0e-5);
        assert!((projected_width(&collider, Rotation::default(), Vec2::Y) - 4.0).abs() < 1.0e-5);
        assert!(
            (projected_width(
                &collider,
                Rotation::radians(std::f32::consts::FRAC_PI_2),
                Vec2::X,
            ) - 4.0)
                .abs()
                < 1.0e-5
        );
    }

    #[test]
    fn default_settings_match_air_panel_defaults() {
        let settings = AirSettings::default();

        assert!(settings.enabled);
        assert_eq!(settings.multiplier, 1.0);
        assert_eq!(settings.linear_term, 0.01);
        assert_eq!(settings.quadratic_term, 0.001);
        assert_eq!(settings.wind_speed, 0.0);
    }

    #[test]
    fn air_system_initializes_without_conflicting_velocity_access() {
        let mut app = App::new();
        app.init_resource::<AirSettings>()
            .add_systems(Update, apply_air_friction);

        app.update();
    }
}
