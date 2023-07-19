use crate::measures::{GravityEnergy, KineticEnergy};
use crate::ui::{InitialPos, Subwindow};
use bevy::hierarchy::Parent;
use bevy::prelude::{Commands, Component, Entity, GlobalTransform, Query, Res, Transform, With};
use bevy_egui::egui::Ui;
use bevy_egui::{egui, EguiContexts};
use bevy_xpbd_2d::{math::*, prelude::*};
use bevy_xpbd_2d::{math::*, prelude::*};
use bevy_xpbd_2d::{math::*, prelude::*};
use crate::systems;

systems!(InformationWindow::show);

#[derive(Default, Component)]
pub struct InformationWindow;

impl InformationWindow {
    pub fn show(
        mut wnds: Query<(Entity, &Parent, &mut InitialPos), With<InformationWindow>>,
        ents: Query<(
            Option<&Position>,
            Option<&GravityEnergy>,
            Option<&LinearVelocity>,
            Option<&AngularVelocity>,
            Option<&ColliderMassProperties>,
            Option<&KineticEnergy>,
        )>,
        gravity: Res<Gravity>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let (xform, grav, linvel, angvel, coll_mass, kine) = ents.get(parent.get()).unwrap();
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
                        if let Some(cmp) = coll_mass {
                            line(ui, "Mass", format!("{:.3} kg", cmp.mass.0));

                            line(
                                ui,
                                "Moment of inertia",
                                format!("{:.3} kgm²", cmp.inertia.0),
                            );
                        }

                        if let Some(xform) = xform {
                            line(
                                ui,
                                "Position",
                                format!(
                                    "[x={:.3}, y={:.3}] m",
                                    xform.0.x, xform.0.y
                                ),
                            );
                        }

                        if let Some(vel) = linvel {
                            line(
                                ui,
                                "Velocity",
                                format!("[x={:.3}, y={:.3}] m/s", vel.0.x, vel.0.y),
                            );
                        }
                        if let Some(vel) = angvel {
                            line(ui, "Angular velocity", format!("{:.3} rad/s", vel.0));
                        }
                    });
                    ui.separator();
                    egui::Grid::new("info grid 2").striped(true).show(ui, |ui| {
                        let mut total = 0.0;

                        if let Some(KineticEnergy { linear, angular }) = kine {
                            line(ui, "Kinetic linear energy", format!("{:.3} J", linear));
                            line(ui, "Kinetic angular energy", format!("{:.3} J", angular));
                            total += linear + angular;
                        }

                        if let Some(GravityEnergy { energy }) = grav {
                            let pot = energy;
                            line(ui, "Potential energy (gravity)", format!("{:.3} J", pot)); // todo: nonvertical gravity
                            total += pot;
                        }

                        line(ui, "Energy (total)", format!("{:.3} J", total));
                    });
                },
            );
        }
    }
}
