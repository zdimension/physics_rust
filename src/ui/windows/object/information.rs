use crate::egui_systems;
use crate::measures::{AggregateMeasureData, aggregate_measures};
use crate::ui::{InitialPos, Subwindow, WindowSelectionTarget, window_target_entities};
use avian2d::prelude::*;
use bevy::prelude::{ChildOf, Commands, Component, Entity, Query, Res, With};
use bevy_egui::egui::Ui;
use bevy_egui::{EguiContexts, egui};

egui_systems!(InformationWindow::show);

#[derive(Default, Component)]
pub struct InformationWindow;

impl InformationWindow {
    pub fn show(
        mut wnds: Query<
            (
                Entity,
                Option<&ChildOf>,
                Option<&WindowSelectionTarget>,
                &mut InitialPos,
            ),
            With<InformationWindow>,
        >,
        ents: Query<AggregateMeasureData>,
        body_positions: Query<(&Position, &Rotation)>,
        gravity: Res<Gravity>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut initial_pos) in wnds.iter_mut() {
            let targets = window_target_entities(target, parent);
            let aggregate = aggregate_measures(targets, &ents, &body_positions, gravity.0);
            egui::Window::new("info").subwindow(
                id,
                ctx,
                &mut initial_pos,
                &mut commands,
                |ui, _commands| {
                    fn line(ui: &mut Ui, label: &'static str, val: String) {
                        ui.label(label);
                        ui.label(val);
                        ui.end_row();
                    }
                    egui::Grid::new("info grid").striped(true).show(ui, |ui| {
                        if let Some(mass) = aggregate.mass {
                            line(ui, "Mass", format!("{:.3} kg", mass));
                        }
                        if let Some(inertia) = aggregate.angular_inertia {
                            line(ui, "Moment of inertia", format!("{:.3} kgÂ·mÂ²", inertia));
                        }
                        if let Some(pos) = aggregate.position {
                            line(ui, "Position", format!("[x={:.3}, y={:.3}] m", pos.x, pos.y));
                        }
                        if let Some(vel) = aggregate.velocity {
                            line(ui, "Velocity", format!("[x={:.3}, y={:.3}] m/s", vel.x, vel.y));
                        }
                        if let Some(vel) = aggregate.angular_velocity {
                            line(ui, "Angular velocity", format!("{:.3} rad/s", vel));
                        }
                        if let Some(momentum) = aggregate.momentum {
                            line(
                                ui,
                                "Momentum",
                                format!(
                                    "[x={:.3}, y={:.3}] NÂ·s",
                                    momentum.linear.x, momentum.linear.y
                                ),
                            );
                            line(ui, "Angular momentum", format!("{:.3} JÂ·s", momentum.angular));
                        }
                    });
                    ui.separator();
                    egui::Grid::new("info grid 2").striped(true).show(ui, |ui| {
                        if let Some(energy) = aggregate.kinetic_linear {
                            line(ui, "Kinetic linear energy", format!("{:.3} J", energy));
                        }
                        if let Some(energy) = aggregate.kinetic_angular {
                            line(ui, "Kinetic angular energy", format!("{:.3} J", energy));
                        }
                        if let Some(energy) = aggregate.gravity_energy {
                            line(ui, "Potential energy (gravity)", format!("{:.3} J", energy));
                        }
                        if let Some(energy) = aggregate.spring_energy {
                            line(ui, "Potential enery (spring)", format!("{:.3} J", energy));
                        }
                        if let Some(total) = aggregate.energy_total() {
                            line(ui, "Energy (total)", format!("{:.3} J", total));
                        }
                    });
                },
            );
        }
    }
}
