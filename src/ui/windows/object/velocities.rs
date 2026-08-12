use crate::egui_systems;
use crate::objects::body::{self, BodyTransform};
use crate::ui::images::GuiIcons;
use crate::ui::windows::toolbar::direction_selector;
use crate::ui::{
    InitialPos, Subwindow, WindowSelectionTarget, shared_f32, window_matching_entities,
    window_title,
};
use avian2d::prelude::{
    AngularVelocity, ColliderOf, ComputedCenterOfMass, LinearVelocity, Position, Rotation,
};
use bevy::prelude::{ChildOf, Commands, Component, Entity, Query, Res, Vec2, With, World};
use bevy_egui::{EguiContexts, egui};
use std::{f32::consts::TAU, ops::RangeInclusive};

#[derive(Default, Component)]
pub struct VelocitiesWindow;

egui_systems!(VelocitiesWindow::show);

fn edit_velocities(
    commands: &mut Commands,
    targets: &[Entity],
    edit: impl Fn(Vec2) -> Vec2 + Send + 'static,
) {
    let targets = targets.to_vec();
    commands.queue(move |world: &mut World| {
        for target in targets {
            if let Some(velocity) = body::point_velocity(world, target) {
                body::set_point_velocity(world, target, edit(velocity));
            }
        }
    });
}

fn edit_bodies(commands: &mut Commands, targets: &[Entity], angular: Option<f32>) {
    let targets = targets.to_vec();
    commands.queue(move |world: &mut World| {
        let mut bodies = Vec::new();
        for target in targets {
            if let Some(body) = body::entity(world, target)
                && !bodies.contains(&body)
            {
                bodies.push(body);
            }
        }
        for body in bodies {
            let mut body = world.entity_mut(body);
            if let Some(angular) = angular {
                body.insert(AngularVelocity(angular));
            } else {
                body.insert((LinearVelocity::ZERO, AngularVelocity::ZERO));
            }
        }
    });
}

fn with_speed(velocity: Vec2, speed: f32) -> Vec2 {
    Vec2::from_angle(if velocity == Vec2::ZERO {
        0.0
    } else {
        velocity.to_angle()
    }) * speed
}

fn point_velocity(
    entity: Entity,
    geometries: &Query<(&ColliderOf, &BodyTransform)>,
    bodies: &Query<(
        &Position,
        &Rotation,
        Option<&ComputedCenterOfMass>,
        &LinearVelocity,
        &AngularVelocity,
    )>,
) -> Option<Vec2> {
    let (link, local) = geometries.get(entity).ok()?;
    let (position, rotation, center, linear, angular) = bodies.get(link.body).ok()?;
    let point = body::world_point((position.0, *rotation), local.translation);
    let center = body::world_point(
        (position.0, *rotation),
        center.map_or(Vec2::ZERO, |center| center.0),
    );
    Some(body::velocity_at_point(center, linear.0, angular.0, point))
}

fn shared(values: impl IntoIterator<Item = f32>) -> f32 {
    shared_f32(values).unwrap_or(f32::NAN)
}

fn slider(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: RangeInclusive<f32>,
    text: &str,
    suffix: &str,
) -> bool {
    ui.add(
        egui::Slider::new(value, range)
            .suffix(suffix)
            .text(text)
            .custom(),
    )
    .changed()
}

impl VelocitiesWindow {
    pub fn show(
        mut windows: Query<
            (
                Entity,
                Option<&ChildOf>,
                Option<&WindowSelectionTarget>,
                &mut InitialPos,
            ),
            With<Self>,
        >,
        geometries: Query<(&ColliderOf, &BodyTransform)>,
        bodies: Query<(
            &Position,
            &Rotation,
            Option<&ComputedCenterOfMass>,
            &LinearVelocity,
            &AngularVelocity,
        )>,
        icons: Res<GuiIcons>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut initial_pos) in windows.iter_mut() {
            let targets = window_matching_entities(target, parent, &geometries);
            if targets.is_empty() {
                commands.entity(id).despawn();
                continue;
            }
            let velocities = targets
                .iter()
                .filter_map(|&entity| point_velocity(entity, &geometries, &bodies))
                .collect::<Vec<_>>();
            let angular = shared(targets.iter().filter_map(|entity| {
                let (link, _) = geometries.get(*entity).ok()?;
                Some(bodies.get(link.body).ok()?.4.0)
            }));

            egui::Window::new(window_title(target, "Velocities"))
                .auto_sized()
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    if ui.button("Freeze velocities").clicked() {
                        edit_bodies(commands, &targets, None);
                    }

                    let mut speed = shared(velocities.iter().map(|velocity| velocity.length()));
                    if slider(ui, &mut speed, 0.0..=25.0, "Speed:", " m/s") {
                        edit_velocities(commands, &targets, move |velocity| {
                            with_speed(velocity, speed)
                        });
                    }

                    ui.horizontal(|ui| {
                        let mut angle = shared(
                            velocities
                                .iter()
                                .map(|velocity| velocity.to_angle().to_degrees()),
                        );
                        if slider(ui, &mut angle, -180.0..=180.0, "Angle:", "°") {
                            let angle = angle.to_radians();
                            edit_velocities(commands, &targets, move |velocity| {
                                Vec2::from_angle(angle) * velocity.length()
                            });
                        }
                        let mut direction = if angle.is_finite() {
                            angle.to_radians()
                        } else {
                            0.0
                        };
                        if direction_selector(ui, &icons, &mut direction).changed() {
                            edit_velocities(commands, &targets, move |velocity| {
                                Vec2::from_angle(direction) * velocity.length()
                            });
                        }
                    });

                    for (label, component) in [("Velocity (X):", 0), ("Velocity (Y):", 1)] {
                        let mut value =
                            shared(velocities.iter().map(|velocity| velocity[component]));
                        if slider(ui, &mut value, -25.0..=25.0, label, " m/s") {
                            edit_velocities(commands, &targets, move |mut velocity| {
                                velocity[component] = value;
                                velocity
                            });
                        }
                    }

                    ui.add_space(10.0);
                    let mut angular = angular;
                    if slider(ui, &mut angular, -TAU..=TAU, "Angular velocity:", " rad/s") {
                        edit_bodies(commands, &targets, Some(angular));
                    }
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_speed_on_a_stopped_object_uses_zero_radians() {
        assert_eq!(with_speed(Vec2::ZERO, 5.0), Vec2::X * 5.0);
        assert!((with_speed(Vec2::Y, 5.0) - Vec2::Y * 5.0).length() < 1e-6);
    }
}
