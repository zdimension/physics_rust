use crate::hsva_to_rgba;
use crate::objects::ColorComponent;
use crate::palette::PaletteConfig;
use crate::ui::{BevyIdThing, InitialPos, Subwindow};
use bevy::prelude::*;
use bevy_egui::egui::ecolor::Hsva;
use bevy_egui::{egui, EguiContext};

#[derive(Default, Component)]
pub struct BackgroundWindow;

impl BackgroundWindow {
    pub fn show(
        mut wnds: Query<(Entity, &mut InitialPos), With<BackgroundWindow>>,
        _ents: Query<&mut ColorComponent>,
        mut egui_ctx: ResMut<EguiContext>,
        mut commands: Commands,
        mut palette: ResMut<PaletteConfig>,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, mut initial_pos) in wnds.iter_mut() {
            let Color::RgbaLinear { red, green, blue, alpha } = palette.current_palette.sky_color else { unreachable!("Sky color is not RGBA") };
            let mut color = Hsva::from_rgba_premultiplied(red, green, blue, alpha);
            egui::Window::new("Background")
                .resizable(false)
                .default_size(egui::Vec2::ZERO)
                .id_bevy(id)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    egui::color_picker::color_picker_hsva_2d(
                        ui,
                        &mut color,
                        egui::color_picker::Alpha::OnlyBlend,
                    );
                    palette.current_palette.sky_color = hsva_to_rgba(color);
                });
        }
    }
}
