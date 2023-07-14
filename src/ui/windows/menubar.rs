use bevy::prelude::{Commands, Entity, Query, Res, With};
use crate::{Despawn, systems};

use bevy_egui::egui::Align2;
use bevy_egui::{egui, EguiContexts};
use crate::ui::icon_button::IconButton;
use crate::ui::images::GuiIcons;
use crate::ui::InitialPos;


use crate::ui::separator_custom::SeparatorCustom;
use crate::ui::text_button::TextButton;
use crate::ui::windows::options::OptionsWindow;

systems!(draw_menubar);

pub fn draw_menubar(
    mut egui_ctx: EguiContexts,
    gui_icons: Res<GuiIcons>,
    mut commands: Commands,
    opt_window: Query<Entity, With<OptionsWindow>>
) {
    egui::Window::new("Menu bar")
        .anchor(Align2::LEFT_TOP, [1.0, 1.0])
        .title_bar(false)
        .resizable(false)
        .default_size(egui::Vec2::ZERO)
        .show(&mut egui_ctx.ctx_mut().clone(), |ui| {
            ui.horizontal(|ui| {
                ui.add(TextButton::new("File"));
                let opt = opt_window.get_single();
                if ui.add(IconButton::new(gui_icons.options, 16.0).selected(opt.is_ok())).clicked() {
                    match opt {
                        Ok(ent) => { commands.entity(ent).insert(Despawn::Recursive); }
                        Err(_) => { commands.spawn((OptionsWindow, InitialPos::ScreenCenter)); }
                    }
                }
                ui.add(SeparatorCustom::default().vertical());
                ui.add(TextButton::new( "physics_rust v0.1"));
            });
        });
}
