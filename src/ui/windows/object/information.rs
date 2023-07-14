use crate::measures::KineticEnergy;
use crate::ui::{InitialPos, Subwindow};
use bevy::hierarchy::Parent;
use bevy::prelude::{Commands, Component, Entity, Query, Res, Transform, With};
use bevy_egui::egui::Ui;
use bevy_egui::{egui, EguiContexts};
use bevy_rapier2d::dynamics::{ReadMassProperties, Velocity};
use bevy_rapier2d::geometry::ColliderMassProperties;
use bevy_rapier2d::plugin::RapierConfiguration;
use crate::systems;

systems!(InformationWindow::show);

#[derive(Default, Component)]
pub struct InformationWindow;

impl InformationWindow {
    pub fn show(
        mut wnds: Query<(Entity, &Parent, &mut InitialPos), With<InformationWindow>>,
        ents: Query<(
            Option<&Transform>,
            Option<&ReadMassProperties>,
            Option<&Velocity>,
            Option<&ColliderMassProperties>,
            Option<&KineticEnergy>,
        )>,
        rapier_conf: Res<RapierConfiguration>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let (xform, mass, vel, coll_mass, kine) = ents.get(parent.get()).unwrap();
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
                        if let Some(ReadMassProperties(mass)) = mass {
                            line(ui, "Mass", format!("{:.3} kg", mass.mass));

                            line(
                                ui,
                                "Moment of inertia",
                                format!("{:.3} kgm²", mass.principal_inertia),
                            );
                        }

                        if let Some(props) = coll_mass {
                            line(ui, "Collider mass", format!("{:?}", props));
                        }

                        if let Some(xform) = xform {
                            line(
                                ui,
                                "Position",
                                format!(
                                    "[x={:.3}, y={:.3}] m",
                                    xform.translation.x, xform.translation.y
                                ),
                            );
                        }

                        if let Some(vel) = vel {
                            line(
                                ui,
                                "Velocity",
                                format!("[x={:.3}, y={:.3}] m/s", vel.linvel.x, vel.linvel.y),
                            );

                            line(ui, "Angular velocity", format!("{:.3} rad/s", vel.angvel));
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

                        if let Some(ReadMassProperties(mass)) = mass {
                            let pot =
                                mass.mass * -rapier_conf.gravity.y * xform.unwrap().translation.y;
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
