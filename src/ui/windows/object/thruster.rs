use bevy::math::EulerRot;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::egui_systems;
use crate::objects::thruster::ThrusterSettings;
use crate::ui::images::GuiIcons;
use crate::ui::{
    InitialPos, Subwindow, WindowSelectionTarget, component_slider, image_checkbox, shared_bool,
    window_matching_entities,
};

egui_systems!(ThrusterWindow::show);

#[derive(Default, Component)]
pub struct ThrusterWindow;

impl ThrusterWindow {
    pub fn show(
        mut wnds: Query<
            (
                Entity,
                Option<&ChildOf>,
                Option<&WindowSelectionTarget>,
                &mut InitialPos,
            ),
            With<ThrusterWindow>,
        >,
        settings: Query<&ThrusterSettings>,
        globals: Query<&GlobalTransform>,
        gui_icons: Res<GuiIcons>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut initial_pos) in wnds.iter_mut() {
            let targets = window_matching_entities(target, parent, &settings);
            if targets.is_empty() {
                commands.entity(id).despawn();
                continue;
            }

            egui::Window::new("Thrusters")
                .auto_sized()
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    component_slider(
                        ui,
                        commands,
                        &targets,
                        &settings,
                        |settings| settings.force,
                        |settings, value| settings.force = value,
                        0.0..=100_000.0,
                        |slider| {
                            slider
                                .logarithmic(true)
                                .smallest_positive(0.1)
                                .suffix("N")
                                .text("Force:")
                        },
                    );

                    let Some(follow_state) = shared_bool(targets.iter().filter_map(|entity| {
                        settings
                            .get(*entity)
                            .ok()
                            .map(|settings| settings.follow_geometry_rotation)
                    })) else {
                        return;
                    };
                    if let Some(follow) = image_checkbox(
                        ui,
                        &gui_icons,
                        follow_state,
                        "Follow geometry rotation",
                    ) {
                        for entity in &targets {
                            let Ok(settings) = settings.get(*entity) else {
                                continue;
                            };
                            let mut updated = *settings;
                            if !follow {
                                let Some(angle) = globals.get(*entity).ok().map(|global| {
                                    global.rotation().to_euler(EulerRot::XYZ).2
                                }) else {
                                    continue;
                                };
                                updated.fixed_angle = angle;
                            }
                            updated.follow_geometry_rotation = follow;
                            commands.entity(*entity).insert(updated);
                        }
                    }
                });
        }
    }
}
