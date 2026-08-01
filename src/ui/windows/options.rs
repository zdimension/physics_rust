use std::ops::Deref;

use crate::config::AppConfig;
use crate::egui_systems;
use crate::skin::SkinConfig;
use crate::ui::images::GuiIcons;
use crate::ui::{InitialPos, Subwindow, bool_checkbox, tabs};
use bevy::prelude::*;
use bevy_egui::egui::color_picker::Alpha;
use bevy_egui::egui::ecolor::HsvaGamma;
use bevy_egui::egui::epaint::Shadow;
use bevy_egui::egui::{Color32, Stroke};
use bevy_egui::{EguiContexts, egui};
use num_traits::Inv;
use strum::EnumIter;

egui_systems!(OptionsWindow::show, update_skin);

#[derive(Default, Component)]
pub struct OptionsWindow;

#[derive(EnumIter, Copy, Clone, Default, PartialEq)]
pub enum Tabs {
    #[default]
    Interface,
    Skin,
}
impl tabs::Tab for Tabs {
    fn name(&self) -> &str {
        match self {
            Tabs::Interface => "Interface",
            Tabs::Skin => "Skin",
        }
    }
}

impl OptionsWindow {
    pub fn show(
        mut wnds: Query<(Entity, &mut InitialPos), With<OptionsWindow>>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
        current_tab: Local<Tabs>,
        mut pending_ui_scale: Local<Option<i32>>,
        gui_icons: Res<GuiIcons>,
        mut skin: ResMut<SkinConfig>,
        mut app: ResMut<AppConfig>,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        let Ok((id, mut initial_pos)) = wnds.single_mut() else {
            return;
        };
        // C'EST PARCE QUE LE PIVOT EST AU CENTRE QUE Ã‡A S'AGRANDIT DU CENTRE ESPÃˆCE DE DÃ‰BILE
        egui::Window::new("Options").auto_sized().subwindow(
            id,
            ctx,
            &mut initial_pos,
            &mut commands,
            |ui, _| {
                let current_tab = current_tab;
                tabs::tabs(ui, current_tab, |ui, tab| match tab {
                    Tabs::Interface => {
                        let mut app_obj = *app;
                        let mut changed = false;

                        if bool_checkbox(ui, &gui_icons, &mut app_obj.tool_cursor, "Tool cursor") {
                            changed = true;
                        }

                        let mut displayed_ui_scale =
                            pending_ui_scale.unwrap_or(app_obj.ui_scale);
                        let ui_scale_response = ui.add(
                            egui::Slider::new(&mut displayed_ui_scale, 50..=250)
                                .clamping(egui::SliderClamping::Always)
                                .text("Menu scale:")
                                .suffix("%")
                                .step_by(1.0)
                                .custom(),
                        );
                        if ui_scale_response.drag_stopped() {
                            app_obj.ui_scale = displayed_ui_scale;
                            *pending_ui_scale = None;
                            changed = true;
                        } else if ui_scale_response.dragged() {
                            *pending_ui_scale = Some(displayed_ui_scale);
                        } else if ui_scale_response.changed() {
                            app_obj.ui_scale = displayed_ui_scale;
                            changed = true;
                        }

                        if ui
                            .add(
                                egui::Slider::new(&mut app_obj.zoom_speed, 0.5..=3.0)
                                    .text("Zoom speed:")
                                    .custom(),
                            )
                            .changed()
                        {
                            changed = true;
                        }

                        if bool_checkbox(
                            ui,
                            &gui_icons,
                            &mut app_obj.kinetic_panning,
                            "Kinetic panning",
                        ) {
                            changed = true;
                        }

                        if changed {
                            *app = app_obj;
                        }
                    }
                    Tabs::Skin => {
                        let mut skin_obj = skin.current_skin;
                        let mut changed = false;

                        if ui
                            .add(
                                egui::Slider::new(&mut skin_obj.opacity, 0.0..=1.0)
                                    .clamping(egui::SliderClamping::Always)
                                    .text("Opacity"),
                            )
                            .changed()
                        {
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
                });
            },
        );
    }
}

pub fn update_skin(skin: Res<SkinConfig>, mut egui_ctx: EguiContexts) {
    if !skin.is_changed() {
        return;
    }

    let hsva = HsvaGamma::from(skin.current_skin.accent);

    let fill_color = HsvaGamma {
        h: hsva.h,
        s: hsva.s,
        v: hsva.v * 0.4,
        a: hsva.a * skin.current_skin.opacity,
    }
    .into();
    let sat_factor = 1.016 * (1.0 - (1.0 + 8.6 * hsva.v.powf(0.94)).inv()); // don't ask
    let border_color = HsvaGamma {
        h: hsva.h,
        s: hsva.s * sat_factor,
        v: hsva.v * 0.85 + 0.09,
        a: hsva.a * skin.current_skin.opacity,
    }
    .into();

    let selected = HsvaGamma {
        h: hsva.h,
        s: hsva.s * sat_factor,
        v: hsva.v * 0.49 + 0.60,
        a: hsva.a * 0.7,
    }
    .into();

    let ctx = egui_ctx.ctx_mut().expect("primary egui context");
    let mut style = ctx.global_style().deref().clone();
    style.visuals.window_fill = fill_color;
    style.visuals.panel_fill = fill_color;
    style.visuals.window_stroke.color = border_color;
    style.visuals.widgets.noninteractive.bg_stroke = style.visuals.window_stroke;
    //style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(102, 153, 102);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(82, 122, 82);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(82, 122, 82);
    //style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(6, 13, 1));
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;

    style.visuals.widgets.active = style.visuals.widgets.hovered;
    style.visuals.widgets.active.bg_stroke = Stroke::new(2.0, Color32::from_rgb(76, 127, 76));

    style.visuals.selection.bg_fill = selected;
    style.visuals.selection.stroke = Stroke::NONE;

    style.visuals.widgets.inactive.bg_fill = selected;
    style.visuals.window_shadow = Shadow {
        offset: [6, 10],
        blur: 8,
        spread: 0,
        color: Color32::from_black_alpha(96),
    };
    ctx.set_global_style(style);
}
