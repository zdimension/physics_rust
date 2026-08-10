use avian2d::prelude::*;
use bevy::prelude::*;

use crate::lyon_compat::{GeometryBuilder, Shape, shapes};
use crate::objects::MotorComponent;
use crate::tools::add_object::AttachmentJoint;
use crate::update_from::UpdateFrom;

#[derive(Component)]
pub struct AxleObject;

#[derive(Component)]
pub struct FixObject;

#[derive(Component, Copy, Clone, Debug, PartialEq)]
pub(crate) struct JointGeometry {
    pub(crate) geoms: [Option<Entity>; 2],
    pub(crate) positions: [Vec2; 2],
}

#[derive(Component)]
pub(crate) struct AxleVisual;

#[derive(Component)]
pub(crate) struct HingeMotorRing;

#[derive(Component)]
pub(crate) struct HingeMotorDirection {
    pub(crate) reversed: bool,
}

pub(crate) const HINGE_MOTOR_VISUAL_DIAMETER: f32 = 2.0;

const HINGE_VISUAL_DIAMETER: f32 = 1.0;
const HINGE_SELECTION_RADIUS: f32 = 0.55;
const MOTOR_DAMPING: f32 = 1.0;

pub(crate) fn hinge_selection_radius(_motor_enabled: bool) -> f32 {
    HINGE_SELECTION_RADIUS
}

fn hinge_visual_radius(motor_enabled: bool) -> f32 {
    if motor_enabled {
        HINGE_MOTOR_VISUAL_DIAMETER * 0.4
    } else {
        HINGE_VISUAL_DIAMETER * 0.4
    }
}

pub(crate) fn sync_hinge_motors(
    mut joints: Query<
        (
            &mut RevoluteJoint,
            &AttachmentJoint,
            &UpdateFrom<MotorComponent>,
        ),
        With<AxleObject>,
    >,
    parents: Query<(Option<&ChildOf>, Option<Ref<MotorComponent>>)>,
    geometries: Query<&JointGeometry>,
    changed_motors: Query<(), Changed<MotorComponent>>,
    changed_sources: Query<(), Changed<UpdateFrom<MotorComponent>>>,
) {
    if changed_motors.is_empty() && changed_sources.is_empty() {
        return;
    }

    for (mut joint, attachment, update_source) in &mut joints {
        let Some((_, motor)) = update_source.find_component(Entity::PLACEHOLDER, &parents) else {
            continue;
        };
        let Ok(geometry) = geometries.get(attachment.visual) else {
            continue;
        };
        let target = angular_motor_from(motor, geometry);
        if joint.motor != target {
            joint.motor = target;
        }
    }
}

pub(crate) fn break_hinges(
    mut commands: Commands,
    joints: Query<
        (Entity, &JointForces, &AttachmentJoint),
        (With<AxleObject>, Without<JointDisabled>),
    >,
    motors: Query<&MotorComponent>,
    time: Res<Time<Physics>>,
) {
    let delta_secs = time.delta_secs();
    if delta_secs <= 0.0 {
        return;
    }

    for (joint, forces, attachment) in &joints {
        let Ok(motor) = motors.get(attachment.visual) else {
            continue;
        };
        if !motor.break_limit.is_finite() || motor.break_limit <= 0.0 {
            continue;
        }

        let impulse = forces.force().length() * delta_secs;
        if impulse > motor.break_limit {
            commands.entity(joint).insert(JointDisabled);
        }
    }
}

pub(crate) fn update_hinge_motor_visuals(
    mut hinges: Query<
        (
            Entity,
            &MotorComponent,
            &JointGeometry,
            &mut Shape,
            &mut Collider,
        ),
        (
            With<AxleVisual>,
            Or<(Changed<MotorComponent>, Changed<JointGeometry>)>,
        ),
    >,
    mut motor_parts: Query<
        (
            &ChildOf,
            Option<&HingeMotorRing>,
            Option<&HingeMotorDirection>,
            &mut Visibility,
        ),
        Or<(With<HingeMotorRing>, With<HingeMotorDirection>)>,
    >,
) {
    for (hinge, motor, geometry, mut shape, mut collider) in &mut hinges {
        shape.path = GeometryBuilder::build_as(&shapes::Circle {
            radius: hinge_selection_radius(motor.enabled),
            ..Default::default()
        });
        *collider = Collider::circle(hinge_visual_radius(motor.enabled));

        for (parent, ring, direction, mut visibility) in &mut motor_parts {
            if parent.parent() != hinge {
                continue;
            }
            let reversed = motor_reversed(motor, geometry);
            let visible =
                ring.is_some() || direction.is_some_and(|direction| direction.reversed == reversed);
            *visibility = if motor.enabled && visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

fn motor_reversed(motor: &MotorComponent, geometry: &JointGeometry) -> bool {
    motor.reversed ^ geometry.geoms[0].is_none()
}

fn angular_motor_from(motor: MotorComponent, geometry: &JointGeometry) -> AngularMotor {
    let sign = if motor_reversed(&motor, geometry) {
        -1.0
    } else {
        1.0
    };
    AngularMotor {
        enabled: motor.enabled,
        target_velocity: sign * motor.vel,
        target_position: 0.0,
        max_torque: motor.torque,
        motor_model: MotorModel::AccelerationBased {
            stiffness: 0.0,
            damping: MOTOR_DAMPING,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motor_velocity_is_stored_in_radians_per_second() {
        let motor = MotorComponent {
            vel: 2.5,
            ..default()
        };
        let body = Some(Entity::PLACEHOLDER);
        let geometry = |geoms| JointGeometry {
            geoms,
            positions: [Vec2::ZERO; 2],
        };
        assert_eq!(
            angular_motor_from(motor, &geometry([body, None])).target_velocity,
            2.5
        );
        assert_eq!(
            angular_motor_from(motor, &geometry([None, body])).target_velocity,
            -2.5
        );
    }
}
