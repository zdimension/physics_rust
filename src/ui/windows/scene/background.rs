use crate::{hsva_to_rgba, systems};
use crate::objects::ColorComponent;
use crate::palette::PaletteConfig;
use crate::ui::{BevyIdThing, InitialPos, Subwindow};
use bevy::prelude::*;
use bevy_egui::egui::ecolor::Hsva;
use bevy_egui::{egui, EguiContexts};

systems!(BackgroundWindow::show);

#[derive(Default, Component)]
pub struct BackgroundWindow;

impl BackgroundWindow {
    pub fn show(
        mut wnds: Query<(Entity, &mut InitialPos), With<BackgroundWindow>>,
        _ents: Query<&mut ColorComponent>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
        mut palette: ResMut<PaletteConfig>,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, mut initial_pos) in wnds.iter_mut() {
            let [red, green, blue, alpha] = palette.current_palette.sky_color.as_linear_rgba_f32() else {
                panic!("color: {:?}", palette.current_palette.sky_color);
                unreachable!("Sky color is not RGBA");
            };
            let mut color = Hsva::from_rgba_premultiplied(red, green, blue, alpha);
            egui::Window::new("Background")
                .resizable(false)
                .default_size(egui::Vec2::ZERO)
                .id_bevy(id)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    if egui::color_picker::color_picker_hsva_2d(
                        ui,
                        &mut color,
                        egui::color_picker::Alpha::OnlyBlend,
                    ) {
                        palette.current_palette.sky_color = hsva_to_rgba(color);
                    }
                });
        }
    }
}
