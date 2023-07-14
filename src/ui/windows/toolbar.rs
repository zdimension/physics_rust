use crate::tools::ToolIcons;
use crate::ui::icon_button::IconButton;
use crate::ui::images::GuiIcons;
use crate::ui::{GravitySetting, RemoveTemporaryWindowsEvent, UiState};
use bevy::math::Vec2;
use bevy::prelude::{EventWriter, Local, Res, ResMut};
use bevy_egui::egui::Align2;
use bevy_egui::{egui, EguiContexts};
use bevy_rapier2d::plugin::{RapierConfiguration, TimestepMode};
use crate::systems;
use crate::ui::separator_custom::SeparatorCustom;

pub fn draw_bottom_toolbar(
    mut egui_ctx: EguiContexts,
    mut ui_state: ResMut<UiState>,
    mut rapier: ResMut<RapierConfiguration>,
    mut gravity_conf: Local<GravitySetting>,
    tool_icons: Res<ToolIcons>,
    gui_icons: Res<GuiIcons>,
    mut clear_tmp: EventWriter<RemoveTemporaryWindowsEvent>,
) {
    egui::Window::new("Tools2")
        .anchor(Align2::CENTER_BOTTOM, [0.0, -1.0])
        .title_bar(false)
        .resizable(false)
        .show(&mut egui_ctx.ctx_mut().clone(), |ui| {
            ui.horizontal(|ui| {
                let ui_state = &mut *ui_state;
                for def in ui_state.toolbox_bottom.iter() {
                    if ui
                        .add(
                            IconButton::new(egui_ctx.add_image(def.icon(&tool_icons)), 32.0)
                                .selected(ui_state.toolbox_selected.is_same(def)),
                        )
                        .clicked()
                    {
                        ui_state.toolbox_selected = *def;
                        clear_tmp.send(RemoveTemporaryWindowsEvent);
                    }
                }

                ui.add(SeparatorCustom::default());

                let playpause = ui.add(IconButton::new(
                    if rapier.physics_pipeline_active {
                        gui_icons.pause
                    } else {
                        gui_icons.play
                    },
                    32.0,
                ));

                if playpause.clicked() {
                    rapier.physics_pipeline_active = !rapier.physics_pipeline_active;
                }
                playpause.context_menu(|ui| {
                    let (max_dt, mut time_scale, substeps) = match rapier.timestep_mode {
                        TimestepMode::Variable {
                            max_dt,
                            time_scale,
                            substeps,
                        } => (max_dt, time_scale, substeps),
                        _ => unreachable!("Shouldn't happen"),
                    };
                    ui.add(
                        egui::Slider::new(&mut time_scale, 0.1..=10.0)
                            .logarithmic(true)
                            .text("Simulation speed"),
                    );
                    rapier.timestep_mode = TimestepMode::Variable {
                        max_dt,
                        time_scale,
                        substeps,
                    };
                });

                ui.add(SeparatorCustom::default());

                let gravity =
                    ui.add(IconButton::new(gui_icons.gravity, 32.0).selected(gravity_conf.enabled));
                if gravity.clicked() {
                    gravity_conf.enabled = !gravity_conf.enabled;
                    if gravity_conf.enabled {
                        rapier.gravity = gravity_conf.value;
                    } else {
                        rapier.gravity = Vec2::ZERO;
                    }
                }
            })
        });
}

systems!(draw_bottom_toolbar);