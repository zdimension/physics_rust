use std::ops::Deref;
use crate::systems;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use bevy_egui::egui::color_picker::Alpha;
use bevy_egui::egui::ecolor::HsvaGamma;
use bevy_egui::egui::{Color32, Stroke};
use bevy_egui::egui::epaint::Shadow;
use num_traits::Inv;
use strum::EnumIter;
use crate::config::AppConfig;
use crate::skin::SkinConfig;
use crate::ui::{InitialPos, Subwindow, tabs};

systems!(OptionsWindow::show, update_skin);

#[derive(Default, Component)]
pub struct OptionsWindow;

#[derive(EnumIter, Copy, Clone, Default, PartialEq)]
pub enum Tabs {
    #[default]
    Interface,
    Skin
}
impl tabs::Tab for Tabs {
    fn name(&self) -> &str {
        match self {
            Tabs::Interface => "Interface",
            Tabs::Skin => "Skin"
        }
    }
}

impl OptionsWindow {
    pub fn show(
        mut wnds: Query<(Entity, &mut InitialPos), With<OptionsWindow>>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
        current_tab: Local<Tabs>,
        mut skin: ResMut<SkinConfig>,
        mut app: ResMut<AppConfig>
    ) {
        let ctx = egui_ctx.ctx_mut();
        let Ok((id, mut initial_pos)) = wnds.get_single_mut() else { return };
        // C'EST PARCE QUE LE PIVOT EST AU CENTRE QUE ÇA S'AGRANDIT DU CENTRE ESPÈCE DE DÉBILE
        egui::Window::new("Options")
            .resizable(false)
            .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _| {
                let current_tab = current_tab;
                tabs::tabs(ui, current_tab, |ui, tab| {
                    match tab {
                        Tabs::Interface => {
                            let mut app_obj = *app;
                            let mut changed = false;

                            if ui.checkbox(&mut app_obj.tool_cursor, "Tool cursor").changed() {
                                changed = true;
                            }

                            if ui.add(
                                egui::Slider::new(&mut app_obj.ui_scale, 50..=250)
                                    .text("Menu scale:")
                                    .custom(),
                            ).changed() {
                                changed = true;
                            }

                            if ui.add(
                                egui::Slider::new(&mut app_obj.zoom_speed, 0.5..=3.0)
                                    .text("Zoom speed:")
                                    .custom(),
                            ).changed() {
                                changed = true;
                            }

                            if ui.checkbox(&mut app_obj.kinetic_panning, "Kinetic panning").changed() {
                                changed = true;
                            }

                            if changed {
                                *app = app_obj;
                            }
                        }
                        Tabs::Skin => {
                            let mut skin_obj = skin.current_skin;
                            let mut changed = false;

                            if ui.add(egui::Slider::new(&mut skin_obj.opacity, 0.0..=1.0)
                                .text("Opacity")).changed() {
                                changed = true;
                            }

                            if egui::color_picker::color_picker_color32(
                                ui,
                                &mut skin_obj.accent,
                                Alpha::OnlyBlend,
                            ) {
                                changed = true;
                            }

                            if changed {
                                skin.current_skin = skin_obj;
                            }
                        }
                    }
                });
            });
    }
}

pub fn update_skin(skin: Res<SkinConfig>, mut egui_ctx: EguiContexts) {
    if !skin.is_changed() {
        return;
    }

    let hsva = HsvaGamma::from(skin.current_skin.accent);

    let fill_color = HsvaGamma { h: hsva.h, s: hsva.s, v: hsva.v * 0.4, a: hsva.a * skin.current_skin.opacity }.into();
    let sat_factor = (1.016 * (1.0 - (1.0 + 8.6 * hsva.v.powf(0.94)).inv())); // don't ask
    let border_color = HsvaGamma {
        h: hsva.h,
        s: hsva.s * sat_factor,
        v: hsva.v * 0.85 + 0.09,
        a: hsva.a * skin.current_skin.opacity }.into();

    let selected = HsvaGamma {
        h: hsva.h,
        s: hsva.s * sat_factor,
        v: hsva.v * 0.49 + 0.40,
        a: hsva.a * 0.7
    }.into();

    let ctx = egui_ctx.ctx_mut();
    let mut style = ctx.style().deref().clone();
    style.visuals.window_fill = fill_color;
    style.visuals.panel_fill = fill_color;
    style.visuals.window_stroke.color = border_color;
    style.visuals.widgets.noninteractive.bg_stroke = style.visuals.window_stroke;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(82, 122, 82);
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    style.visuals.widgets.active = style.visuals.widgets.hovered;
    style.visuals.selection.bg_fill = selected;
    style.visuals.selection.stroke = Stroke::NONE;
    style.visuals.widgets.inactive.bg_fill = selected;
    style.visuals.window_shadow = Shadow::small_dark();
    ctx.set_style(style);
}