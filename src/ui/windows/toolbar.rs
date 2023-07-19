use crate::tools::ToolIcons;
use crate::ui::icon_button::IconButton;
use crate::ui::images::GuiIcons;
use crate::ui::{GravitySetting, RemoveTemporaryWindowsEvent, UiState};
use bevy::math::Vec2;
use bevy::prelude::{EventWriter, Local, Res, ResMut};
use bevy_egui::egui::Align2;
use bevy_egui::{egui, EguiContexts};
use bevy_xpbd_2d::{math::*, prelude::*};
use crate::{systems, update_changed};
use crate::ui::separator_custom::SeparatorCustom;

pub fn draw_bottom_toolbar(
    mut egui_ctx: EguiContexts,
    mut ui_state: ResMut<UiState>,
    //mut rapier: ResMut<RapierConfiguration>,
    mut gravity_conf: Local<GravitySetting>,
    tool_icons: Res<ToolIcons>,
    gui_icons: Res<GuiIcons>,
    mut clear_tmp: EventWriter<RemoveTemporaryWindowsEvent>,
    mut timescale: ResMut<PhysicsTimescale>,
    mut gravity: ResMut<Gravity>,
    mut physics: ResMut<PhysicsLoop>
) {
    egui::Window::new("Tools2")
        .anchor(Align2::CENTER_BOTTOM, [0.0, -1.0])
        .title_bar(false)
        .resizable(false)
        .show(&egui_ctx.ctx_mut().clone(), |ui| {
            ui.style_mut().spacing.item_spacing = egui::Vec2::new(3.0, 3.0);
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
                    if physics.paused {
                        gui_icons.play
                    } else {
                        gui_icons.pause
                    },
                    32.0,
                ));

                if playpause.clicked() {
                    physics.paused = !physics.paused;
                }
                playpause.context_menu(|ui| {
                    update_changed!(ui, timescale.0, 0.1..=10.0, |slider| {
                        slider.logarithmic(true).text("Simulation speed :")
                    });
                });

                ui.add(SeparatorCustom::default());

                let gravity_btn =
                    ui.add(IconButton::new(gui_icons.gravity, 32.0).selected(gravity_conf.enabled));
                if gravity_btn.clicked() {
                    gravity_conf.enabled = !gravity_conf.enabled;
                    if gravity_conf.enabled {
                        gravity.0 = gravity_conf.value;
                    } else {
                        gravity.0 = Vec2::ZERO;
                    }
                }
            })
        });
}

systems!(draw_bottom_toolbar);