use crate::palette::{PaletteConfig, PaletteList};
use crate::{Despawn, systems};
use bevy::prelude::*;
use bevy_egui::egui::Align2;
use bevy_egui::{egui, EguiContexts};
use crate::ui::menu_item::MenuItem;
use crate::ui::icon_button::IconButton;
use crate::ui::images::GuiIcons;
use crate::ui::{BevyIdThing, InitialPos, RemoveTemporaryWindowsEvent, Subwindow, UiState};
use crate::ui::separator_custom::SeparatorCustom;
use crate::ui::text_button::TextButton;

systems!(draw_menubar);

pub fn draw_menubar(
    mut egui_ctx: EguiContexts,
    _ui_state: ResMut<UiState>,
    gui_icons: Res<GuiIcons>,
    _clear_tmp: EventWriter<RemoveTemporaryWindowsEvent>,
    mut commands: Commands,
) {
    egui::Window::new("Menu bar")
        .anchor(Align2::LEFT_TOP, [0.0, 0.0])
        .title_bar(false)
        .resizable(false)
        .default_size(egui::Vec2::ZERO)
        .show(&mut egui_ctx.ctx_mut().clone(), |ui| {
            ui.horizontal(|ui| {
                ui.add(TextButton::new("File"));
                ui.add(SeparatorCustom::default().vertical());
                ui.add(TextButton::new( "physics_rust v0.1"));
            });
        });
}
