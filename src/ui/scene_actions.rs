use crate::ui::icon_button::IconButton;
use crate::ui::separator_custom::SeparatorCustom;
use crate::ui::RemoveTemporaryWindowsEvent;
use crate::{GuiIcons, ToolIcons, UiState};
use bevy::prelude::{EventWriter, Res, ResMut};
use bevy_egui::egui::Align2;
use bevy_egui::{egui, EguiContext};

pub fn draw_scene_actions(
    mut egui_ctx: ResMut<EguiContext>,
    mut ui_state: ResMut<UiState>,
    gui_icons: Res<GuiIcons>,
    mut clear_tmp: EventWriter<RemoveTemporaryWindowsEvent>,
) {
    egui::Window::new("Scene actions")
        .anchor(Align2::LEFT_TOP, [0.0, 0.0])
        .title_bar(false)
        .resizable(false)
        .default_size(egui::Vec2::ZERO)
        .show(egui_ctx.clone().ctx_mut(), |ui| {
            ui.vertical(|ui| {
                if ui.add(IconButton::new(gui_icons.new, 32.0)).clicked() {}
                if ui.add(IconButton::new(gui_icons.save, 32.0)).clicked() {}
                if ui.add(IconButton::new(gui_icons.open, 32.0)).clicked() {}
            });
        });
}
